use irelia::rest::LcuClient;
use overlay_types::{Phase, RecentGame, RunePagePayload, SummonerInfo};
use serde::Serialize;
use serde_json::Value;

use crate::error::LcuError;

/// Map any `irelia` error into our crate error.
fn ie<E: std::fmt::Debug>(e: E) -> LcuError {
    LcuError::Other(format!("{e:?}"))
}

/// The logged-in summoner with their solo-queue rank. Errors if the client
/// isn't running; rank fields stay empty/0 if ranked stats aren't available
/// yet (e.g. right after login).
pub async fn fetch_summoner() -> Result<SummonerInfo, LcuError> {
    let client = LcuClient::connect().map_err(ie)?;
    let me: Value = client
        .get("/lol-summoner/v1/current-summoner")
        .await
        .map_err(ie)?;

    let str_of =
        |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    let int_of = |v: &Value, key: &str| v.get(key).and_then(Value::as_i64).unwrap_or(0);

    let ranked: Value = client
        .get("/lol-ranked/v1/current-ranked-stats")
        .await
        .unwrap_or(Value::Null);
    let solo = ranked
        .pointer("/queueMap/RANKED_SOLO_5x5")
        .cloned()
        .unwrap_or(Value::Null);

    Ok(SummonerInfo {
        game_name: str_of(&me, "gameName"),
        tag_line: str_of(&me, "tagLine"),
        level: int_of(&me, "summonerLevel"),
        profile_icon_id: int_of(&me, "profileIconId"),
        solo_tier: str_of(&solo, "tier"),
        solo_division: str_of(&solo, "division"),
        solo_lp: int_of(&solo, "leaguePoints"),
        solo_wins: int_of(&solo, "wins"),
        solo_losses: int_of(&solo, "losses"),
    })
}

const REMAKE_MAX_DURATION_SECONDS: i64 = 5 * 60;

/// The local player's most recent games (newest first). The current-summoner
/// match-history endpoint returns only our own participant per game.
pub async fn fetch_recent_matches(count: usize) -> Result<Vec<RecentGame>, LcuError> {
    let client = LcuClient::connect().map_err(ie)?;
    let fetch_count = count.saturating_mul(3).max(count);
    let resp: Value = client
        .get(format!(
            "/lol-match-history/v1/products/lol/current-summoner/matches?begIndex=0&endIndex={fetch_count}"
        ))
        .await
        .map_err(ie)?;

    Ok(parse_recent_matches(&resp, count))
}

pub(crate) fn parse_recent_matches(resp: &Value, count: usize) -> Vec<RecentGame> {
    let mut games: Vec<RecentGame> = resp
        .pointer("/games/games")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|g| {
            let int = |v: &Value, key: &str| v.get(key).and_then(Value::as_i64).unwrap_or(0);
            let queue_id = int(g, "queueId");
            if queue_id <= 0 {
                return None; // custom / practice tool
            }
            let stats = g.pointer("/participants/0/stats")?;
            if is_remake(g, stats) {
                return None;
            }
            Some(RecentGame {
                champion_id: g
                    .pointer("/participants/0/championId")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                win: stats.get("win").and_then(Value::as_bool).unwrap_or(false),
                kills: int(stats, "kills"),
                deaths: int(stats, "deaths"),
                assists: int(stats, "assists"),
                queue_id,
                game_creation: int(g, "gameCreation"),
            })
        })
        .collect();
    games.sort_by_key(|g| std::cmp::Reverse(g.game_creation));
    games.truncate(count);
    games
}

fn is_remake(game: &Value, stats: &Value) -> bool {
    stats
        .get("gameEndedInEarlySurrender")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || game
            .get("gameDuration")
            .and_then(Value::as_i64)
            .is_some_and(|seconds| seconds <= REMAKE_MAX_DURATION_SECONDS)
}

/// DeepLoL-style platform id ("JP1", "KR", …) for the logged-in client,
/// resolved from `/riotclient/region-locale`.
pub async fn fetch_platform_id() -> Result<String, LcuError> {
    let client = LcuClient::connect().map_err(ie)?;
    let v: Value = client.get("/riotclient/region-locale").await.map_err(ie)?;
    let region = v
        .get("region")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| LcuError::Other("region-locale has no region".into()))?;
    Ok(platform_id_from_region(region))
}

/// Map an LCU region ("JP", "EUW", "KR") to the numbered platform id DeepLoL
/// expects (`JP1`, `EUW1`, …; `KR`/`RU` have no number).
pub(crate) fn platform_id_from_region(region: &str) -> String {
    let region = region.to_ascii_uppercase();
    match region.as_str() {
        "KR" | "RU" => region,
        "EUNE" => "EUN1".into(),
        "OCE" => "OC1".into(),
        "LAN" => "LA1".into(),
        "LAS" => "LA2".into(),
        r if r.ends_with(|c: char| c.is_ascii_digit()) => r.into(),
        r => format!("{r}1"),
    }
}

/// Current gameflow phase. Errors if the client isn't running.
pub async fn fetch_phase() -> Result<Phase, LcuError> {
    let client = LcuClient::connect().map_err(ie)?;
    // The endpoint returns a bare JSON string like `"ChampSelect"`.
    let raw: String = client
        .get("/lol-gameflow/v1/gameflow-phase")
        .await
        .map_err(ie)?;
    Ok(Phase::from_api(&raw))
}

/// The raw champ-select session, or `None` if not currently in champ select.
pub async fn fetch_session() -> Result<Option<Value>, LcuError> {
    let client = LcuClient::connect().map_err(ie)?;
    Ok(client
        .get::<Option<Value>>("/lol-champ-select/v1/session")
        .await
        .ok()
        .flatten())
}

/// Replace the current rune page with `page` and make it active.
pub async fn apply_runes(page: &RunePagePayload) -> Result<(), LcuError> {
    validate_rune_page(page)?;

    let client = LcuClient::connect().map_err(ie)?;

    let pages: Option<Value> = client.get("/lol-perks/v1/pages").await.map_err(ie)?;
    if let Some(id) = pages.as_ref().and_then(deletable_page_id) {
        // Empty 204 body deserializes to an EOF error; we don't care about it.
        let _: std::result::Result<Option<Value>, _> =
            client.delete(format!("/lol-perks/v1/pages/{id}")).await;
    }

    // POST returns the created page; capture as Value so deserialize succeeds.
    let _created: Option<Value> = client.post("/lol-perks/v1/pages", page).await.map_err(ie)?;
    Ok(())
}

fn validate_rune_page(page: &RunePagePayload) -> Result<(), LcuError> {
    if page.primary_style_id <= 0 || page.sub_style_id <= 0 || page.selected_perk_ids.len() != 9 {
        return Err(LcuError::Other(format!(
            "invalid rune page: expected 2 styles and 9 selected perks, got {} selected perks",
            page.selected_perk_ids.len()
        )));
    }
    if page.selected_perk_ids.iter().any(|id| *id <= 0) {
        return Err(LcuError::Other(
            "invalid rune page: selected perks must be positive ids".into(),
        ));
    }
    Ok(())
}

/// Body for `PATCH /lol-champ-select/v1/session/my-selection`. The endpoint
/// treats every field as optional, so sending only the two spells is valid.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MySelection {
    spell1_id: i64,
    spell2_id: i64,
}

/// Set our two summoner spells. Only valid while a champ-select session exists.
pub async fn apply_spells(spell1: i64, spell2: i64) -> Result<(), LcuError> {
    let client = LcuClient::connect().map_err(ie)?;
    let body = MySelection {
        spell1_id: spell1,
        spell2_id: spell2,
    };
    // A successful PATCH answers 204 with an empty body, which irelia's
    // msgpack decoder surfaces as an EOF error (same quirk `apply_runes`
    // works around for DELETE) — treat that as success.
    let res: std::result::Result<Option<Value>, _> = client
        .patch("/lol-champ-select/v1/session/my-selection", &body)
        .await;
    match res {
        Ok(_) | Err(irelia::requests::Error::RmpDecode(_)) => Ok(()),
        Err(e) => Err(ie(e)),
    }
}

/// Pick a deletable page id to free a slot — prefer the active one.
fn deletable_page_id(pages: &Value) -> Option<i64> {
    let arr = pages.as_array()?;
    let is_deletable = |p: &&Value| {
        p.get("isDeletable")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    };
    arr.iter()
        .find(|p| is_deletable(p) && p.get("current").and_then(Value::as_bool).unwrap_or(false))
        .or_else(|| arr.iter().find(is_deletable))
        .and_then(|p| p.get("id").and_then(Value::as_i64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn history_game(
        champion_id: i64,
        game_creation: i64,
        game_duration: i64,
        early_surrender: bool,
    ) -> Value {
        json!({
            "queueId": 420,
            "gameCreation": game_creation,
            "gameDuration": game_duration,
            "participants": [{
                "championId": champion_id,
                "stats": {
                    "win": champion_id % 2 == 0,
                    "kills": champion_id,
                    "deaths": 1,
                    "assists": 2,
                    "gameEndedInEarlySurrender": early_surrender,
                },
            }],
        })
    }

    #[test]
    fn platform_id_mapping() {
        assert_eq!(platform_id_from_region("KR"), "KR");
        assert_eq!(platform_id_from_region("RU"), "RU");
        assert_eq!(platform_id_from_region("JP"), "JP1");
        assert_eq!(platform_id_from_region("EUW"), "EUW1");
        assert_eq!(platform_id_from_region("EUNE"), "EUN1");
        assert_eq!(platform_id_from_region("OCE"), "OC1");
        assert_eq!(platform_id_from_region("LAN"), "LA1");
        assert_eq!(platform_id_from_region("LAS"), "LA2");
        assert_eq!(platform_id_from_region("NA1"), "NA1"); // already numbered
    }

    #[test]
    fn recent_matches_exclude_remakes_before_truncating() {
        let resp = json!({
            "games": {
                "games": [
                    history_game(1, 1_004, 1_800, false),
                    history_game(2, 1_003, 240, false),
                    history_game(3, 1_002, 1_700, false),
                    history_game(4, 1_001, 1_600, false),
                ],
            },
        });

        let games = parse_recent_matches(&resp, 3);

        assert_eq!(
            games.iter().map(|g| g.champion_id).collect::<Vec<_>>(),
            vec![1, 3, 4]
        );
    }

    #[test]
    fn recent_matches_exclude_early_surrender_remakes() {
        let resp = json!({
            "games": {
                "games": [
                    history_game(1, 1_003, 1_800, false),
                    history_game(2, 1_002, 1_800, true),
                    history_game(3, 1_001, 1_700, false),
                ],
            },
        });

        let games = parse_recent_matches(&resp, 10);

        assert_eq!(
            games.iter().map(|g| g.champion_id).collect::<Vec<_>>(),
            vec![1, 3]
        );
    }

    #[test]
    fn rune_page_validation_requires_complete_page() {
        let mut page = RunePagePayload {
            name: "test".into(),
            primary_style_id: 8000,
            sub_style_id: 8400,
            selected_perk_ids: vec![8010, 9111, 9104, 8299, 8473, 8451, 5005, 5008, 5001],
            current: true,
        };

        assert!(validate_rune_page(&page).is_ok());

        page.selected_perk_ids.pop();
        assert!(validate_rune_page(&page).is_err());
    }
}
