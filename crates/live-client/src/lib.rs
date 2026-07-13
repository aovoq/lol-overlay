//! Client for Riot's **Live Client Data API**.
//!
//! While a game is running, the client exposes a read-only REST API on
//! `https://127.0.0.1:2999/liveclientdata/*` with a self-signed certificate.
//! It needs no authentication and exposes only data the local player can
//! already see, which is why reading it is allowed under Riot's ToS.
//!
//! Docs: <https://developer.riotgames.com/docs/lol#game-client-api>

use serde::Deserialize;
use std::time::Duration;

pub use overlay_types::{EnemyChampion, GamePlayer, GameSnapshot};

const BASE: &str = "https://127.0.0.1:2999/liveclientdata";
const TIMEOUT: Duration = Duration::from_secs(8);
const RETRY_ATTEMPTS: usize = 2;
const RETRY_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, thiserror::Error)]
pub enum LiveClientError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, LiveClientError>;

/// One participant as returned by `/allgamedata`'s `allPlayers`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawPlayer {
    /// Localized display name (e.g. "タロン" on a JP client).
    #[serde(default)]
    pub champion_name: String,
    /// Locale-independent id, e.g. "game_character_displayname_Talon".
    #[serde(default)]
    pub raw_champion_name: String,
    /// `"ORDER"` (blue) or `"CHAOS"` (red).
    #[serde(default)]
    pub team: String,
    #[serde(default)]
    pub position: String,
    /// Riot ID game name, e.g. `"Faker#KR1"`. Newer clients use `riotId`.
    #[serde(default)]
    pub riot_id: String,
    #[serde(default)]
    pub items: Vec<RawItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawItem {
    #[serde(default)]
    pub item_id: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawActivePlayer {
    /// Riot ID, used to locate ourselves inside `allPlayers`.
    #[serde(default)]
    pub riot_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawGameData {
    #[serde(default)]
    pub game_mode: String,
    #[serde(default)]
    pub game_time: f64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllGameData {
    #[serde(default)]
    pub active_player: RawActivePlayer,
    #[serde(default)]
    pub all_players: Vec<RawPlayer>,
    #[serde(default)]
    pub game_data: RawGameData,
}

/// Pull the English champion name out of `rawChampionName`
/// ("game_character_displayname_Talon" -> "Talon"); fall back to `display`.
fn english_name(raw: &str, display: &str) -> String {
    raw.rsplit('_')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(display)
        .to_string()
}

pub struct LiveClient {
    http: reqwest::Client,
}

impl LiveClient {
    pub fn new() -> Result<Self> {
        // The cert is Riot's self-signed `riotgames.pem`; bundling it adds no
        // security on loopback, so we simply skip verification for this client.
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(TIMEOUT)
            .build()?;
        Ok(Self { http })
    }

    async fn raw(&self) -> Result<AllGameData> {
        let text = self
            .http
            .get(format!("{BASE}/allgamedata"))
            .send_with_retry()
            .await?
            .error_for_status()?
            .text()
            .await?;
        // Decode separately from the read so field-level serde errors are clear.
        Ok(serde_json::from_str::<AllGameData>(&text)?)
    }

    /// Fetch the current snapshot, or `None` if the Live Client API is unavailable.
    ///
    /// HTTP/connect failures are treated as "not in a game"; parse failures are
    /// returned so the app can surface schema drift instead of silently looking
    /// idle forever.
    pub async fn snapshot(&self) -> Result<Option<GameSnapshot>> {
        let data = match self.raw().await {
            Ok(data) => data,
            Err(LiveClientError::Http(e)) if live_client_unavailable(&e) => return Ok(None),
            Err(e) => return Err(e),
        };

        // Locate ourselves to determine which team is hostile.
        let me = data
            .all_players
            .iter()
            .find(|p| p.riot_id == data.active_player.riot_id);
        let (self_champion, self_raw_name, self_position, my_team) = match me {
            Some(p) => (
                p.champion_name.clone(),
                english_name(&p.raw_champion_name, &p.champion_name),
                p.position.clone(),
                p.team.clone(),
            ),
            // Spectator or pre-spawn; still report mode so the UI can react.
            None => (String::new(), String::new(), String::new(), String::new()),
        };

        let mut enemies = Vec::new();
        let mut allies = Vec::new();
        let mut players = Vec::new();
        for p in &data.all_players {
            let ally = !my_team.is_empty() && p.team == my_team;
            players.push(GamePlayer {
                riot_id: p.riot_id.clone(),
                name: p.champion_name.clone(),
                raw_name: english_name(&p.raw_champion_name, &p.champion_name),
                position: p.position.clone(),
                ally,
            });
            if ally {
                if p.champion_name != self_champion {
                    allies.push(p.champion_name.clone());
                }
            } else {
                enemies.push(EnemyChampion {
                    name: p.champion_name.clone(),
                    raw_name: english_name(&p.raw_champion_name, &p.champion_name),
                    position: p.position.clone(),
                    items: p.items.iter().map(|i| i.item_id).collect(),
                });
            }
        }

        Ok(Some(GameSnapshot {
            game_mode: data.game_data.game_mode,
            game_time: data.game_data.game_time,
            self_champion,
            self_raw_name,
            self_position,
            enemies,
            allies,
            players,
        }))
    }
}

fn live_client_unavailable(error: &reqwest::Error) -> bool {
    error.is_connect()
        || error.is_timeout()
        || error.status().is_some_and(|status| status.as_u16() == 404)
}

trait RequestBuilderRetryExt {
    async fn send_with_retry(self) -> std::result::Result<reqwest::Response, reqwest::Error>;
}

impl RequestBuilderRetryExt for reqwest::RequestBuilder {
    async fn send_with_retry(self) -> std::result::Result<reqwest::Response, reqwest::Error> {
        let request = self;
        let mut attempt = 0;
        loop {
            let Some(next) = request.try_clone() else {
                return request.send().await;
            };
            match next.send().await {
                Ok(response) if response.status().is_server_error() && attempt < RETRY_ATTEMPTS => {
                    attempt += 1;
                    tokio::time::sleep(RETRY_DELAY * attempt as u32).await;
                }
                Err(err)
                    if (err.is_connect() || err.is_timeout() || err.is_request())
                        && attempt < RETRY_ATTEMPTS =>
                {
                    attempt += 1;
                    tokio::time::sleep(RETRY_DELAY * attempt as u32).await;
                }
                result => return result,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::english_name;

    #[test]
    fn english_name_extracts_raw_champion_suffix() {
        assert_eq!(
            english_name("game_character_displayname_Talon", "タロン"),
            "Talon"
        );
        assert_eq!(english_name("", "Cho'Gath"), "Cho'Gath");
    }
}
