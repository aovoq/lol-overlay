use super::types::{MatchUp, MatchUpEntry, OtpRune, OtpSpell};
use super::*;

overlay_provider::build_provider_contract_suite!(deeplol_shared_contract, "deeplol");

#[test]
fn counter_and_rune_failures_keep_upstream_diagnostics() {
    let counter = decode_deeplol_body::<MatchupStatsResponse>(
        502,
        Some("text/plain"),
        "counter upstream\nfailed",
    )
    .expect_err("counter failure");
    assert!(counter.to_string().contains("HTTP 502"));
    assert!(counter.to_string().contains("content-type=text/plain"));
    assert!(counter.to_string().contains("counter upstream failed"));

    let rune = decode_deeplol_body::<BuildResponse>(
        200,
        Some("application/json"),
        r#"{"build_by_lane":{"Middle":{"games":"not-a-number"}}}"#,
    )
    .expect_err("rune schema mismatch");
    assert!(matches!(rune, ProviderError::InvalidData(_)));
    assert!(rune.to_string().contains("schema mismatch"));
}

#[test]
fn null_games_lane_survives_parse() {
    // Regression: an Aram lane with `"games": null` used to abort the whole
    // `/champion/build` parse, leaving the UI with no items at all.
    let json = r#"{
          "build_by_lane": {
            "Middle": {"games": 100, "build_lst": [
              {"win_rate": 0.53, "games": 80,
               "rune": {"main_build": [8000,8010,9111,9105,8299],
                        "sub_build": [8400,8473,8453],
                        "stat_build": [5001,5008,5008]},
               "item": {"build": [6692,3047,0,3814]}}
            ]},
            "Aram": {"games": null, "build_lst": []}
          }
        }"#;
    let b: BuildResponse = serde_json::from_str(json).expect("null games must not abort parse");
    let (lane, e) = pick(&b, Some("MIDDLE")).expect("Middle build should be picked");
    assert_eq!(lane, "Middle");
    assert_eq!(e.item.build, vec![6692, 3047, 0, 3814]);
    // Rune flatten is keystone+3 / +2 / +3 shards = 9 perks (LCU page size).
    let perks =
        (e.rune.main_build.len() - 1) + (e.rune.sub_build.len() - 1) + e.rune.stat_build.len();
    assert_eq!(perks, 9);
}

#[test]
fn normalize_collapses_punctuation_and_case() {
    assert_eq!(normalize("Cho'Gath"), "chogath");
    assert_eq!(normalize("Chogath"), "chogath");
    assert_eq!(normalize("Kai'Sa"), "kaisa");
}

#[test]
fn stat_shards_normalize_to_lcu_slot_order() {
    assert_eq!(
        normalize_stat_shards(&[5005, 5008, 5001]),
        vec![5005, 5008, 5001]
    );
    assert_eq!(
        normalize_stat_shards(&[5001, 5008, 5005]),
        vec![5005, 5008, 5001]
    );
    assert_eq!(
        normalize_stat_shards(&[5001, 5008, 5008]),
        vec![5008, 5008, 5001]
    );
}

#[test]
fn rank_response_parses_and_shapes_tier_rows() {
    // `rank_delta: null` exercises the null_default path on /champion/rank.
    let now_json = r#"{
          "champion_data_list": [
            {"champion_id": 64, "performance_dict": {
              "Jungle": {"win_rate": 0.52, "pick_rate": 0.12, "ban_rate": 0.08,
                         "tier": 1, "rank": 3, "rank_delta": null, "games": 0},
              "Total": {}
            }},
            {"champion_id": 35, "performance_dict": {
              "Jungle": {"win_rate": 0.545, "pick_rate": 0.04, "ban_rate": 0.10,
                         "tier": 1, "rank": 1, "rank_delta": 2, "games": 0}
            }},
            {"champion_id": 1, "performance_dict": {
              "Jungle": {"win_rate": 0.61, "pick_rate": 0.001, "ban_rate": 0,
                         "tier": 0, "rank": 99, "rank_delta": 0, "games": 0},
              "Middle": {"win_rate": 0.51, "pick_rate": 0.06, "ban_rate": 0.02,
                         "tier": 2, "rank": 10, "rank_delta": 0, "games": 0}
            }},
            {"champion_id": 99, "performance_dict": {
              "Jungle": {"win_rate": 0, "pick_rate": 0.02, "ban_rate": 0,
                         "tier": 0, "rank": 0, "rank_delta": 0, "games": 0}
            }}
          ]
        }"#;
    let prev_json = r#"{"champion_data_list": [
          {"champion_id": 64, "performance_dict": {
            "Jungle": {"win_rate": 0.50, "pick_rate": 0.11, "ban_rate": 0.07,
                       "tier": 2, "rank": 4, "rank_delta": 0, "games": 0}
          }}
        ]}"#;
    let now: RankResponse = serde_json::from_str(now_json).expect("rank must parse");
    let prev: RankResponse = serde_json::from_str(prev_json).expect("prev rank must parse");

    let rows = tier_rows(&now, Some(&prev), "Jungle");
    // 1 is dropped (0.1% pick rate), 99 is dropped (win_rate 0 = not a
    // jungler); the rest sort by win rate desc.
    assert_eq!(
        rows.iter().map(|r| r.champion_id).collect::<Vec<_>>(),
        vec![35, 64]
    );
    // 35 is missing from the previous patch → delta unknown (0.0).
    assert_eq!(rows[0].win_rate_delta, None);
    // 64: 0.52 vs 0.50 → +2.0 percentage points.
    assert!((rows[1].win_rate_delta.expect("previous win rate") - 2.0).abs() < 1e-9);
    assert!((rows[1].pick_rate - 0.12).abs() < 1e-9);
    assert!((rows[1].ban_rate - 0.08).abs() < 1e-9);
    // Games are calibrated separately; the pure shaping leaves them 0.
    assert!(rows.iter().all(|r| r.games.is_none()));
}

#[test]
fn build_spell_and_matchup_parse_and_counters_invert() {
    let json = r#"{
          "build_by_lane": {
            "Jungle": {
              "games": 5000, "pick_rate": 0.05, "win_rate": 0.51, "ban_rate": null,
              "build_lst": [
                {"win_rate": 0.53, "games": 800,
                 "rune": {"main_build": [8000,8010,9111,9105,8299],
                          "sub_build": [8400,8473,8453],
                          "stat_build": [5005,5008,5001]},
                 "item": {"build": [6692]},
                 "spell": {"build": [11, 4]},
                 "skill": {"build": [3, 1, 2],
                           "detail": [3, 1, 2, 3, 3, 4],
                           "win_rate": 0.54,
                           "games": 777}}
              ],
              "match_up": {
                "strong_against": [
                  {"games": 120, "win_rate": 0.6, "match_rate": 0.01, "enemy_champion_id": 5}
                ],
                "weak_against": [
                  {"games": 200, "win_rate": 0.42, "match_rate": 0.02, "enemy_champion_id": 64},
                  {"games": 10, "win_rate": 0.43, "match_rate": 0.001, "enemy_champion_id": 76},
                  {"games": 150, "win_rate": 0.46, "match_rate": 0.015, "enemy_champion_id": 121}
                ]
              }
            }
          }
        }"#;
    let b: BuildResponse = serde_json::from_str(json).expect("build must parse");
    let (lane, lb) = pick_lane(&b, Some("jungle")).expect("Jungle lane should be picked");
    assert_eq!(lane, "Jungle");
    assert_eq!(lb.build_lst[0].spell.build, vec![11, 4]);
    assert_eq!(lb.build_lst[0].skill.build, vec![3, 1, 2]);
    assert_eq!(lb.build_lst[0].skill.detail, vec![3, 1, 2, 3, 3, 4]);
    assert_eq!(lb.build_lst[0].skill.games, 777);

    let counters = counter_entries(lb);
    // 76 is dropped (< 30 games); order (worst-for-subject first) kept.
    assert_eq!(counters.len(), 2);
    assert_eq!(counters[0].champion_id, 64);
    // win_rate is inverted to the counter champion's perspective.
    assert!((counters[0].win_rate - 0.58).abs() < 1e-9);
    assert_eq!(counters[0].games, 200);
    assert_eq!(counters[1].champion_id, 121);
    assert!((counters[1].win_rate - 0.54).abs() < 1e-9);
}

#[test]
fn matchup_stats_null_positions_survive_parse() {
    // Invalid pairs come back as 200 + `"stats_by_position": null`.
    let r: MatchupStatsResponse =
        serde_json::from_str(r#"{"stats_by_position": null}"#).expect("null must parse");
    assert!(r.stats_by_position.is_empty());

    let r: MatchupStatsResponse = serde_json::from_str(
        r#"{"stats_by_position": {"Middle": {"games": 1468, "my_win_rate": 51.57,
                "enemy_win_rate": 48.43}}}"#,
    )
    .expect("stats must parse");
    let mid = &r.stats_by_position["Middle"];
    assert_eq!(mid.games, 1468);
    // Percent 0–100 here, not a fraction.
    assert!((mid.my_win_rate - 51.57).abs() < 1e-9);
}

#[test]
fn otp_match_parses_and_flags_incomplete_runes() {
    let json = r#"{"match_up_list": [
          {"position": "Jungle", "win": 1, "tier": "MASTER",
           "rune": {"perk_0": 8010, "perk_1": 9111, "perk_2": 9104, "perk_3": 8299,
                    "perk_4": 8473, "perk_5": 8451, "perk_primary_style": 8000,
                    "perk_sub_style": 8400, "stat_perk_0": 5005, "stat_perk_1": 5008,
                    "stat_perk_2": 5001},
           "spell": {"spell_1": 11, "spell_2": 4}},
          {"position": "Jungle", "win": 0, "rune": null, "spell": null}
        ]}"#;
    let r: OtpResponse = serde_json::from_str(json).expect("OTP must parse");
    assert_eq!(r.match_up_list.len(), 2);
    assert!(r.match_up_list[0].rune.is_complete());
    assert_eq!(r.match_up_list[0].spell.spell_1, 11);
    // A null rune block zeroes out → incomplete → excluded from aggregation.
    assert!(!r.match_up_list[1].rune.is_complete());
}

/// Convenience builder for aggregation tests.
fn otp(
    primary: i64,
    keystone: i64,
    p1: i64,
    sub_style: i64,
    p4: i64,
    p5: i64,
    spells: (i64, i64),
) -> OtpEntry {
    OtpEntry {
        position: "Jungle".into(),
        win: 1,
        rune: OtpRune {
            perk_0: keystone,
            perk_1: p1,
            perk_2: 9104,
            perk_3: 8299,
            perk_4: p4,
            perk_5: p5,
            perk_primary_style: primary,
            perk_sub_style: sub_style,
            stat_perk_0: 5005,
            stat_perk_1: 5008,
            stat_perk_2: 5001,
        },
        spell: OtpSpell {
            spell_1: spells.0,
            spell_2: spells.1,
        },
    }
}

#[test]
fn matchup_aggregation_groups_modes_and_spell_pairs() {
    let entries = vec![
        // Group A: Precision + keystone 8010 (3 games — wins).
        otp(8000, 8010, 9111, 8400, 8473, 8451, (11, 4)),
        otp(8000, 8010, 9111, 8400, 8473, 8451, (4, 11)),
        // Same group, but secondary tree differs — its minors must not
        // leak into the secondary-slot modes.
        otp(8000, 8010, 9104, 8100, 8139, 8135, (11, 4)),
        // Group B: Domination + keystone 8112 (2 games — loses).
        otp(8100, 8112, 9111, 8000, 9111, 8009, (11, 12)),
        otp(8100, 8112, 9111, 8000, 9111, 8009, (11, 12)),
    ];
    let refs: Vec<&OtpEntry> = entries.iter().collect();
    let page = aggregate_otp(&refs).expect("aggregation must produce a page");

    assert_eq!(page.primary_style, 8000); // 3-game group beats 2-game group
    assert_eq!(page.primary_perks[0], 8010); // the group's keystone
    assert_eq!(page.primary_perks[1], 9111); // per-slot mode (2 vs 1)
    assert_eq!(page.sub_style, 8400); // modal secondary tree
    assert_eq!(page.sub_perks, vec![8473, 8451]); // 8100-tree game excluded
    assert_eq!(page.shards, vec![5005, 5008, 5001]);
    // Spells: Smite/Flash appears 3× ([11,4] twice + [4,11] once) vs
    // [11,12] twice — the pair counting must merge orientations and the
    // output must keep the more common one.
    assert_eq!(page.spells, vec![11, 4]);
}

#[test]
fn counter_inversion_caps_at_eight() {
    let weak: Vec<MatchUpEntry> = (0..12)
        .map(|i| MatchUpEntry {
            games: 100,
            win_rate: 0.40 + i as f64 * 0.01,
            match_rate: 0.01,
            enemy_champion_id: 1000 + i,
        })
        .collect();
    let lb = LaneBuild {
        match_up: MatchUp {
            strong_against: vec![],
            weak_against: weak,
        },
        ..Default::default()
    };
    let counters = counter_entries(&lb);
    assert_eq!(counters.len(), 8);
    // Best counter (subject's worst matchup) stays first.
    assert_eq!(counters[0].champion_id, 1000);
    assert!((counters[0].win_rate - 0.60).abs() < 1e-9);
}

// ---- live tests (network) ----
//
// Decisive end-to-end checks against the real DeepLoL + Data Dragon APIs.
// Ignored by default; run with:
//   cargo test -p overlay-provider-deeplol --lib -- --ignored --nocapture

/// Shared harness for the live tests: one provider, one runtime.
fn live<F, Fut>(f: F)
where
    F: FnOnce(DeepLolProvider) -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let ddragon = Arc::new(DdragonClient::new());
    rt.block_on(f(DeepLolProvider::new(ddragon).unwrap()));
}

#[test]
#[ignore]
fn live_champion_names() {
    // The mock scenarios lean on both name directions; Wukong is the case
    // where display name and Data Dragon image id actually differ.
    live(|p| async move {
        let (name, image) = p.champion_names(62).await.expect("Wukong names");
        assert_eq!(name, "Wukong");
        assert_eq!(image, "MonkeyKing");
        let (name, image) = p.champion_names(31).await.expect("Cho'Gath names");
        assert_eq!(name, "Cho'Gath");
        assert_eq!(image, "Chogath");
        assert!(p.champion_names(-1).await.is_none());
        println!("CHAMPION NAMES OK");
    });
}

#[test]
#[ignore]
fn live_zed_items() {
    live(|p| async move {
        let snap = GameSnapshot {
            game_mode: "CLASSIC".into(),
            game_time: 600.0,
            self_champion: "Zed".into(),
            self_raw_name: "Zed".into(),
            self_position: "MIDDLE".into(),
            enemies: vec![],
            allies: vec![],
            players: vec![],
        };
        match p.items(&snap).await {
            Ok(items) => {
                println!("ITEMS OK ({}):", items.len());
                for it in &items {
                    println!("  {} [{}] {}", it.item_id, it.name, it.reason);
                }
                assert!(!items.is_empty());
            }
            Err(e) => panic!("items() failed: {e}"),
        }
        match p.runes(238, Some("middle")).await {
            Ok(r) => println!(
                "RUNES OK: {} primary={} sub={} perks={:?}",
                r.name, r.primary_style_id, r.sub_style_id, r.selected_perk_ids
            ),
            Err(e) => panic!("runes() failed: {e}"),
        }
    });
}

#[test]
#[ignore]
fn live_openlol_tier_list() {
    live(|p| async move {
        let rows = p.tier_list("jungle").await.expect("tier_list failed");
        println!("TIER LIST OK ({} rows):", rows.len());
        for r in rows.iter().take(8) {
            println!(
                "  {:>4} wr={:.3} d={:?} pr={:.3} br={:.3} games={:?}",
                r.champion_id, r.win_rate, r.win_rate_delta, r.pick_rate, r.ban_rate, r.games
            );
        }
        assert!(!rows.is_empty());
        for r in &rows {
            assert!(r.champion_id > 0);
            assert!(r.win_rate > 0.0 && r.win_rate < 1.0, "wr {}", r.win_rate);
            assert!(r.pick_rate >= 0.005 && r.pick_rate <= 1.0);
            assert!(r.ban_rate >= 0.0 && r.ban_rate <= 1.0);
            assert!(r.games.is_none_or(|games| games >= 0));
        }
        assert!(
            rows.windows(2).all(|w| w[0].win_rate >= w[1].win_rate),
            "tier list must be sorted by win rate desc"
        );
        assert!(
            rows.iter().any(|r| r.games.is_some_and(|games| games > 0)),
            "games calibration produced no estimates"
        );
        // Second invoke must come from the cache (and stay identical).
        let again = p
            .tier_list("jungle")
            .await
            .expect("cached tier_list failed");
        assert_eq!(again.len(), rows.len());
    });
}

#[test]
#[ignore]
fn live_openlol_counters() {
    live(|p| async move {
        // Who counters Shaco (35) in the jungle?
        let counters = p.counters(35, "jungle").await.expect("counters failed");
        println!("COUNTERS OK ({}):", counters.len());
        for c in &counters {
            println!(
                "  {:>4} wr={:.3} games={}",
                c.champion_id, c.win_rate, c.games
            );
        }
        assert!(!counters.is_empty());
        assert!(counters.len() <= 8);
        for c in &counters {
            assert!(c.champion_id > 0);
            assert!(c.win_rate > 0.0 && c.win_rate < 1.0);
            assert!(c.games >= MIN_MATCHUP_GAMES);
        }
    });
}

#[test]
#[ignore]
fn live_openlol_rune_build() {
    live(|p| async move {
        // Viego (234) jungle, no enemy → the plain best-build page.
        let b = p
            .rune_build(234, Some("jungle"), None)
            .await
            .expect("rune_build failed");
        println!("RUNE BUILD OK: {b:?}");
        assert!(b.page_name.starts_with("OPENLOL Viego"), "{}", b.page_name);
        assert_eq!(b.lane, "Jungle");
        assert!(b.primary_style_id > 0 && b.sub_style_id > 0);
        assert_eq!(b.primary_perk_ids.len(), 4);
        assert_eq!(b.sub_perk_ids.len(), 2);
        assert_eq!(b.shard_ids.len(), 3);
        assert_eq!(b.spell_ids.len(), 2, "expected a spell pair");
        assert!(b.win_rate > 0.0 && b.win_rate < 1.0);
        assert!(b.games > 0);
        assert!(!b.matchup);
        // runes() is a shim over rune_build(): same data, flat LCU shape.
        let r = p.runes(234, Some("jungle")).await.expect("runes failed");
        assert_eq!(r.primary_style_id, b.primary_style_id);
        assert_eq!(r.selected_perk_ids.len(), 9);
    });
}

#[test]
#[ignore]
fn live_current_mock_pick_rune_build() {
    live(|p| async move {
        let rows = p.tier_list("jungle").await.expect("tier_list failed");
        let champion_id = rows.first().expect("jungle tier list empty").champion_id;
        let b = p
            .rune_build(champion_id, Some("jungle"), None)
            .await
            .expect("current mock pick rune_build failed");
        println!("MOCK PICK RUNE BUILD OK: champion={champion_id} {b:?}");
        assert!(b.primary_style_id > 0 && b.sub_style_id > 0);
        assert_eq!(b.primary_perk_ids.len(), 4);
        assert_eq!(b.sub_perk_ids.len(), 2);
        assert_eq!(b.shard_ids.len(), 3);
        assert!(b.win_rate > 0.0 && b.win_rate < 1.0);
        assert!(b.games > 0);
    });
}

#[test]
#[ignore]
fn live_current_jungle_tier_rune_builds() {
    live(|p| async move {
        let rows = p.tier_list("jungle").await.expect("tier_list failed");
        for row in rows.iter().take(12) {
            let b = p
                .rune_build(row.champion_id, Some("jungle"), None)
                .await
                .unwrap_or_else(|e| {
                    panic!(
                        "rune_build failed for current jungle tier champion {}: {e}",
                        row.champion_id
                    )
                });
            println!(
                "JUNGLE RUNE OK: champion={} lane={} games={}",
                row.champion_id, b.lane, b.games
            );
            assert_eq!(b.primary_perk_ids.len(), 4);
            assert_eq!(b.sub_perk_ids.len(), 2);
            assert_eq!(b.shard_ids.len(), 3);
        }
    });
}

#[test]
#[ignore]
fn live_jp1_region_falls_back_to_kr_builds() {
    live(|p| async move {
        p.set_platform_id("JP1");

        let zyra = p
            .rune_build(143, Some("jungle"), None)
            .await
            .expect("JP1 Zyra should fall back to KR build data");
        println!("JP1 FALLBACK RUNE OK: {zyra:?}");
        assert_eq!(zyra.lane, "Jungle");
        assert_eq!(zyra.primary_perk_ids.len(), 4);
        assert_eq!(zyra.sub_perk_ids.len(), 2);
        assert_eq!(zyra.shard_ids.len(), 3);

        let counters = p
            .counters(200, "jungle")
            .await
            .expect("JP1 Aurora counters should fall back to KR build data");
        println!("JP1 FALLBACK COUNTERS OK: {}", counters.len());
        assert!(!counters.is_empty());
    });
}

#[test]
#[ignore]
fn live_openlol_matchup_build() {
    live(|p| async move {
        // Viego (234) vs Shaco (35) in the jungle. The matchup may
        // legitimately be too thin at the current patch, so the only
        // acceptable failure is exactly NotEnoughData.
        match p.rune_build(234, Some("jungle"), Some(35)).await {
            Ok(b) => {
                println!("MATCHUP BUILD OK: {b:?}");
                assert!(b.matchup);
                assert!(b.page_name.contains(" vs "), "{}", b.page_name);
                assert!(b.primary_style_id > 0 && b.sub_style_id > 0);
                assert_eq!(b.primary_perk_ids.len(), 4);
                assert_eq!(b.sub_perk_ids.len(), 2);
                assert_eq!(b.shard_ids.len(), 3);
                assert!(b.win_rate > 0.0 && b.win_rate < 1.0);
                assert!(b.games >= MIN_MATCHUP_GAMES);
            }
            Err(ProviderError::NotEnoughData) => {
                println!("MATCHUP: not enough data (acceptable outcome)");
            }
            Err(e) => panic!("matchup rune_build failed unexpectedly: {e}"),
        }
    });
}
