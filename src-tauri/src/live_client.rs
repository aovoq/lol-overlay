//! Client for Riot's **Live Client Data API**.
//!
//! While a game is running, the client exposes a read-only REST API on
//! `https://127.0.0.1:2999/liveclientdata/*` with a self-signed certificate.
//! It needs no authentication and exposes only data the local player can
//! already see, which is why reading it is allowed under Riot's ToS.
//!
//! Docs: <https://developer.riotgames.com/docs/lol#game-client-api>

use serde::{Deserialize, Serialize};

use crate::error::Result;

const BASE: &str = "https://127.0.0.1:2999/liveclientdata";

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

/// A champion on the enemy team, normalized for the rest of the app.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyChampion {
    /// Localized display name (for the UI).
    pub name: String,
    /// English name for locale-independent logic (e.g. "Talon", "Chogath").
    pub raw_name: String,
    pub position: String,
    pub items: Vec<i64>,
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

/// The slice of game state we care about: who we are and who we face.
#[derive(Debug, Clone, Serialize)]
pub struct GameSnapshot {
    pub game_mode: String,
    pub game_time: f64,
    /// Localized display name of our champion (for the UI).
    pub self_champion: String,
    /// English name of our champion (e.g. "Talon"), for id lookups in the
    /// data layer. Empty when spectating / before spawn.
    pub self_raw_name: String,
    pub self_position: String,
    pub enemies: Vec<EnemyChampion>,
    pub allies: Vec<String>,
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
            .build()?;
        Ok(Self { http })
    }

    async fn raw(&self) -> Result<AllGameData> {
        let text = self
            .http
            .get(format!("{BASE}/allgamedata"))
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        // Decode separately from the read so field-level serde errors are clear.
        Ok(serde_json::from_str::<AllGameData>(&text)?)
    }

    /// Fetch the current snapshot, or `None` if not in a game.
    pub async fn snapshot(&self) -> Option<GameSnapshot> {
        let data = self.raw().await.ok()?;

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
        for p in &data.all_players {
            if p.team == my_team {
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

        Some(GameSnapshot {
            game_mode: data.game_data.game_mode,
            game_time: data.game_data.game_time,
            self_champion,
            self_raw_name,
            self_position,
            enemies,
            allies,
        })
    }
}
