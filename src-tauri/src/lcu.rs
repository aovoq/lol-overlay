//! LCU (League Client) access via the [`irelia`] crate.
//!
//! `irelia` handles lockfile discovery, auth, and the self-signed cert for us,
//! so this module is just thin helpers over its REST client plus a WebSocket
//! subscriber that pushes champ-select updates onto a channel for event-driven
//! rune import (no polling needed for the pick itself).
//!
//! Writing runes uses `/lol-perks/*`, which is on Riot's approved LCU endpoint
//! list but still requires registering the app with Riot before public release.

use irelia::rest::LcuClient;
use irelia::ws::types::{Event, EventKind};
use irelia::ws::{LcuWebSocket, Subscriber};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::{Error, Result};
use crate::events::ChampSelectEvent;

/// Map any `irelia` error into our crate error.
fn ie<E: std::fmt::Debug>(e: E) -> Error {
    Error::Other(format!("{e:?}"))
}

/// Gameflow phases we branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Phase {
    None,
    Lobby,
    Matchmaking,
    ChampSelect,
    InProgress,
    Other,
}

impl Phase {
    fn from_api(s: &str) -> Self {
        match s {
            "None" => Phase::None,
            "Lobby" => Phase::Lobby,
            "Matchmaking" | "ReadyCheck" => Phase::Matchmaking,
            "ChampSelect" => Phase::ChampSelect,
            "GameStart" | "InProgress" => Phase::InProgress,
            _ => Phase::Other,
        }
    }

    /// Stable string label sent to the frontend.
    pub fn label(self) -> &'static str {
        match self {
            Phase::None => "None",
            Phase::Lobby => "Lobby",
            Phase::Matchmaking => "Matchmaking",
            Phase::ChampSelect => "ChampSelect",
            Phase::InProgress => "InProgress",
            Phase::Other => "Other",
        }
    }
}

/// Our current pick in champ select.
#[derive(Debug, Clone)]
pub struct MyPick {
    pub champion_id: i64,
    /// LCU position string: "top" | "jungle" | "middle" | "bottom" | "utility".
    pub position: Option<String>,
}

/// A rune page to POST into the client.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunePagePayload {
    pub name: String,
    pub primary_style_id: i64,
    pub sub_style_id: i64,
    pub selected_perk_ids: Vec<i64>,
    pub current: bool,
}

/// Logged-in summoner + current solo-queue standing, emitted to the frontend
/// as the `summoner` event (camelCase to match the TS interface).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummonerInfo {
    pub game_name: String,
    pub tag_line: String,
    pub level: i64,
    pub profile_icon_id: i64,
    /// "" when unranked.
    pub solo_tier: String,
    /// Roman numeral ("II"); "NA" for apex tiers.
    pub solo_division: String,
    pub solo_lp: i64,
    pub solo_wins: i64,
    pub solo_losses: i64,
}

/// The logged-in summoner with their solo-queue rank. Errors if the client
/// isn't running; rank fields stay empty/0 if ranked stats aren't available
/// yet (e.g. right after login).
pub async fn fetch_summoner() -> Result<SummonerInfo> {
    let client = LcuClient::connect().map_err(ie)?;
    let me: Value = client
        .get("/lol-summoner/v1/current-summoner")
        .await
        .map_err(ie)?;

    let str_of = |v: &Value, key: &str| {
        v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
    };
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

/// One game from the local match history, reduced to what the profile chip
/// shows. Emitted to the frontend inside `match-history` (newest first).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentGame {
    pub champion_id: i64,
    pub win: bool,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub queue_id: i64,
    /// Unix millis.
    pub game_creation: i64,
}

const REMAKE_MAX_DURATION_SECONDS: i64 = 5 * 60;

/// The local player's most recent games (newest first). The current-summoner
/// match-history endpoint returns only our own participant per game.
pub async fn fetch_recent_matches(count: usize) -> Result<Vec<RecentGame>> {
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

fn parse_recent_matches(resp: &Value, count: usize) -> Vec<RecentGame> {
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
pub async fn fetch_platform_id() -> Result<String> {
    let client = LcuClient::connect().map_err(ie)?;
    let v: Value = client.get("/riotclient/region-locale").await.map_err(ie)?;
    let region = v
        .get("region")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::Other("region-locale has no region".into()))?;
    Ok(platform_id_from_region(region))
}

/// Map an LCU region ("JP", "EUW", "KR") to the numbered platform id DeepLoL
/// expects (`JP1`, `EUW1`, …; `KR`/`RU` have no number).
fn platform_id_from_region(region: &str) -> String {
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
pub async fn fetch_phase() -> Result<Phase> {
    let client = LcuClient::connect().map_err(ie)?;
    // The endpoint returns a bare JSON string like `"ChampSelect"`.
    let raw: String = client
        .get("/lol-gameflow/v1/gameflow-phase")
        .await
        .map_err(ie)?;
    Ok(Phase::from_api(&raw))
}

/// The raw champ-select session, or `None` if not currently in champ select.
pub async fn fetch_session() -> Result<Option<Value>> {
    let client = LcuClient::connect().map_err(ie)?;
    Ok(client
        .get::<Option<Value>>("/lol-champ-select/v1/session")
        .await
        .ok()
        .flatten())
}

/// Extract our pick from a champ-select session JSON (works for both the REST
/// response and the WebSocket event payload — same shape).
///
/// `championId` stays 0 until our actual pick turn, but during the planning
/// phase the declared intent lives in `championPickIntent` — counting it lets
/// auto-import fire as soon as a champion is hovered, not just on lock.
pub fn parse_my_pick(session: &Value) -> Option<MyPick> {
    let cell = session.get("localPlayerCellId")?.as_i64()?;
    let me = session
        .get("myTeam")?
        .as_array()?
        .iter()
        .find(|m| m.get("cellId").and_then(Value::as_i64) == Some(cell))?;

    let field = |key: &str| me.get(key).and_then(Value::as_i64).unwrap_or(0);
    let champion_id = match field("championId") {
        id if id > 0 => id,
        _ => field("championPickIntent"),
    };
    if champion_id <= 0 {
        return None; // nothing hovered/locked yet
    }
    let position = me
        .get("assignedPosition")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Some(MyPick {
        champion_id,
        position,
    })
}

/// Parse the full champ-select session into the HEXGATE panel event.
/// `active` is always true here — the poller emits the `active: false`
/// sentinel when champ select ends.
///
/// Returns `None` when the payload isn't a champ-select session (e.g. the
/// WebSocket `Delete` event carries `null`). Everything else is parsed
/// defensively: older session shapes lack `isAllyAction`/`isInProgress`, and
/// blind pick has no `assignedPosition` and no enemy data at all.
pub fn parse_champ_select(session: &Value) -> Option<ChampSelectEvent> {
    let my_cell = session.get("localPlayerCellId")?.as_i64()?;

    // (cellId, championId) per team, sorted so slot order is cell order.
    let team_members = |key: &str| -> Vec<(i64, i64)> {
        let mut v: Vec<(i64, i64)> = session
            .get(key)
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .map(|m| {
                        (
                            m.get("cellId").and_then(Value::as_i64).unwrap_or(-1),
                            m.get("championId").and_then(Value::as_i64).unwrap_or(0),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        v.sort_by_key(|&(cell, _)| cell);
        v
    };
    let my_team = team_members("myTeam");
    let their_team = team_members("theirTeam");
    let my_cells: Vec<i64> = my_team.iter().map(|&(cell, _)| cell).collect();

    let me = session
        .get("myTeam")
        .and_then(Value::as_array)
        .and_then(|arr| {
            arr.iter()
                .find(|m| m.get("cellId").and_then(Value::as_i64) == Some(my_cell))
        });
    let my_field = |key: &str| {
        me.and_then(|m| m.get(key)).and_then(Value::as_i64).unwrap_or(0)
    };
    let my_role = me
        .and_then(|m| m.get("assignedPosition"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Walk every action once. `actions` is the reliable source for revealed
    // enemy bans (the `bans` node may lag) and the only lock signal.
    let mut my_locked = false;
    let mut my_action_champ = 0_i64;
    let mut action_my_bans: Vec<i64> = Vec::new();
    let mut action_enemy_bans: Vec<i64> = Vec::new();
    let mut enemy_picks: Vec<(i64, i64)> = Vec::new(); // (cellId, championId)

    let turns = session.get("actions").and_then(Value::as_array);
    for action in turns.into_iter().flatten().filter_map(Value::as_array).flatten() {
        let kind = action.get("type").and_then(Value::as_str).unwrap_or("");
        let completed = action.get("completed").and_then(Value::as_bool).unwrap_or(false);
        let champion = action.get("championId").and_then(Value::as_i64).unwrap_or(0);
        let actor = action.get("actorCellId").and_then(Value::as_i64).unwrap_or(-1);
        // `isAllyAction` is missing in older session shapes (blind pick dumps);
        // fall back to checking the actor against our team's cells.
        let ally = action
            .get("isAllyAction")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| my_cells.contains(&actor));

        match kind {
            // championId 0 on a completed ban = the ban was skipped.
            "ban" if completed && champion > 0 => {
                if ally {
                    action_my_bans.push(champion);
                } else {
                    action_enemy_bans.push(champion);
                }
            }
            "pick" if actor == my_cell => {
                if completed {
                    my_locked = true;
                }
                if champion > 0 {
                    my_action_champ = champion;
                }
            }
            // Completed enemy picks backfill `theirTeam` slots that still
            // read 0 (enemy hovers are never broadcast, only locks).
            "pick" if !ally && completed && champion > 0 => {
                enemy_picks.push((actor, champion));
            }
            _ => {}
        }
    }

    // 5 slots in cell order; 0 = not picked/revealed yet.
    let slots = |team: &[(i64, i64)]| -> Vec<i64> {
        let mut s: Vec<i64> = team.iter().map(|&(_, champ)| champ).take(5).collect();
        s.resize(5, 0);
        s
    };
    let my_team_champion_ids = slots(&my_team);
    let mut enemy_champion_ids = slots(&their_team);
    for (cell, champion) in enemy_picks {
        if let Some(idx) = their_team.iter().position(|&(c, _)| c == cell) {
            if idx < enemy_champion_ids.len() && enemy_champion_ids[idx] == 0 {
                enemy_champion_ids[idx] = champion;
            }
        }
    }

    // Merge the `bans` node with action-derived bans, deduped — whichever
    // populates first wins, order preserved.
    let ban_list = |key: &str| -> Vec<i64> {
        session
            .get("bans")
            .and_then(|b| b.get(key))
            .and_then(Value::as_array)
            .map(|arr| arr.iter().filter_map(Value::as_i64).filter(|&id| id > 0).collect())
            .unwrap_or_default()
    };
    let mut my_bans = ban_list("myTeamBans");
    let mut enemy_bans = ban_list("theirTeamBans");
    for ban in action_my_bans {
        if !my_bans.contains(&ban) {
            my_bans.push(ban);
        }
    }
    for ban in action_enemy_bans {
        if !enemy_bans.contains(&ban) {
            enemy_bans.push(ban);
        }
    }

    let timer_phase = session
        .get("timer")
        .and_then(|t| t.get("phase"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    // Hover/lock sources in confidence order: the team row (set on lock and
    // during our pick turn), planning-phase intent, then our pick action.
    let my_champion_id = [
        my_field("championId"),
        my_field("championPickIntent"),
        my_action_champ,
    ]
    .into_iter()
    .find(|&id| id > 0)
    .unwrap_or(0);

    Some(ChampSelectEvent {
        active: true,
        my_role,
        my_champion_id,
        my_locked,
        my_team_champion_ids,
        enemy_champion_ids,
        my_bans,
        enemy_bans,
        timer_phase,
    })
}

/// Replace the current rune page with `page` and make it active.
pub async fn apply_runes(page: &RunePagePayload) -> Result<()> {
    let client = LcuClient::connect().map_err(ie)?;

    let pages: Option<Value> = client.get("/lol-perks/v1/pages").await.map_err(ie)?;
    if let Some(id) = pages.as_ref().and_then(deletable_page_id) {
        // Empty 204 body deserializes to an EOF error; we don't care about it.
        let _: std::result::Result<Option<Value>, _> =
            client.delete(format!("/lol-perks/v1/pages/{id}")).await;
    }

    // POST returns the created page; capture as Value so deserialize succeeds.
    let _created: Option<Value> = client
        .post("/lol-perks/v1/pages", page)
        .await
        .map_err(ie)?;
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
pub async fn apply_spells(spell1: i64, spell2: i64) -> Result<()> {
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
    let is_deletable = |p: &&Value| p.get("isDeletable").and_then(Value::as_bool).unwrap_or(false);
    arr.iter()
        .find(|p| is_deletable(p) && p.get("current").and_then(Value::as_bool).unwrap_or(false))
        .or_else(|| arr.iter().find(is_deletable))
        .and_then(|p| p.get("id").and_then(Value::as_i64))
}

/// WebSocket subscriber that forwards champ-select session payloads onto a channel.
struct SessionForwarder {
    tx: UnboundedSender<Value>,
}

impl Subscriber for SessionForwarder {
    fn on_event(&mut self, event: &Event, _continues: &mut bool) {
        // Event(RequestType, EventKind, EventData{ data, event_type, uri }).
        let _ = self.tx.send(event.2.data.clone());
    }
}

/// Open a WebSocket subscribed to champ-select session updates, forwarding each
/// update to `tx`. Keep the returned handle alive for the app's lifetime —
/// dropping it closes the socket.
#[must_use]
pub fn subscribe_champ_select(tx: UnboundedSender<Value>) -> LcuWebSocket {
    let mut ws = LcuWebSocket::new();
    let _ = ws.subscribe(
        EventKind::json_api_event_callback("/lol-champ-select/v1/session"),
        SessionForwarder { tx },
    );
    ws
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

    /// A `myTeam`/`theirTeam` row with the fields the LCU actually sends.
    fn player(cell: i64, champion: i64, intent: i64, position: &str) -> Value {
        json!({
            "cellId": cell,
            "championId": champion,
            "championPickIntent": intent,
            "assignedPosition": position,
            "selectedSkinId": 0,
            "spell1Id": 4,
            "spell2Id": 11,
            "team": if cell < 5 { 1 } else { 2 },
        })
    }

    /// A modern-shape action (has `isAllyAction`/`isInProgress`).
    fn action(actor: i64, kind: &str, champion: i64, completed: bool, ally: bool) -> Value {
        json!({
            "id": actor * 10,
            "actorCellId": actor,
            "championId": champion,
            "type": kind,
            "completed": completed,
            "isAllyAction": ally,
            "isInProgress": !completed,
            "pickTurn": 1,
        })
    }

    fn empty_bans() -> Value {
        json!({ "myTeamBans": [], "theirTeamBans": [], "numBans": 10 })
    }

    fn session(
        my_cell: i64,
        my_team: Vec<Value>,
        their_team: Vec<Value>,
        actions: Value,
        bans: Value,
        timer_phase: &str,
    ) -> Value {
        json!({
            "localPlayerCellId": my_cell,
            "myTeam": my_team,
            "theirTeam": their_team,
            "actions": actions,
            "bans": bans,
            "timer": {
                "phase": timer_phase,
                "adjustedTimeLeftInPhase": 30000,
                "totalTimeInPhase": 30000,
                "isInfinite": false,
            },
            "isSpectating": false,
        })
    }

    fn full_my_team(me_slot: usize, champion: i64, intent: i64) -> Vec<Value> {
        let roles = ["top", "jungle", "middle", "bottom", "utility"];
        (0..5)
            .map(|i| {
                if i == me_slot {
                    player(i as i64, champion, intent, roles[i])
                } else {
                    player(i as i64, 0, 0, roles[i])
                }
            })
            .collect()
    }

    fn empty_enemy_team() -> Vec<Value> {
        (5..10).map(|c| player(c, 0, 0, "")).collect()
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
    fn champ_select_none_without_local_cell() {
        assert!(parse_champ_select(&json!({ "myTeam": [] })).is_none());
        assert!(parse_champ_select(&json!(null)).is_none());
    }

    #[test]
    fn champ_select_planning_hover_uses_pick_intent() {
        let s = session(
            1,
            full_my_team(1, 0, 234), // Viego declared, not picked
            empty_enemy_team(),
            json!([[action(1, "pick", 0, false, true)]]),
            empty_bans(),
            "PLANNING",
        );
        let ev = parse_champ_select(&s).unwrap();
        assert!(ev.active);
        assert_eq!(ev.my_champion_id, 234);
        assert!(!ev.my_locked);
        assert_eq!(ev.my_role, "jungle");
        assert_eq!(ev.timer_phase, "PLANNING");
        // Intent is not a pick: team slots stay unknown.
        assert_eq!(ev.my_team_champion_ids, vec![0, 0, 0, 0, 0]);
    }

    #[test]
    fn champ_select_locked_pick_via_completed_action() {
        // Post-lock the LCU sets championId and resets championPickIntent.
        let s = session(
            1,
            full_my_team(1, 234, 0),
            empty_enemy_team(),
            json!([[action(1, "pick", 234, true, true)]]),
            empty_bans(),
            "BAN_PICK",
        );
        let ev = parse_champ_select(&s).unwrap();
        assert_eq!(ev.my_champion_id, 234);
        assert!(ev.my_locked);
        assert_eq!(ev.my_team_champion_ids, vec![0, 234, 0, 0, 0]);
    }

    #[test]
    fn champ_select_enemy_bans_from_actions_when_bans_node_lags() {
        // Real dumps show `theirTeamBans` empty while completed enemy ban
        // actions exist; ally ban 266 appears in BOTH sources → deduped.
        let s = session(
            0,
            full_my_team(0, 0, 0),
            empty_enemy_team(),
            json!([
                [action(0, "ban", 266, true, true)],
                [action(5, "ban", 157, true, false)],
                [action(6, "ban", 238, true, false)],
                [action(7, "ban", 0, true, false)], // skipped ban
            ]),
            json!({ "myTeamBans": [266], "theirTeamBans": [], "numBans": 10 }),
            "BAN_PICK",
        );
        let ev = parse_champ_select(&s).unwrap();
        assert_eq!(ev.my_bans, vec![266]);
        assert_eq!(ev.enemy_bans, vec![157, 238]);
    }

    #[test]
    fn champ_select_enemy_picks_revealed() {
        // Cell 5 revealed via theirTeam, cell 6 only via its completed pick
        // action (theirTeam may lag the action), cell 7 in progress = hidden.
        let mut enemies = empty_enemy_team();
        enemies[0] = player(5, 35, 0, ""); // Shaco revealed
        let s = session(
            0,
            full_my_team(0, 0, 0),
            enemies,
            json!([[
                action(6, "pick", 64, true, false),
                action(7, "pick", 0, false, false),
            ]]),
            empty_bans(),
            "BAN_PICK",
        );
        let ev = parse_champ_select(&s).unwrap();
        assert_eq!(ev.enemy_champion_ids, vec![35, 64, 0, 0, 0]);
    }

    #[test]
    fn champ_select_blind_pick_legacy_shape() {
        // Blind pick: no assignedPosition, no enemy info, all pick actions in
        // one turn, and the legacy action shape lacks isAllyAction — team
        // membership must be inferred from actorCellId.
        let bare = |actor: i64, champion: i64, completed: bool| {
            json!({
                "id": actor,
                "actorCellId": actor,
                "championId": champion,
                "completed": completed,
                "pickTurn": 1,
                "type": "pick",
            })
        };
        let my_team: Vec<Value> = (0..5)
            .map(|c| player(c, if c == 0 { 17 } else { 0 }, 0, ""))
            .collect();
        let s = session(
            0,
            my_team,
            vec![],
            json!([[bare(0, 17, true), bare(1, 0, false), bare(2, 0, false)]]),
            empty_bans(),
            "BAN_PICK",
        );
        let ev = parse_champ_select(&s).unwrap();
        assert_eq!(ev.my_role, "");
        assert_eq!(ev.my_champion_id, 17);
        assert!(ev.my_locked);
        assert_eq!(ev.my_team_champion_ids, vec![17, 0, 0, 0, 0]);
        assert_eq!(ev.enemy_champion_ids, vec![0, 0, 0, 0, 0]);
        assert!(ev.my_bans.is_empty());
        assert!(ev.enemy_bans.is_empty());
    }

    #[test]
    fn my_pick_counts_pick_intent_during_planning() {
        let s = session(
            1,
            full_my_team(1, 0, 234),
            empty_enemy_team(),
            json!([]),
            empty_bans(),
            "PLANNING",
        );
        let pick = parse_my_pick(&s).unwrap();
        assert_eq!(pick.champion_id, 234);
        assert_eq!(pick.position.as_deref(), Some("jungle"));
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
    fn my_pick_none_when_nothing_chosen() {
        let s = session(
            1,
            full_my_team(1, 0, 0),
            empty_enemy_team(),
            json!([]),
            empty_bans(),
            "PLANNING",
        );
        assert!(parse_my_pick(&s).is_none());
    }
}
