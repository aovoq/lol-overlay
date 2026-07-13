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

fn apply_player_stats_source(engine: &Engine, source: &str) -> error::PlayerResult<()> {
    let parsed = ProviderKind::parse(source).ok_or_else(|| {
        overlay_provider::ProviderError::InvalidPlayerRequest(format!(
            "unknown player stats source: {source}"
        ))
    })?;
    if !engine
        .player_provider
        .available()
        .iter()
        .any(|descriptor| descriptor.id == source)
    {
        return Err(
            overlay_provider::ProviderError::InvalidPlayerRequest(format!(
                "player stats source is not registered: {source}"
            ))
            .into(),
        );
    }
    engine.player_provider.set_active(parsed)?;
    engine.settings.lock().player_stats_source = parsed;
    engine.persist()?;
    Ok(())
}

#[tauri::command]
pub fn set_player_stats_source(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    source: String,
) -> error::PlayerResult<()> {
    apply_player_stats_source(&engine, &source)?;
    let _ = app.emit("player-stats-source", source);
    Ok(())
}

#[tauri::command]
pub async fn get_player_profile(
    engine: State<'_, Arc<Engine>>,
    player: PlayerRef,
    force_refresh: bool,
) -> error::PlayerResult<PlayerProfile> {
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
) -> error::PlayerResult<MatchPage> {
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
) -> error::PlayerResult<Vec<PlayerChampionStats>> {
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
) -> error::PlayerResult<overlay_types::RefreshResult> {
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

#[cfg(test)]
mod player_command_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU64};

    use async_trait::async_trait;
    use overlay_provider::{
        BuildProvider, BuildProviderProxy, HardcodedProvider, PlayerStatsProxy,
        ProviderCapabilities, ProviderError,
    };
    use overlay_types::{
        MatchParticipant, PlayerIdentity, PlayerMatch, ProviderExtras, RankedEntry,
        RefreshAvailability, RefreshResult,
    };
    use parking_lot::Mutex;
    use tauri::Manager;

    struct CommandStub(&'static str);

    fn player() -> PlayerRef {
        PlayerRef {
            platform_id: "KR".into(),
            game_name: "Command Player".into(),
            tag_line: "KR1".into(),
        }
    }

    fn extras(source: &str) -> ProviderExtras {
        match source {
            "deeplol" => ProviderExtras::Deeplol(serde_json::json!({"fixture": true})),
            "opgg" => ProviderExtras::Opgg(serde_json::json!({"fixture": true})),
            _ => ProviderExtras::None,
        }
    }

    #[async_trait]
    impl PlayerStatsProvider for CommandStub {
        async fn profile(
            &self,
            player: &PlayerRef,
            _force: bool,
        ) -> overlay_provider::Result<PlayerProfile> {
            match player.game_name.as_str() {
                "Missing" => return Err(ProviderError::PlayerNotFound),
                "Invalid" => return Err(ProviderError::InvalidPlayerRequest("bad Riot ID".into())),
                "Limited" => {
                    return Err(ProviderError::RateLimited {
                        retry_after: Some(9),
                    })
                }
                "Timeout" => return Err(ProviderError::Timeout),
                "Malformed" => return Err(ProviderError::InvalidData("bad payload".into())),
                _ => {}
            }
            Ok(PlayerProfile {
                source: self.0.into(),
                identity: PlayerIdentity {
                    platform_id: player.platform_id.clone(),
                    game_name: player.game_name.clone(),
                    tag_line: player.tag_line.clone(),
                    puuid: Some("command-puuid".into()),
                },
                level: Some(100),
                profile_icon_id: Some(6),
                ranks: vec![RankedEntry {
                    queue: "RANKED_SOLO_5x5".into(),
                    tier: Some("GOLD".into()),
                    division: Some("I".into()),
                    lp: Some(50),
                    wins: Some(6),
                    losses: Some(4),
                }],
                previous_seasons: vec![],
                ladder_rank: Some(10),
                ladder_percentile: Some(1.0),
                fetched_at: 1,
                refresh: RefreshAvailability {
                    app_refresh: true,
                    ..RefreshAvailability::default()
                },
                extras: extras(self.0),
            })
        }

        async fn recent_matches(
            &self,
            _player: &PlayerRef,
            cursor: Option<&str>,
            _queue: Option<i64>,
            _force: bool,
        ) -> overlay_provider::Result<MatchPage> {
            Ok(MatchPage {
                source: self.0.into(),
                matches: vec![PlayerMatch {
                    match_id: format!("{}_{}", self.0, cursor.unwrap_or("0")),
                    started_at: 1,
                    duration_seconds: 1800,
                    queue_id: 420,
                    remake: false,
                    champion_id: 103,
                    role: Some("Middle".into()),
                    win: true,
                    kills: 5,
                    deaths: 2,
                    assists: 7,
                    cs: Some(200),
                    items: vec![],
                    spell_ids: vec![],
                    perk_ids: vec![],
                    participants: Vec::<MatchParticipant>::new(),
                    extras: extras(self.0),
                }],
                next_cursor: None,
                partial_failures: vec![],
                fetched_at: 1,
            })
        }

        async fn champion_stats(
            &self,
            _player: &PlayerRef,
            _season: Option<&str>,
            queue: Option<&str>,
            role: Option<&str>,
            _force: bool,
        ) -> overlay_provider::Result<Vec<PlayerChampionStats>> {
            Ok(vec![PlayerChampionStats {
                source: self.0.into(),
                champion_id: 103,
                games: 10,
                wins: 6,
                losses: 4,
                win_rate: 0.6,
                kda: None,
                cs_per_minute: None,
                role: role.map(str::to_owned),
                queue: queue.unwrap_or("RANKED").into(),
                extras: extras(self.0),
            }])
        }

        async fn refresh(&self, _player: &PlayerRef) -> overlay_provider::Result<RefreshResult> {
            Ok(RefreshResult {
                source: self.0.into(),
                cache_invalidated: true,
                mutation_performed: false,
                refreshed_at: 1,
            })
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                player_profile: true,
                match_history: true,
                champion_stats: true,
                regions: vec!["KR".into()],
                ..ProviderCapabilities::default()
            }
        }
    }

    fn engine() -> Arc<Engine> {
        let build = BuildProviderProxy::new(
            ProviderKind::Deeplol,
            [(
                ProviderKind::Deeplol,
                Arc::new(HardcodedProvider) as Arc<dyn BuildProvider>,
            )],
        )
        .unwrap();
        let player_provider = PlayerStatsProxy::new(
            ProviderKind::Deeplol,
            [
                (
                    ProviderKind::Deeplol,
                    Arc::new(CommandStub("deeplol")) as Arc<dyn PlayerStatsProvider>,
                ),
                (
                    ProviderKind::Opgg,
                    Arc::new(CommandStub("opgg")) as Arc<dyn PlayerStatsProvider>,
                ),
            ],
        )
        .unwrap();
        Arc::new(Engine {
            provider: Arc::new(build),
            player_provider: Arc::new(player_provider),
            live: overlay_live_client::LiveClient::new().unwrap(),
            settings: Mutex::new(Settings::default()),
            ui_layout: Mutex::new(UiLayout::default()),
            store_path: Mutex::new(None),
            mock: AtomicBool::new(false),
            mock_stage: Mutex::new(MockStage::Off),
            mock_generation: AtomicU64::new(0),
            last_champ_select: Mutex::new(None),
            last_phase: Mutex::new(None),
            current_summoner: Mutex::new(None),
            current_platform_id: Mutex::new(None),
            hit_regions: Mutex::new(vec![]),
            drag_active: AtomicBool::new(false),
            forced_interactive: AtomicBool::new(false),
            interactive_applied: AtomicBool::new(false),
            window_champselect: AtomicBool::new(false),
            window_ingame: AtomicBool::new(false),
            phase_champselect: AtomicBool::new(false),
            phase_in_game: AtomicBool::new(false),
            mobile: crate::mobile::MobileRelay::new().unwrap(),
        })
    }

    #[tokio::test]
    async fn direct_player_commands_forward_and_serialize_camel_case_contracts() {
        let engine = engine();
        let app = tauri::test::mock_builder()
            .manage(engine)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();

        assert_eq!(get_player_stats_source(app.state()), "deeplol");
        let descriptors = list_player_stats_sources(app.state());
        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>(),
            vec!["deeplol", "opgg"]
        );
        assert!(!descriptors.iter().any(|descriptor| descriptor.id == "ugg"));

        let profile = get_player_profile(app.state(), player(), true)
            .await
            .expect("profile command");
        let profile_json = serde_json::to_value(profile).unwrap();
        assert_eq!(profile_json["profileIconId"], 6);
        assert_eq!(profile_json["fetchedAt"], 1);
        assert!(profile_json.get("profile_icon_id").is_none());

        let matches =
            get_player_matches(app.state(), player(), Some("20".into()), Some(420), false)
                .await
                .expect("matches command");
        assert_eq!(matches.matches[0].match_id, "deeplol_20");

        let champions = get_player_champion_stats(
            app.state(),
            player(),
            Some("current".into()),
            Some("RANKED".into()),
            Some("Middle".into()),
            false,
        )
        .await
        .expect("champion command");
        assert_eq!(champions[0].role.as_deref(), Some("Middle"));

        let refresh = refresh_player_data(app.state(), player())
            .await
            .expect("refresh command");
        assert!(refresh.cache_invalidated);
        assert!(!refresh.mutation_performed);
    }

    #[tokio::test]
    async fn direct_profile_command_preserves_typed_error_matrix() {
        let engine = engine();
        let app = tauri::test::mock_builder()
            .manage(engine)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        for (game_name, kind, retry_after) in [
            ("Missing", "notFound", None),
            ("Invalid", "validation", None),
            ("Limited", "rateLimited", Some(9)),
            ("Timeout", "timeout", None),
            ("Malformed", "invalidData", None),
        ] {
            let error = get_player_profile(
                app.state(),
                PlayerRef {
                    game_name: game_name.into(),
                    ..player()
                },
                false,
            )
            .await
            .expect_err("typed command failure");
            let value = serde_json::to_value(error).unwrap();
            assert_eq!(value["kind"], kind);
            assert_eq!(
                value["retryAfter"],
                serde_json::to_value(retry_after).unwrap()
            );
            assert!(value["message"]
                .as_str()
                .is_some_and(|message| !message.is_empty()));
        }
    }

    #[test]
    fn player_source_command_persists_independently_and_rejects_ugg() {
        let engine = engine();
        let path = std::env::temp_dir().join(format!(
            "lol-overlay-player-command-{}.json",
            std::process::id()
        ));
        *engine.store_path.lock() = Some(path.clone());
        apply_player_stats_source(&engine, "opgg").expect("persist OP.GG source");
        assert_eq!(engine.settings().player_stats_source, ProviderKind::Opgg);
        assert_eq!(engine.settings().build_data_source, ProviderKind::Deeplol);
        let persisted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted["settings"]["playerStatsSource"], "opgg");
        assert_eq!(persisted["settings"]["buildDataSource"], "deeplol");

        let error = apply_player_stats_source(&engine, "ugg").expect_err("U.GG is build-only");
        assert_eq!(serde_json::to_value(error).unwrap()["kind"], "validation");
        assert_eq!(engine.settings().player_stats_source, ProviderKind::Opgg);
        let _ = std::fs::remove_file(path);
    }
}
