use overlay_types::{ChampSelectEvent, MyPick};
use serde_json::Value;

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

/// Parse the full champ-select session into the OPENLOL panel event.
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
        me.and_then(|m| m.get(key))
            .and_then(Value::as_i64)
            .unwrap_or(0)
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
    for action in turns
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .flatten()
    {
        let kind = action.get("type").and_then(Value::as_str).unwrap_or("");
        let completed = action
            .get("completed")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let champion = action
            .get("championId")
            .and_then(Value::as_i64)
            .unwrap_or(0);
        let actor = action
            .get("actorCellId")
            .and_then(Value::as_i64)
            .unwrap_or(-1);
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
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_i64)
                    .filter(|&id| id > 0)
                    .collect()
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
