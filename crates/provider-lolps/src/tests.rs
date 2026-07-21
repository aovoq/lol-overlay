use super::*;
use crate::api::{decode_versions, LolpsApi};
use crate::types::{SummaryResponse, TierResponse, TierRow, VersionInfo};
use serde_json::json;
use std::io::{Read, Write as _};
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

overlay_provider::build_provider_contract_suite!(lolps_shared_contract, "lolps");

fn summary() -> SummaryRow {
    SummaryRow {
        build_type_id: 0,
        champion_id: 41,
        lane_id: 0,
        count: 18_026,
        win_rate: "50.85".into(),
        counter_champion_id_list: vec![240, 79, 2],
        counter_winrate_list: vec![43.6, 44.44, 45.45],
        counter_count_list: vec![211, 29, 440],
        main_rune_category: Some(8200),
        sub_rune_category: Some(8000),
        main_rune1: Some(8992),
        main_rune2: Some(8275),
        main_rune3: Some(8210),
        main_rune4: Some(8237),
        sub_rune1: Some(8017),
        sub_rune2: Some(9105),
        statperk1_id: Some(5008),
        statperk2_id: Some(5008),
        statperk3_id: Some(5011),
        spell1_id: Some(4),
        spell2_id: Some(14),
        skill_master_list: vec!["Q".into(), "E".into(), "W".into()],
        skill_lv15_list: [
            "E", "Q", "W", "Q", "Q", "R", "Q", "E", "Q", "E", "R", "E", "E", "W", "W",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        starting_item_id_list: vec![vec![1055], vec![2003, 2003]],
        core_item_id_list: vec![3508, 6676, 3031],
        shoes_id: Some(3047),
        rune_total_winrate: "48.39".into(),
        rune_total_count: 6666,
        skill_master_winrate: "51.64".into(),
        skill_master_count: 17_138,
        top1_three_core_winrate: Some("56.19".into()),
        top1_three_core_count: Some(2214),
    }
}

fn selected() -> Selected<SummaryRow> {
    Selected {
        value: summary(),
        version: VersionInfo {
            version_id: 151,
            description: "26.14".into(),
            patch_date: "2026-07-15".into(),
        },
        fallback_from: None,
    }
}

fn version_payload() -> String {
    json!({
        "type": "data",
        "nodes": [null, {"type": "data", "data": [
            {"versionInfo": 1},
            [2, 7],
            {"versionId": 3, "description": 4, "patchDate": 5, "isActive": 6},
            151, "26.14", "2026-07-15", true,
            {"versionId": 8, "description": 9, "patchDate": 10, "isActive": 11},
            150, "26.13", "2026-06-24", false
        ]}]
    })
    .to_string()
}

struct TestResponse {
    status: &'static str,
    headers: Vec<(&'static str, &'static str)>,
    body: String,
}

type TestServer = (
    String,
    Arc<AtomicUsize>,
    Arc<Mutex<Vec<String>>>,
    JoinHandle<()>,
);

fn test_server(responses: Vec<TestResponse>) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
    let address = listener.local_addr().expect("test server address");
    let count = Arc::new(AtomicUsize::new(0));
    let paths = Arc::new(Mutex::new(Vec::new()));
    let count_for_thread = count.clone();
    let paths_for_thread = paths.clone();
    let handle = std::thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut request = [0_u8; 4096];
            let read = stream.read(&mut request).expect("read request");
            let request = String::from_utf8_lossy(&request[..read]);
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or_default()
                .to_string();
            paths_for_thread.lock().unwrap().push(path);
            count_for_thread.fetch_add(1, Ordering::SeqCst);
            let extra_headers =
                response
                    .headers
                    .iter()
                    .fold(String::new(), |mut headers, (name, value)| {
                        write!(headers, "{name}: {value}\r\n")
                            .expect("writing to String cannot fail");
                        headers
                    });
            write!(
                stream,
                "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
                response.status,
                response.body.len(),
                extra_headers,
                response.body
            )
            .expect("write response");
        }
    });
    (format!("http://{address}"), count, paths, handle)
}

#[test]
fn decodes_only_version_info_from_sveltekit_devalue_payload() {
    let payload = json!({
        "type": "data",
        "nodes": [null, {"type": "data", "data": [
            {"versionInfo": 1},
            [2, 7],
            {"versionId": 3, "description": 4, "patchDate": 5, "isActive": 6},
            151, "26.14", "2026-07-15", true,
            {"versionId": 8, "description": 9, "patchDate": 10, "isActive": 11},
            150, "26.13", "2026-06-24", false
        ]}]
    });
    let versions = decode_versions(&payload).expect("versions");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].version_id, 151);
    assert_eq!(versions[0].description, "26.14");
}

#[test]
fn lane_aliases_map_absence_to_primary_and_reject_unknown_values() {
    assert_eq!(lane_id("top"), Some(0));
    assert_eq!(lane_id("JUNGLE"), Some(1));
    assert_eq!(lane_id("mid"), Some(2));
    assert_eq!(lane_id("adc"), Some(3));
    assert_eq!(lane_id("utility"), Some(4));
    assert_eq!(optional_lane_id(None).unwrap(), -1);
    assert_eq!(optional_lane_id(Some("")).unwrap(), -1);
    assert!(matches!(
        optional_lane_id(Some("aram")),
        Err(ProviderError::Other(message)) if message == "unknown role: \"aram\""
    ));
}

#[test]
fn item_order_flattens_starting_then_core_then_shoes() {
    assert_eq!(
        item_ids(&summary()),
        vec![1055, 2003, 2003, 3508, 6676, 3031, 3047]
    );
    let recommendations =
        item_recommendations(item_ids(&summary()), |_| "item".into(), 56.19, 2214);
    let mut recommendations = recommendations;
    apply_item_reasons(
        &mut recommendations,
        &summary(),
        0.5619,
        2214,
        "3-core build",
        Some("26.13"),
    );
    assert_eq!(
        recommendations
            .iter()
            .map(|item| item.item_id)
            .collect::<Vec<_>>(),
        vec![1055, 2003, 3508, 6676, 3031, 3047]
    );
    assert_eq!(
        recommendations[0].reason,
        "Starting item · 3-core build 56% WR · 2214 games · fallback from 26.13"
    );
    assert_eq!(recommendations[2].reason, "Core item");
    assert_eq!(recommendations[5].reason, "Boots");
}

#[test]
fn item_reason_precedence_prefers_starting_then_boots_over_core() {
    let mut row = summary();
    // Boots also listed as a core item, and a core item also listed as a
    // starting item: starting wins over core, boots wins over core.
    row.starting_item_id_list = vec![vec![3508]];
    row.core_item_id_list = vec![3508, 3047, 3031];
    let mut recommendations = item_recommendations(item_ids(&row), |_| "item".into(), 50.0, 100);
    apply_item_reasons(&mut recommendations, &row, 0.5, 100, "champion stats", None);
    let reasons = recommendations
        .iter()
        .map(|item| (item.item_id, item.reason.as_str()))
        .collect::<Vec<_>>();
    assert_eq!(
        reasons,
        vec![
            (3508, "Starting item · champion stats 50% WR · 100 games"),
            (3047, "Boots"),
            (3031, "Core item"),
        ]
    );
}

#[test]
fn maps_skill_rune_and_counter_contracts() {
    let selected = selected();
    let skills = skill_order(&selected.value).expect("skills");
    assert_eq!(skills.max_order, vec![1, 3, 2]);
    assert_eq!(
        skills.level_order,
        vec![3, 1, 2, 1, 1, 4, 1, 3, 1, 3, 4, 3, 3, 2, 2]
    );
    assert!((skills.win_rate - 0.5164).abs() < 1e-9);

    let runes = rune_build(&selected, "Top").expect("runes");
    assert_eq!(runes.primary_perk_ids, vec![8992, 8275, 8210, 8237]);
    assert_eq!(runes.sub_perk_ids, vec![8017, 9105]);
    assert_eq!(runes.shard_ids, vec![5008, 5008, 5011]);
    assert_eq!(runes.spell_ids, vec![4, 14]);
    assert!((runes.win_rate - 0.4839).abs() < 1e-9);

    let counters = counter_entries(&selected.value).expect("counters");
    assert_eq!(counters.len(), 2);
    assert_eq!(counters[0].champion_id, 240);
    assert!((counters[0].win_rate - 0.564).abs() < 1e-9);
    assert!(counters.iter().all(|counter| counter.games >= 30));
}

#[test]
fn maps_tier_percentages_and_provenance_without_rank_delta() {
    let response = Arc::new(TierResponse { data: vec![] });
    let selected = Selected {
        value: response,
        version: VersionInfo {
            version_id: 150,
            description: "26.13".into(),
            patch_date: "2026-06-24".into(),
        },
        fallback_from: Some("26.14".into()),
    };
    let row = TierRow {
        champion_id: 41,
        lane_id: 0,
        count: 18_026,
        win_rate: "50.85".into(),
        pick_rate: "7.60".into(),
        ban_rate: "18.05".into(),
    };
    let entry = tier_entry(&row, &selected).expect("tier entry");
    assert!((entry.win_rate - 0.5085).abs() < 1e-9);
    assert!((entry.pick_rate - 0.076).abs() < 1e-9);
    assert_eq!(entry.win_rate_delta, None);
    assert_eq!(entry.provenance.provider, "lolps");
    assert_eq!(entry.provenance.region.as_deref(), Some("KR"));
    assert_eq!(entry.provenance.rank.as_deref(), Some("Emerald+"));
    assert_eq!(entry.provenance.fallback_from.as_deref(), Some("26.14"));
}

#[test]
fn selects_only_populated_default_build_type() {
    let mut alternate = summary();
    alternate.build_type_id = 1;
    let mut empty_default = summary();
    empty_default.count = 0;
    let response = SummaryResponse {
        data: vec![alternate, empty_default],
    };
    assert!(matches!(
        crate::api::select_summary(&response, 41, 0),
        Err(ProviderError::NotEnoughData)
    ));
}

#[test]
fn primary_role_selection_uses_the_largest_sample() {
    let mut smaller = summary();
    smaller.lane_id = 2;
    smaller.count = 100;
    let mut larger = summary();
    larger.lane_id = 0;
    larger.count = 500;
    let response = SummaryResponse {
        data: vec![smaller, larger],
    };
    let selected = crate::api::select_summary(&response, 41, -1).expect("primary role");
    assert_eq!(selected.lane_id, 0);
    assert_eq!(selected.count, 500);
}

#[test]
fn skill_order_requires_all_fifteen_levels() {
    let mut row = summary();
    row.skill_lv15_list.pop();
    assert!(matches!(
        skill_order(&row),
        Err(ProviderError::NotEnoughData)
    ));
}

#[test]
fn nullable_ids_in_a_zero_sample_row_are_not_a_schema_error() {
    let mut empty = summary();
    empty.count = 0;
    empty.main_rune_category = None;
    empty.sub_rune_category = None;
    empty.main_rune1 = None;
    empty.main_rune2 = None;
    empty.main_rune3 = None;
    empty.main_rune4 = None;
    empty.sub_rune1 = None;
    empty.sub_rune2 = None;
    empty.statperk1_id = None;
    empty.statperk2_id = None;
    empty.statperk3_id = None;
    empty.spell1_id = None;
    empty.spell2_id = None;
    empty.shoes_id = None;
    let body = serde_json::to_string(&SummaryResponse { data: vec![empty] }).unwrap();
    let response: SummaryResponse = serde_json::from_str(&body).expect("nullable summary");
    assert!(matches!(
        crate::api::select_summary(&response, 41, 0),
        Err(ProviderError::NotEnoughData)
    ));
}

#[tokio::test]
async fn falls_back_to_previous_patch_only_for_an_empty_current_summary() {
    let previous = serde_json::to_string(&SummaryResponse {
        data: vec![summary()],
    })
    .unwrap();
    let (base, count, paths, server) = test_server(vec![
        TestResponse {
            status: "200 OK",
            headers: vec![],
            body: version_payload(),
        },
        TestResponse {
            status: "200 OK",
            headers: vec![],
            body: r#"{"data":[]}"#.into(),
        },
        TestResponse {
            status: "200 OK",
            headers: vec![],
            body: previous,
        },
    ]);
    let api = LolpsApi::with_base_url(&base).unwrap();
    let selected = api.summary(41, 0).await.expect("fallback summary");
    assert_eq!(selected.version.version_id, 150);
    assert_eq!(selected.fallback_from.as_deref(), Some("26.14"));
    server.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 3);
    assert!(paths.lock().unwrap()[2].contains("version=150"));
}

#[tokio::test]
async fn schema_errors_do_not_query_the_previous_patch() {
    let (base, count, _paths, server) = test_server(vec![
        TestResponse {
            status: "200 OK",
            headers: vec![],
            body: version_payload(),
        },
        TestResponse {
            status: "200 OK",
            headers: vec![],
            body: r#"{"unexpected":true}"#.into(),
        },
    ]);
    let api = LolpsApi::with_base_url(&base).unwrap();
    assert!(matches!(
        api.summary(41, 0).await,
        Err(ProviderError::InvalidData(_))
    ));
    server.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn retries_server_errors_and_caches_version_metadata() {
    let (base, count, _paths, server) = test_server(vec![
        TestResponse {
            status: "500 Internal Server Error",
            headers: vec![],
            body: r#"{"error":"temporary"}"#.into(),
        },
        TestResponse {
            status: "200 OK",
            headers: vec![],
            body: version_payload(),
        },
    ]);
    let api = LolpsApi::with_base_url(&base).unwrap();
    assert_eq!(api.versions().await.unwrap()[0].version_id, 151);
    assert_eq!(api.versions().await.unwrap()[0].version_id, 151);
    server.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn reports_rate_limits_with_retry_after() {
    let (base, count, _paths, server) = test_server(vec![TestResponse {
        status: "429 Too Many Requests",
        headers: vec![("Retry-After", "17")],
        body: r#"{"error":"slow down"}"#.into(),
    }]);
    let api = LolpsApi::with_base_url(&base).unwrap();
    assert!(matches!(
        api.versions().await,
        Err(ProviderError::RateLimited {
            retry_after: Some(17)
        })
    ));
    server.join().unwrap();
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn matchup_runes_are_rejected_before_network_access() {
    let provider = LolpsProvider::new(Arc::new(DdragonClient::new())).unwrap();
    assert!(matches!(
        provider.rune_build(41, Some("top"), Some(86)).await,
        Err(ProviderError::NotEnoughData)
    ));
}

#[tokio::test]
async fn unknown_roles_are_rejected_before_network_access() {
    let provider = LolpsProvider::new(Arc::new(DdragonClient::new())).unwrap();
    let snapshot = GameSnapshot {
        game_mode: "CLASSIC".into(),
        game_time: 0.0,
        self_champion: "Gangplank".into(),
        self_raw_name: "Gangplank".into(),
        self_position: "aram".into(),
        enemies: vec![],
        allies: vec![],
        players: vec![],
    };
    assert!(matches!(
        provider.items(&snapshot).await,
        Err(ProviderError::Other(_))
    ));
    assert!(matches!(
        provider.skill_order(&snapshot).await,
        Err(ProviderError::Other(_))
    ));
    assert!(matches!(
        provider.rune_build(41, Some("aram"), None).await,
        Err(ProviderError::Other(_))
    ));
    assert!(matches!(
        provider.tier_list("aram").await,
        Err(ProviderError::Other(_))
    ));
    assert!(matches!(
        provider.counters(41, "aram").await,
        Err(ProviderError::Other(_))
    ));
}

#[tokio::test]
#[ignore = "network: live LOL.PS latest-patch Gangplank top build and tier list"]
async fn fetch_live_gangplank_top_build_and_tier_list() {
    let provider = LolpsProvider::new(Arc::new(DdragonClient::new())).expect("provider");
    let snapshot = GameSnapshot {
        game_mode: "CLASSIC".into(),
        game_time: 600.0,
        self_champion: "Gangplank".into(),
        self_raw_name: "Gangplank".into(),
        self_position: "top".into(),
        enemies: vec![],
        allies: vec![],
        players: vec![],
    };
    assert!(!provider.items(&snapshot).await.expect("items").is_empty());
    assert!(!provider
        .skill_order(&snapshot)
        .await
        .expect("skills")
        .max_order
        .is_empty());
    assert_eq!(
        provider
            .rune_build(41, Some("top"), None)
            .await
            .expect("runes")
            .primary_perk_ids
            .len(),
        4
    );
    assert!(!provider
        .counters(41, "top")
        .await
        .expect("counters")
        .is_empty());
    assert!(!provider.tier_list("top").await.expect("tier").is_empty());
}
