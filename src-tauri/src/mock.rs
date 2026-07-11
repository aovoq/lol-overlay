//! Debug/mock mode (Ctrl+Shift+D cycles: off → champ select → in game → off).
//!
//! Drives the overlay with synthetic state through the *real* provider
//! pipeline, so both the OPENLOL champ-select panel and the in-game UI can be
//! exercised without launching League (e.g. on macOS).
//!
//! The scenarios themselves are built from **live DeepLoL data**: the current
//! meta's strongest picks fill the lanes and the highest ban-rate champions
//! fill the ban slots, so debugging always looks at the same numbers a real
//! champ select would. Hardcoded champions remain only as an offline fallback.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use crate::engine::{self, Engine, MockStage};
use crate::events::{log, ChampSelectEvent, PhaseEvent, RecommendationsEvent, RuneImportedEvent};
use overlay_provider::{classify_threats, BuildProvider};
use overlay_types::{EnemyChampion, GameSnapshot};

/// Switch the mock scenario to `next` and spawn/stop the matching loop.
/// Shared by the Ctrl+Shift+D hotkey (cycling) and the debug-panel command
/// (direct jump). Emits `mock-stage` so the panel tracks hotkey changes too.
pub fn apply_stage(app: &AppHandle, engine: &Arc<Engine>, next: MockStage) {
    let generation = engine.set_mock_stage(next);
    log(app, "info", format!("mock stage: {next:?}"));
    let _ = app.emit("mock-stage", next.as_str());
    match next {
        MockStage::ChampSelect => {
            tauri::async_runtime::spawn(mock_champ_select_loop(
                app.clone(),
                engine.clone(),
                generation,
            ));
        }
        MockStage::InGame => {
            tauri::async_runtime::spawn(mock_loop(app.clone(), engine.clone(), generation));
        }
        MockStage::Off => {
            engine.phase_champselect.store(false, Ordering::SeqCst);
            engine.phase_in_game.store(false, Ordering::SeqCst);
            let _ = app.emit("champ-select", ChampSelectEvent::default());
            let _ = app.emit(
                "phase",
                PhaseEvent {
                    phase: "None".into(),
                    client_up: false,
                    in_game: false,
                },
            );
            engine::apply_desired_window_mode(app, engine);
        }
    }
}

/// Champ-select mock: me on the top meta jungler, the runner-up revealed as
/// the enemy jungler, real ban targets in the ban slots. The frontend then
/// drives tier list / counters / runes through the live provider, so the
/// whole panel is testable end to end.
pub async fn mock_champ_select_loop(app: AppHandle, engine: Arc<Engine>, generation: u64) {
    engine.phase_champselect.store(true, Ordering::SeqCst);
    engine.phase_in_game.store(false, Ordering::SeqCst);
    engine::apply_desired_window_mode(&app, &engine);

    let ev = champ_select_scenario(&engine).await;

    // Re-emit on a timer (like the in-game mock) so a stray event can't
    // strand the panel; stop as soon as the hotkey advances the cycle.
    while engine.mock_stage() == MockStage::ChampSelect && engine.mock_generation() == generation {
        let _ = app.emit(
            "phase",
            PhaseEvent {
                phase: "ChampSelect".into(),
                client_up: true,
                in_game: false,
            },
        );
        let _ = app.emit("champ-select", ev.clone());
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }

    if engine.mock_generation() != generation {
        return;
    }

    // Close the champ-select control. Only return to compact mode when the
    // cycle is actually leaving mock mode; advancing to InGame immediately
    // switches to the in-game overlay panel.
    let _ = app.emit("champ-select", ChampSelectEvent::default());
    if engine.mock_stage() == MockStage::Off {
        engine.phase_champselect.store(false, Ordering::SeqCst);
        engine::apply_desired_window_mode(&app, &engine);
    }
}

/// Build the champ-select scene from the live jungle tier list: strongest
/// pick = me, runner-up = the revealed enemy, highest ban rates = the bans.
async fn champ_select_scenario(engine: &Engine) -> ChampSelectEvent {
    // Offline fallback: Viego vs Shaco, fixed bans.
    let fallback = ChampSelectEvent {
        active: true,
        my_role: "jungle".into(),
        my_champion_id: 234,
        my_locked: false,
        my_team_champion_ids: vec![234, 0, 0, 0, 0],
        enemy_champion_ids: vec![35, 0, 0, 0, 0],
        my_bans: vec![266],
        enemy_bans: vec![157, 238],
        timer_phase: "BAN_PICK".into(),
    };

    let Ok(list) = engine.provider.tier_list("jungle").await else {
        return fallback;
    };
    if list.len() < 5 {
        return fallback;
    }
    let me = list[0].champion_id;
    let enemy = list[1].champion_id;

    // Ban what real lobbies ban: the remaining junglers by ban rate.
    let mut by_ban: Vec<_> = list
        .iter()
        .filter(|t| t.champion_id != me && t.champion_id != enemy)
        .collect();
    by_ban.sort_by(|a, b| b.ban_rate.total_cmp(&a.ban_rate));
    let bans: Vec<i64> = by_ban.iter().take(3).map(|t| t.champion_id).collect();

    ChampSelectEvent {
        active: true,
        my_role: "jungle".into(),
        my_champion_id: me,
        my_locked: false,
        my_team_champion_ids: vec![me, 0, 0, 0, 0],
        enemy_champion_ids: vec![enemy, 0, 0, 0, 0],
        my_bans: bans.get(2).map(|id| vec![*id]).unwrap_or_default(),
        enemy_bans: bans.iter().take(2).copied().collect(),
        timer_phase: "BAN_PICK".into(),
    }
}

/// In-game mock: re-emits synthetic state on a timer while its stage is active
/// (so a stray poller `phase` event can't hide the panel); clears the UI when
/// the cycle moves on.
pub async fn mock_loop(app: AppHandle, engine: Arc<Engine>, generation: u64) {
    engine.phase_champselect.store(false, Ordering::SeqCst);
    engine.phase_in_game.store(true, Ordering::SeqCst);
    engine::apply_desired_window_mode(&app, &engine);

    let (snapshot, my_champion_id) = ingame_scenario(&engine).await;
    let threats = classify_threats(&snapshot);
    let items = match engine.provider.items(&snapshot).await {
        Ok(items) => items,
        Err(e) => {
            log(&app, "warn", format!("mock items fetch failed: {e}"));
            Vec::new()
        }
    };
    let skill_order = match engine.provider.skill_order(&snapshot).await {
        Ok(skill_order) => Some(skill_order),
        Err(e) => {
            log(&app, "warn", format!("mock skill order fetch failed: {e}"));
            None
        }
    };

    // Rune banner once (it has its own auto-hide timer). Pull the real page
    // name through the provider so mock mode exercises `runes()` too.
    let page_name = engine
        .provider
        .runes(my_champion_id, Some("top"))
        .await
        .map(|r| r.name)
        .unwrap_or_else(|e| {
            log(&app, "warn", format!("mock runes fetch failed: {e}"));
            "Mock runes".into()
        });
    let _ = app.emit(
        "rune-imported",
        RuneImportedEvent {
            champion_id: my_champion_id,
            page_name,
        },
    );

    let recommendations = RecommendationsEvent {
        self_champion: snapshot.self_champion.clone(),
        self_raw_name: snapshot.self_raw_name.clone(),
        self_position: snapshot.self_position.clone(),
        enemies: snapshot.enemies.clone(),
        threats,
        skill_order,
        items,
    };

    while engine.mock_stage() == MockStage::InGame && engine.mock_generation() == generation {
        let _ = app.emit(
            "phase",
            PhaseEvent {
                phase: "InProgress".into(),
                client_up: true,
                in_game: true,
            },
        );
        let _ = app.emit("recommendations", recommendations.clone());
        engine
            .mobile
            .publish_game(&app, "InProgress", true, &snapshot, &recommendations);
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }

    if engine.mock_generation() != generation {
        return;
    }

    // Reset the UI on the way out of mock mode.
    let _ = app.emit(
        "phase",
        PhaseEvent {
            phase: "None".into(),
            client_up: false,
            in_game: false,
        },
    );
    engine.phase_in_game.store(false, Ordering::SeqCst);
    engine.mobile.publish_idle(&app, "None", false);
    engine::apply_desired_window_mode(&app, &engine);
}

/// Build the in-game scene from live tier lists: the top meta pick of every
/// lane as the enemy team, the strongest *other* top-laner as us. Returns the
/// snapshot plus our champion id (for the rune banner).
async fn ingame_scenario(engine: &Engine) -> (GameSnapshot, i64) {
    const LANES: [(&str, &str); 5] = [
        ("top", "TOP"),
        ("jungle", "JUNGLE"),
        ("middle", "MIDDLE"),
        ("bottom", "BOTTOM"),
        ("utility", "UTILITY"),
    ];

    let mut enemies = Vec::new();
    for (role, pos) in LANES {
        let Ok(list) = engine.provider.tier_list(role).await else {
            return (offline_snapshot(), 17);
        };
        let Some(top) = list.first() else {
            return (offline_snapshot(), 17);
        };
        let Some((name, image)) = engine.provider.champion_names(top.champion_id).await else {
            return (offline_snapshot(), 17);
        };
        enemies.push(EnemyChampion {
            name,
            raw_name: image,
            position: pos.into(),
            items: vec![],
        });
    }

    // Us: the strongest top-lane pick that isn't already the enemy top-laner.
    let Ok(top_list) = engine.provider.tier_list("top").await else {
        return (offline_snapshot(), 17);
    };
    let enemy_top = top_list.first().map(|t| t.champion_id).unwrap_or(0);
    let me = top_list.iter().find(|t| t.champion_id != enemy_top);
    let Some(me) = me else {
        return (offline_snapshot(), 17);
    };
    let Some((my_name, my_image)) = engine.provider.champion_names(me.champion_id).await else {
        return (offline_snapshot(), 17);
    };

    (
        GameSnapshot {
            game_mode: "CLASSIC (mock)".into(),
            game_time: 600.0,
            self_champion: my_name,
            self_raw_name: my_image,
            self_position: "TOP".into(),
            enemies,
            allies: vec![],
        },
        me.champion_id,
    )
}

/// Offline fallback comp with valid Data Dragon id names (so icons load):
/// 2 AD (Zed, Jinx), 2 AP (Ahri, Lux), 1 tank (Malphite); us on Teemo top.
fn offline_snapshot() -> GameSnapshot {
    let mk = |name: &str, pos: &str| EnemyChampion {
        name: name.into(),
        raw_name: name.into(),
        position: pos.into(),
        items: vec![],
    };
    GameSnapshot {
        game_mode: "CLASSIC (mock)".into(),
        game_time: 600.0,
        self_champion: "Teemo".into(),
        self_raw_name: "Teemo".into(),
        self_position: "TOP".into(),
        enemies: vec![
            mk("Zed", "MIDDLE"),
            mk("Ahri", "MIDDLE"),
            mk("Malphite", "TOP"),
            mk("Lux", "UTILITY"),
            mk("Jinx", "BOTTOM"),
        ],
        allies: vec![],
    }
}
