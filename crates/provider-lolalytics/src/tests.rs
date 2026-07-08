use super::*;
use crate::types::{CounterRow, TierChampion, TierGroup, TierLane, TierResponse};
use std::collections::HashMap;

fn tier_response(lane: &str, rows: &[(i64, TierChampion)]) -> TierResponse {
    let cid: HashMap<String, TierChampion> = rows
        .iter()
        .map(|(id, c)| (id.to_string(), c.clone()))
        .collect();
    let mut lanes = HashMap::new();
    lanes.insert(lane.to_string(), TierLane { cid });
    let mut tier = HashMap::new();
    tier.insert("0".to_string(), TierGroup { lane: lanes });
    TierResponse { tier }
}

fn champ(wr: f64, pr: f64, br: f64, games: i64) -> TierChampion {
    TierChampion { wr, pr, br, games }
}

#[test]
fn lanes_map_from_lcu_positions() {
    assert_eq!(lol_lane("top"), Some("top"));
    assert_eq!(lol_lane("MIDDLE"), Some("middle"));
    assert_eq!(lol_lane("mid"), Some("middle"));
    assert_eq!(lol_lane("bottom"), Some("bottom"));
    assert_eq!(lol_lane("adc"), Some("bottom"));
    assert_eq!(lol_lane("utility"), Some("support"));
    assert_eq!(lol_lane("support"), Some("support"));
    assert_eq!(lol_lane(""), None);
    assert_eq!(lol_lane("aram"), None);
}

#[test]
fn item_combo_splits_and_ignores_non_numeric() {
    assert_eq!(parse_item_combo("1055_1037_2021"), vec![1055, 1037, 2021]);
    assert_eq!(parse_item_combo("3161"), vec![3161]);
    assert_eq!(parse_item_combo("a_10_b"), vec![10]);
    assert!(parse_item_combo("").is_empty());
}

#[test]
fn tier_entries_convert_percentages_filter_fringe_and_sort() {
    let resp = tier_response(
        "top",
        &[
            (266, champ(52.2, 6.38, 5.85, 27697)), // strong pick
            (86, champ(53.5, 4.10, 2.00, 12000)),  // strongest WR
            (1, champ(60.0, 0.12, 0.74, 500)),     // fringe: pick rate < 0.5%, dropped
        ],
    );
    let rows = tier_entries(&resp, "top");
    assert_eq!(rows.len(), 2);
    // sorted by win rate desc; percentages divided by 100
    assert_eq!(rows[0].champion_id, 86);
    assert!((rows[0].win_rate - 0.535).abs() < 1e-9);
    assert!((rows[0].pick_rate - 0.041).abs() < 1e-9);
    assert!((rows[0].ban_rate - 0.02).abs() < 1e-9);
    assert_eq!(rows[0].games, 12000);
    assert_eq!(rows[0].win_rate_delta, 0.0);
    assert_eq!(rows[1].champion_id, 266);
    // fringe champion filtered out
    assert!(rows.iter().all(|r| r.champion_id != 1));
}

#[test]
fn tier_entries_empty_for_missing_lane() {
    let resp = tier_response("top", &[(266, champ(52.0, 5.0, 5.0, 1000))]);
    assert!(tier_entries(&resp, "jungle").is_empty());
}

#[test]
fn counters_invert_vs_wr_filter_by_games_and_cap() {
    let rows = vec![
        CounterRow {
            cid: 236,
            vs_wr: 65.0,
            n: 228,
        }, // strong counter
        CounterRow {
            cid: 202,
            vs_wr: 40.0,
            n: 300,
        }, // we beat them
        CounterRow {
            cid: 5,
            vs_wr: 90.0,
            n: 10,
        }, // too few games, dropped
    ];
    let entries = counter_entries(&rows);
    assert_eq!(entries.len(), 2);
    // strongest counter first, win rate kept as the counter champion's own
    assert_eq!(entries[0].champion_id, 236);
    assert!((entries[0].win_rate - 0.65).abs() < 1e-9);
    assert_eq!(entries[0].games, 228);
    assert_eq!(entries[1].champion_id, 202);
    assert!((entries[1].win_rate - 0.40).abs() < 1e-9);
    assert!(entries
        .iter()
        .all(|e| e.games >= overlay_provider::MIN_MATCHUP_GAMES));
}

#[tokio::test]
#[ignore = "network: live LoLalytics tier list"]
async fn fetch_tier_list_from_live_api() {
    let provider = LolalyticsProvider::new(Arc::new(DdragonClient::new())).expect("provider");
    let rows = provider.tier_list("middle").await.expect("tier_list");
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|r| r.champion_id > 0));
    assert!(rows.iter().all(|r| r.win_rate > 0.0 && r.win_rate < 1.0));
    // sorted by win rate descending
    assert!(rows.windows(2).all(|w| w[0].win_rate >= w[1].win_rate));
    println!("tier rows: {}", rows.len());
}

#[tokio::test]
#[ignore = "network: live LoLalytics items + counters for Aatrox top"]
async fn fetch_items_and_counters_from_live_api() {
    let provider = LolalyticsProvider::new(Arc::new(DdragonClient::new())).expect("provider");

    let snapshot = GameSnapshot {
        game_mode: "CLASSIC".into(),
        game_time: 600.0,
        self_champion: "Aatrox".into(),
        self_raw_name: "Aatrox".into(),
        self_position: "top".into(),
        enemies: vec![],
        allies: vec![],
    };
    let items = provider.items(&snapshot).await.expect("items");
    assert!(!items.is_empty());
    assert!(items.iter().all(|i| i.item_id > 0));
    println!("items: {items:?}");

    // Aatrox = 266
    let counters = provider.counters(266, "top").await.expect("counters");
    assert!(!counters.is_empty());
    assert!(counters
        .iter()
        .all(|c| c.games >= overlay_provider::MIN_MATCHUP_GAMES));
    println!("counters: {counters:?}");
}

#[tokio::test]
#[ignore = "network: runes are unsupported by this provider"]
async fn runes_report_not_enough_data() {
    let provider = LolalyticsProvider::new(Arc::new(DdragonClient::new())).expect("provider");
    assert!(matches!(
        provider.runes(266, Some("top")).await,
        Err(ProviderError::NotEnoughData)
    ));
}
