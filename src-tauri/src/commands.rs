//! Commands invoked from the frontend (`@tauri-apps/api` `invoke`).
//!
//! The OPENLOL data commands are thin proxies to the provider — it caches
//! per (patch, role, champion), so after the first load these are instant.
//! Errors cross the boundary as their `Display` string; the frontend branches
//! on the literal `"not-enough-data"` (`Error::NotEnoughData`).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::engine::{
    self, Engine, MockStage, PanelPosition, PresentationMode, Settings, UiLayout, WindowGeometry,
};
use crate::error;
use crate::events::{log, PhaseEvent, RuneImportedEvent};
use crate::hittest::HitRegion;
use crate::mobile::MobilePairingState;
use overlay_lcu::{self as lcu, RunePagePayload};
use overlay_provider::{BuildProvider, PlayerStatsProvider, ProviderKind};
use overlay_types::PlayerRef;
use overlay_types::{
    CounterEntry, EnemyChampion, GameSnapshot, ItemRecommendation, MatchPage, PlayerChampionStats,
    PlayerProfile, RuneBuild, SkillOrder, TierEntry,
};
use serde::Serialize;

/// An empty role string means "unknown" on the frontend; the provider's
/// optional-role APIs take `None` for that.
fn role_opt(role: &str) -> Option<&str> {
    if role.is_empty() {
        None
    } else {
        Some(role)
    }
}

#[tauri::command]
pub fn get_settings(engine: State<'_, Arc<Engine>>) -> Settings {
    engine.settings()
}

#[tauri::command]
pub fn get_current_player_ref(engine: State<'_, Arc<Engine>>) -> Option<PlayerRef> {
    engine.current_player_ref()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    phase: PhaseEvent,
    champ_select: overlay_types::ChampSelectEvent,
    window_mode: String,
}

#[tauri::command]
pub fn get_app_snapshot(engine: State<'_, Arc<Engine>>) -> AppSnapshot {
    let champ_select = engine.last_champ_select.lock().clone().unwrap_or_default();
    let in_game = engine.phase_in_game.load(Ordering::SeqCst);
    let is_draft_phase = engine.phase_champselect.load(Ordering::SeqCst);
    let phase = engine
        .last_phase
        .lock()
        .clone()
        .unwrap_or_else(|| PhaseEvent {
            phase: if is_draft_phase {
                "ChampSelect"
            } else if in_game {
                "InProgress"
            } else {
                "None"
            }
            .into(),
            client_up: is_draft_phase || in_game,
            in_game,
        });
    AppSnapshot {
        phase,
        champ_select,
        window_mode: engine::current_window_mode(&engine).as_str().into(),
    }
}

#[tauri::command]
pub fn set_auto_import(engine: State<'_, Arc<Engine>>, enabled: bool) -> error::Result<()> {
    {
        engine.settings.lock().auto_import_runes = enabled;
    }
    engine.persist()
}

#[tauri::command]
pub fn set_import_spells(engine: State<'_, Arc<Engine>>, enabled: bool) -> error::Result<()> {
    {
        engine.settings.lock().import_spells = enabled;
    }
    engine.persist()
}

#[tauri::command]
pub fn set_spells_flipped(engine: State<'_, Arc<Engine>>, flipped: bool) -> error::Result<()> {
    {
        engine.settings.lock().spells_flipped = flipped;
    }
    engine.persist()
}

#[tauri::command]
pub fn set_auto_open_champion(engine: State<'_, Arc<Engine>>, enabled: bool) -> error::Result<()> {
    engine.settings.lock().auto_open_champion = enabled;
    engine.persist()
}

#[tauri::command]
pub fn set_auto_open_live(engine: State<'_, Arc<Engine>>, enabled: bool) -> error::Result<()> {
    engine.settings.lock().auto_open_live = enabled;
    engine.persist()
}

#[tauri::command]
pub fn set_presentation_mode(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    mode: String,
) -> error::Result<()> {
    let parsed = PresentationMode::parse(&mode)
        .ok_or_else(|| error::Error::Other(format!("unknown presentation mode: {mode}")))?;
    {
        engine.settings.lock().presentation_mode = parsed;
    }
    engine.persist()?;
    engine::apply_desired_window_mode(&app, &engine);
    Ok(())
}

#[tauri::command]
pub fn get_ui_layout(engine: State<'_, Arc<Engine>>) -> UiLayout {
    engine.ui_layout()
}

#[tauri::command]
pub fn set_ingame_panel_position(
    engine: State<'_, Arc<Engine>>,
    left: f64,
    top: f64,
) -> error::Result<()> {
    if !left.is_finite() || !top.is_finite() {
        return Err(error::Error::Other("invalid panel position".into()));
    }
    {
        engine.ui_layout.lock().ingame_panel = Some(PanelPosition { left, top });
    }
    engine.persist()
}

#[tauri::command]
pub fn set_control_window_geometry(
    engine: State<'_, Arc<Engine>>,
    mode: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> error::Result<()> {
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
    {
        return Err(error::Error::Other("invalid window geometry".into()));
    }
    let mode = engine::WindowMode::parse(&mode)
        .ok_or_else(|| error::Error::Other(format!("unknown window mode: {mode}")))?;
    {
        engine.ui_layout.lock().set_control_geometry(
            mode,
            WindowGeometry {
                x,
                y,
                width,
                height,
            },
        );
    }
    engine.persist()
}

/// Force the *whole* overlay window interactive. Normal mouse input is granted
/// per-region by `hittest::cursor_watcher` from the rects reported via
/// `set_hit_regions`.
#[tauri::command]
pub fn set_interactive(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    interactive: bool,
) -> error::Result<()> {
    engine
        .forced_interactive
        .store(interactive, Ordering::SeqCst);
    let _ = app.emit("interactive", interactive);
    Ok(())
}

/// Replace the set of clickable rects (the frontend's visible `data-hit`
/// elements, in window-relative CSS px). Called whenever their layout changes.
#[tauri::command]
pub fn set_hit_regions(engine: State<'_, Arc<Engine>>, regions: Vec<HitRegion>) {
    *engine.hit_regions.lock() = regions;
}

/// Hold the window interactive for the duration of a panel drag, where the
/// cursor can outrun the last reported rects.
#[tauri::command]
pub fn set_drag_active(engine: State<'_, Arc<Engine>>, active: bool) {
    engine.drag_active.store(active, Ordering::SeqCst);
}

#[tauri::command]
pub fn set_ingame_collapsed(engine: State<'_, Arc<Engine>>, collapsed: bool) -> error::Result<()> {
    {
        engine.ui_layout.lock().ingame_collapsed = collapsed;
    }
    engine.persist()
}

/// Tier list for a role (strong picks / ban targets).
#[tauri::command]
pub async fn get_tier_list(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    role: String,
) -> error::Result<Vec<TierEntry>> {
    engine.provider.tier_list(&role).await.map_err(|e| {
        log(
            &app,
            "warn",
            format!("get_tier_list failed role={role:?}: {e}"),
        );
        e.into()
    })
}

/// Champions that counter `champion_id` in `role`, best counters first.
#[tauri::command]
pub async fn get_counters(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    champion_id: i64,
    role: String,
) -> error::Result<Vec<CounterEntry>> {
    engine
        .provider
        .counters(champion_id, &role)
        .await
        .map_err(|e| {
            log(
                &app,
                "warn",
                format!("get_counters failed champion_id={champion_id} role={role:?}: {e}"),
            );
            e.into()
        })
}

/// Detailed rune page (incl. shards + spells). `enemy_champion_id` asks for a
/// matchup-specific page; thin matchups can still return "not-enough-data".
#[tauri::command]
pub async fn get_rune_build(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    champion_id: i64,
    role: String,
    enemy_champion_id: Option<i64>,
) -> error::Result<RuneBuild> {
    engine
        .provider
        .rune_build(champion_id, role_opt(&role), enemy_champion_id)
        .await
        .map_err(|e| {
            log(
                &app,
                "warn",
                format!(
                    "get_rune_build failed champion_id={champion_id} role={role:?} enemy_champion_id={enemy_champion_id:?}: {e}"
                ),
            );
            e.into()
        })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildDetails {
    items: Vec<ItemRecommendation>,
    skill_order: Option<SkillOrder>,
}

#[tauri::command]
pub async fn get_build_details(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    champion_id: i64,
    role: String,
    enemy_champion_id: Option<i64>,
) -> error::Result<BuildDetails> {
    let (self_champion, self_raw_name) = engine
        .provider
        .champion_names(champion_id)
        .await
        .ok_or_else(|| error::Error::Other(format!("unknown champion id: {champion_id}")))?;
    let enemies = if let Some(enemy_id) = enemy_champion_id {
        engine
            .provider
            .champion_names(enemy_id)
            .await
            .map(|(name, raw_name)| {
                vec![EnemyChampion {
                    name,
                    raw_name,
                    position: String::new(),
                    items: vec![],
                }]
            })
            .unwrap_or_default()
    } else {
        vec![]
    };
    let snapshot = GameSnapshot {
        game_mode: "CLASSIC".into(),
        game_time: 0.0,
        self_champion,
        self_raw_name,
        self_position: role,
        enemies,
        allies: vec![],
        players: vec![],
    };
    let items = engine.provider.items(&snapshot).await.map_err(|e| {
        log(&app, "warn", format!("get_build_details items failed: {e}"));
        error::Error::from(e)
    })?;
    let skill_order = engine.provider.skill_order(&snapshot).await.ok();
    Ok(BuildDetails { items, skill_order })
}

/// Manually import the currently displayed build: write the rune page and
/// (optionally) the summoner spells through the LCU. In mock mode both LCU
/// writes are skipped so the import button is testable without a client.
#[tauri::command]
pub async fn import_build(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    champion_id: i64,
    role: String,
    enemy_champion_id: Option<i64>,
    include_spells: bool,
    flip_spells: bool,
) -> error::Result<()> {
    let build = engine
        .provider
        .rune_build(champion_id, role_opt(&role), enemy_champion_id)
        .await?;

    if engine.mock.load(Ordering::Relaxed) {
        log(&app, "info", "mock import ok");
    } else {
        // Flatten to the LCU page shape: [keystone, p1..p3, s1, s2, shards×3].
        let mut perks = build.primary_perk_ids.clone();
        perks.extend_from_slice(&build.sub_perk_ids);
        perks.extend_from_slice(&build.shard_ids);
        lcu::apply_runes(&RunePagePayload {
            name: build.page_name.clone(),
            primary_style_id: build.primary_style_id,
            sub_style_id: build.sub_style_id,
            selected_perk_ids: perks,
            current: true,
        })
        .await?;

        if include_spells && build.spell_ids.len() == 2 {
            let (s1, s2) = if flip_spells {
                (build.spell_ids[1], build.spell_ids[0])
            } else {
                (build.spell_ids[0], build.spell_ids[1])
            };
            lcu::apply_spells(s1, s2).await?;
        }
    }

    let _ = app.emit(
        "rune-imported",
        RuneImportedEvent {
            champion_id,
            page_name: build.page_name,
        },
    );
    Ok(())
}

/// Toggle developer mode (persisted). Turning it off also stops any running
/// mock scenario so the overlay can't get stuck in synthetic state.
#[tauri::command]
pub fn set_developer_mode(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    enabled: bool,
) -> error::Result<()> {
    {
        engine.settings.lock().developer_mode = enabled;
    }
    engine.persist()?;
    if !enabled && engine.mock_stage() != MockStage::Off {
        crate::mock::apply_stage(&app, engine.inner(), MockStage::Off);
    }
    Ok(())
}

#[tauri::command]
pub fn get_mock_stage(engine: State<'_, Arc<Engine>>) -> String {
    engine.mock_stage().as_str().to_string()
}

/// Jump the mock scenario directly to `stage` (debug panel). Requires
/// developer mode so a stray invoke can't fake game state.
#[tauri::command]
pub fn set_mock_stage(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    stage: String,
) -> error::Result<()> {
    if !engine.settings.lock().developer_mode {
        return Err(error::Error::Other(
            "mock mode requires developer mode".into(),
        ));
    }
    let parsed = MockStage::parse(&stage)
        .ok_or_else(|| error::Error::Other(format!("unknown mock stage: {stage}")))?;
    crate::mock::apply_stage(&app, engine.inner(), parsed);
    Ok(())
}

#[tauri::command]
pub fn get_data_source(engine: State<'_, Arc<Engine>>) -> String {
    engine.provider.active().as_str().to_string()
}

#[tauri::command]
pub fn list_data_sources(engine: State<'_, Arc<Engine>>) -> Vec<String> {
    engine
        .provider
        .available()
        .into_iter()
        .map(|k| k.as_str().to_string())
        .collect()
}

#[tauri::command]
pub fn set_data_source(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    kind: String,
) -> error::Result<()> {
    let parsed = ProviderKind::parse(&kind)
        .ok_or_else(|| error::Error::Other(format!("unknown data source: {kind}")))?;
    engine.provider.set_active(parsed)?;
    {
        engine.settings.lock().build_data_source = parsed;
    }
    engine.persist()?;
    let _ = app.emit("data-source", kind);
    Ok(())
}

#[tauri::command]
pub fn get_player_stats_source(engine: State<'_, Arc<Engine>>) -> String {
    engine.player_provider.active().as_str().into()
}

#[tauri::command]
pub fn list_player_stats_sources(
    engine: State<'_, Arc<Engine>>,
) -> Vec<overlay_provider::ProviderDescriptor> {
    engine.player_provider.available()
}

#[tauri::command]
pub fn set_player_stats_source(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    source: String,
) -> error::Result<()> {
    let parsed = ProviderKind::parse(&source)
        .ok_or_else(|| error::Error::Other(format!("unknown player stats source: {source}")))?;
    engine.player_provider.set_active(parsed)?;
    engine.settings.lock().player_stats_source = parsed;
    engine.persist()?;
    let _ = app.emit("player-stats-source", source);
    Ok(())
}

#[tauri::command]
pub async fn get_player_profile(
    engine: State<'_, Arc<Engine>>,
    player: PlayerRef,
    force_refresh: bool,
) -> error::Result<PlayerProfile> {
    engine
        .player_provider
        .profile(&player, force_refresh)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_player_matches(
    engine: State<'_, Arc<Engine>>,
    player: PlayerRef,
    cursor: Option<String>,
    queue: Option<i64>,
    force_refresh: bool,
) -> error::Result<MatchPage> {
    engine
        .player_provider
        .recent_matches(&player, cursor.as_deref(), queue, force_refresh)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn get_player_champion_stats(
    engine: State<'_, Arc<Engine>>,
    player: PlayerRef,
    season: Option<String>,
    queue: Option<String>,
    role: Option<String>,
    force_refresh: bool,
) -> error::Result<Vec<PlayerChampionStats>> {
    engine
        .player_provider
        .champion_stats(
            &player,
            season.as_deref(),
            queue.as_deref(),
            role.as_deref(),
            force_refresh,
        )
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub async fn refresh_player_data(
    engine: State<'_, Arc<Engine>>,
    player: PlayerRef,
) -> error::Result<overlay_types::RefreshResult> {
    engine
        .player_provider
        .refresh(&player)
        .await
        .map_err(Into::into)
}

#[tauri::command]
pub fn get_mobile_pairing(engine: State<'_, Arc<Engine>>) -> MobilePairingState {
    engine.mobile.state()
}

#[tauri::command]
pub async fn start_mobile_pairing(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    relay_url: String,
) -> error::Result<MobilePairingState> {
    engine.mobile.start(&app, &relay_url).await
}

#[tauri::command]
pub async fn stop_mobile_pairing(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
) -> error::Result<MobilePairingState> {
    Ok(engine.mobile.stop(&app).await)
}
