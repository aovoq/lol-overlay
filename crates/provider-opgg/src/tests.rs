use super::*;
use crate::types::{
    CounterChampion, CounterRow, Perk, PerkStyle, RuneBuildData, RunePage, TierRow,
};

fn perk(id: i64, is_active: bool) -> Perk {
    Perk { id, is_active }
}

fn sample_build() -> RuneBuildData {
    RuneBuildData {
        primary_perk_style: PerkStyle { id: 8000 },
        perk_sub_style: PerkStyle { id: 8400 },
        main_runes: vec![
            vec![perk(8005, false), perk(8008, false), perk(8010, true)],
            vec![perk(9101, false), perk(9111, true)],
            vec![perk(9104, false), perk(9105, true)],
            vec![perk(8014, false), perk(8299, true)],
        ],
        sub_runes: vec![
            vec![perk(8126, false), perk(8139, true)],
            vec![perk(8138, false)],
            vec![perk(8135, false), perk(8134, true)],
        ],
        shards: vec![
            vec![perk(5008, true), perk(5005, false)],
            vec![perk(5008, false), perk(5010, false), perk(5001, true)],
            vec![perk(5011, true), perk(5013, false)],
        ],
    }
}

#[test]
fn lanes_map_from_lcu_positions() {
    assert_eq!(opgg_lane("top"), Some("top"));
    assert_eq!(opgg_lane("MIDDLE"), Some("mid"));
    assert_eq!(opgg_lane("mid"), Some("mid"));
    assert_eq!(opgg_lane("bottom"), Some("adc"));
    assert_eq!(opgg_lane("adc"), Some("adc"));
    assert_eq!(opgg_lane("utility"), Some("support"));
    assert_eq!(opgg_lane("support"), Some("support"));
    assert_eq!(opgg_lane(""), None);
    assert_eq!(opgg_lane("aram"), None);
}

#[test]
fn active_perk_ids_flattens_rows_in_order() {
    let build = sample_build();
    assert_eq!(
        active_perk_ids(&build.main_runes),
        vec![8010, 9111, 9105, 8299]
    );
    assert_eq!(active_perk_ids(&build.sub_runes), vec![8139, 8134]);
    assert_eq!(active_perk_ids(&build.shards), vec![5008, 5001, 5011]);
}

#[test]
fn top_rune_page_stats_reads_first_page_as_percent() {
    let page = BuildPage {
        runes: vec![RunePage {
            play: 127_576,
            pick_rate: 0.821,
            win_rate: 0.505,
            builds: vec![sample_build()],
        }],
        ..Default::default()
    };
    let (win_rate, games) = top_rune_page_stats(&page);
    assert!((win_rate - 50.5).abs() < 1e-9);
    assert_eq!(games, 127_576);
}

#[test]
fn top_rune_page_stats_defaults_when_no_pages() {
    let page = BuildPage::default();
    assert_eq!(top_rune_page_stats(&page), (0.0, 0));
}

fn counter_row(key: &str, win_rate: f64, play: i64) -> CounterRow {
    CounterRow {
        play,
        win_rate,
        champion: CounterChampion {
            key: key.to_string(),
        },
    }
}

#[test]
fn counter_entries_maps_slug_to_id_and_keeps_subject_win_rate_semantics() {
    let mut slug_to_id = HashMap::new();
    slug_to_id.insert("yone".to_string(), 777);
    slug_to_id.insert("garen".to_string(), 86);
    slug_to_id.insert("darius".to_string(), 122);

    let rows = vec![
        counter_row("yone", 40.0, 500), // we lose this matchup a lot -> strong counter
        counter_row("garen", 60.0, 500), // we win this one -> not a counter
        counter_row("darius", 30.0, 10), // too few games, dropped
        counter_row("unknown_champ", 20.0, 500), // not in the ddragon map, dropped
    ];
    let entries = counter_entries(&rows, &slug_to_id);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].champion_id, 777);
    assert!((entries[0].win_rate - 0.60).abs() < 1e-9);
    assert_eq!(entries[0].games, 500);
    assert!(entries.iter().all(|e| e.champion_id != 122));
}

fn tier_row(key: &str, win_rate: f64, pick_rate: f64, ban_rate: f64) -> TierRow {
    TierRow {
        key: key.to_string(),
        win_rate,
        pick_rate,
        ban_rate,
    }
}

#[test]
fn tier_entries_maps_slug_to_id_converts_percent_and_sorts_by_win_rate() {
    let mut slug_to_id = HashMap::new();
    slug_to_id.insert("garen".to_string(), 86);
    slug_to_id.insert("darius".to_string(), 122);

    let rows = vec![
        tier_row("garen", 51.7974, 8.16142, 6.8726),
        tier_row("darius", 53.5, 4.1, 2.0),
        tier_row("unknown_champ", 60.0, 0.5, 0.1), // not in the ddragon map, dropped
    ];
    let entries = tier_entries(&rows, &slug_to_id);
    assert_eq!(entries.len(), 2);
    // sorted by win rate desc; percentages divided by 100
    assert_eq!(entries[0].champion_id, 122);
    assert!((entries[0].win_rate - 0.535).abs() < 1e-9);
    assert!((entries[0].pick_rate - 0.041).abs() < 1e-9);
    assert!((entries[0].ban_rate - 0.02).abs() < 1e-9);
    assert_eq!(entries[0].games, 0);
    assert_eq!(entries[0].win_rate_delta, 0.0);
    assert_eq!(entries[1].champion_id, 86);
}

#[tokio::test]
#[ignore = "network: live op.gg build page for Aatrox top"]
async fn fetch_items_runes_and_skill_order_from_live_site() {
    let provider = OpggProvider::new(Arc::new(DdragonClient::new())).expect("provider");

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
    println!("items: {items:?}");

    // Aatrox = 266
    let build = provider
        .rune_build(266, Some("top"), None)
        .await
        .expect("rune_build");
    assert_eq!(build.primary_perk_ids.len(), 4);
    assert_eq!(build.sub_perk_ids.len(), 2);
    assert_eq!(build.shard_ids.len(), 3);
    println!("rune build: {build:?}");

    let skills = provider.skill_order(&snapshot).await.expect("skill_order");
    assert!(!skills.max_order.is_empty());
    println!("skill order: {skills:?}");
}

#[tokio::test]
#[ignore = "network: live op.gg counters page for Aatrox top"]
async fn fetch_counters_from_live_site() {
    let provider = OpggProvider::new(Arc::new(DdragonClient::new())).expect("provider");
    let counters = provider.counters(266, "top").await.expect("counters");
    assert!(!counters.is_empty());
    assert!(counters
        .iter()
        .all(|c| c.games >= overlay_provider::MIN_MATCHUP_GAMES));
    println!("counters: {counters:?}");
}

#[tokio::test]
#[ignore = "network: live op.gg tier list for all 5 lanes"]
async fn fetch_tier_list_from_live_site() {
    let provider = OpggProvider::new(Arc::new(DdragonClient::new())).expect("provider");
    for role in ["top", "jungle", "middle", "bottom", "utility"] {
        let rows = provider.tier_list(role).await.expect("tier_list");
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| r.champion_id > 0));
        assert!(rows.iter().all(|r| r.win_rate > 0.0 && r.win_rate < 1.0));
        assert!(rows.windows(2).all(|w| w[0].win_rate >= w[1].win_rate));
        println!("{role}: {} rows, top = {:?}", rows.len(), rows[0]);
    }
}

#[tokio::test]
#[ignore = "network: matchup-specific rune pages are unsupported by this provider"]
async fn matchup_rune_build_reports_not_enough_data() {
    let provider = OpggProvider::new(Arc::new(DdragonClient::new())).expect("provider");
    assert!(matches!(
        provider.rune_build(266, Some("top"), Some(75)).await,
        Err(ProviderError::NotEnoughData)
    ));
}
