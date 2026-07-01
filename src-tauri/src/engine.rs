//! Core engine: shared application state plus the two background tasks.
//!
//!   * [`rune_processor`] drains champ-select sessions and imports runes on
//!     pick change (sessions arrive from the WebSocket and the poller fallback).
//!   * [`poller`] tracks phase / in-game state for the UI and feeds the
//!     rune-import channel as a REST fallback.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Monitor, PhysicalPosition,
    PhysicalSize,
};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::events::{
    log, ChampSelectEvent, LpChangeEvent, PhaseEvent, RecommendationsEvent, RuneImportedEvent,
};
use crate::hittest::HitRegion;
use overlay_lcu::{self as lcu, Phase, RunePagePayload};
use overlay_live_client::LiveClient;
use overlay_provider::{classify_threats, BuildProvider, ProviderKind, ProviderProxy};

/// How often the poller checks phase / in-game state.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// User-tunable settings. Serialized camelCase because the frontend mirrors
/// this shape directly, and persisted in the app config store.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(default = "default_true")]
    pub auto_import_runes: bool,
    /// Write summoner spells along with runes on manual import.
    #[serde(default = "default_true")]
    pub import_spells: bool,
    /// Swap the two spells (D/F order) on import.
    #[serde(default)]
    pub spells_flipped: bool,
    /// Legacy setting kept for persisted-settings compatibility.
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub data_source: ProviderKind,
    #[serde(default)]
    pub presentation_mode: PresentationMode,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PresentationMode {
    #[default]
    Overlay,
    Window,
}

impl PresentationMode {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "overlay" => Some(Self::Overlay),
            "window" => Some(Self::Window),
            _ => None,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_import_runes: true,
            import_spells: true,
            spells_flipped: false,
            pinned: false,
            data_source: ProviderKind::default(),
            presentation_mode: PresentationMode::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowMode {
    Overlay,
    ChampSelect,
    InGame,
}

impl WindowMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Overlay => "overlay",
            Self::ChampSelect => "champselect",
            Self::InGame => "ingame",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PanelPosition {
    pub left: f64,
    pub top: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowPosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLayout {
    #[serde(default)]
    pub ingame_panel: Option<PanelPosition>,
    #[serde(default)]
    pub champselect_window: Option<WindowPosition>,
    /// In-game panel collapsed to its header chip.
    #[serde(default)]
    pub ingame_collapsed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredState {
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    ui_layout: UiLayout,
}

/// Which synthetic scenario the debug hotkey is driving
/// (Ctrl+Shift+D cycles off → champ select → in game → off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MockStage {
    Off,
    ChampSelect,
    InGame,
}

/// Shared application state, held in Tauri's managed state.
pub struct Engine {
    pub provider: Arc<ProviderProxy>,
    pub live: LiveClient,
    pub settings: Mutex<Settings>,
    pub ui_layout: Mutex<UiLayout>,
    pub store_path: Mutex<Option<PathBuf>>,
    /// Debug/mock mode: when on, the poller pauses and the UI is driven by
    /// synthetic state (cycle with Ctrl+Shift+D). Lets you work on the
    /// overlay without launching League.
    pub mock: AtomicBool,
    /// Which mock scenario is active; `mock` stays in sync (true iff != Off).
    pub mock_stage: Mutex<MockStage>,
    /// Last `champ-select` event emitted, so duplicate sessions (WS + REST
    /// fallback) don't spam the frontend with identical state.
    pub last_champ_select: Mutex<Option<ChampSelectEvent>>,
    /// Clickable rects reported by the frontend (`data-hit` elements), in
    /// window-relative logical px. Read by `hittest::cursor_watcher`.
    pub hit_regions: Mutex<Vec<HitRegion>>,
    /// A panel drag is in progress: hold the window interactive even when the
    /// cursor outruns the (briefly stale) reported rects.
    pub drag_active: AtomicBool,
    /// Command-forced override: whole overlay window interactive.
    pub forced_interactive: AtomicBool,
    /// Interactivity last applied to the window, so the watcher only touches
    /// the window style on transitions.
    pub interactive_applied: AtomicBool,
    /// Control-window mode last applied by `apply_window_mode`.
    pub window_champselect: AtomicBool,
    /// In-game UI currently presented in the normal control window.
    pub window_ingame: AtomicBool,
    /// Current gameflow summary for settings changes that need immediate
    /// re-layout between poll ticks.
    pub phase_champselect: AtomicBool,
    pub phase_in_game: AtomicBool,
}

impl Engine {
    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    pub fn ui_layout(&self) -> UiLayout {
        self.ui_layout.lock().unwrap().clone()
    }

    pub fn init_store(&self, path: PathBuf) -> crate::error::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        if path.exists() {
            let bytes = fs::read(&path)?;
            let stored = serde_json::from_slice::<StoredState>(&bytes)?;
            let data_source = stored.settings.data_source;
            *self.settings.lock().unwrap() = stored.settings;
            *self.ui_layout.lock().unwrap() = stored.ui_layout;
            let _ = self.provider.set_active(data_source);
        }

        *self.store_path.lock().unwrap() = Some(path);
        self.persist()
    }

    pub fn persist(&self) -> crate::error::Result<()> {
        let path = self.store_path.lock().unwrap().clone();
        let Some(path) = path else {
            return Ok(());
        };

        write_store(
            &path,
            &StoredState {
                settings: self.settings(),
                ui_layout: self.ui_layout(),
            },
        )
    }

    pub fn mock_stage(&self) -> MockStage {
        *self.mock_stage.lock().unwrap()
    }

    /// Advance/clear the mock scenario, keeping the plain `mock` flag in sync
    /// (the poller pause and `import_build` only care about on/off).
    pub fn set_mock_stage(&self, stage: MockStage) {
        *self.mock_stage.lock().unwrap() = stage;
        self.mock.store(stage != MockStage::Off, Ordering::SeqCst);
    }
}

fn write_store(path: &Path, stored: &StoredState) -> crate::error::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(stored)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Emit `ev` on `champ-select` unless it equals the last emitted state — the
/// WebSocket and the poller's REST fallback feed the same sessions twice.
pub fn emit_champ_select(app: &AppHandle, engine: &Engine, ev: ChampSelectEvent) {
    let mut last = engine.last_champ_select.lock().unwrap();
    if last.as_ref() == Some(&ev) {
        return;
    }
    let _ = app.emit("champ-select", ev.clone());
    *last = Some(ev);
}

/// Control window sizes. Compact is the startup/status view; pick is the
/// normal-window version of the old OPENLOL champ-select panel.
const CONTROL_COMPACT_SIZE: (f64, f64) = (520.0, 220.0);
const CONTROL_PICK_SIZE: (f64, f64) = (1040.0, 860.0);
const CONTROL_INGAME_SIZE: (f64, f64) = (540.0, 820.0);
const CONTROL_MARGIN: f64 = 16.0;
const CONTROL_PICK_X: f64 = 48.0;

/// The screen region the overlay window may occupy on `monitor`.
///
/// On Windows the borderless game covers the whole monitor, so the overlay
/// must too. On macOS a regular window can't overlap the menu bar: the OS
/// pushes a monitor-sized window down below it while keeping the requested
/// size, sliding the bottom edge (and the bottom-anchored status chip) off
/// screen — so use the work area (screen minus menu bar / Dock) there.
pub fn overlay_bounds(monitor: &Monitor) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    if cfg!(target_os = "macos") {
        let area = monitor.work_area();
        (area.position, area.size)
    } else {
        (*monitor.position(), *monitor.size())
    }
}

/// Keep the transparent overlay window covering its current monitor.
pub fn apply_overlay_bounds(app: &AppHandle) {
    let Some(win) = app.get_webview_window("main") else {
        return;
    };
    let _ = win.show();
    let monitor = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let (pos, size) = overlay_bounds(&monitor);
    let _ = win.set_position(pos);
    let _ = win.set_size(size);
    let _ = win.set_always_on_top(true);
    let _ = win.set_ignore_cursor_events(true);
}

fn control_position(
    monitor: &Monitor,
    mode: WindowMode,
) -> (LogicalPosition<f64>, LogicalSize<f64>) {
    let scale = monitor.scale_factor();
    let area = monitor.work_area();
    let origin = area.position.to_logical::<f64>(scale);
    let bounds = area.size.to_logical::<f64>(scale);
    let (w, h) = match mode {
        WindowMode::Overlay => CONTROL_COMPACT_SIZE,
        WindowMode::ChampSelect => CONTROL_PICK_SIZE,
        WindowMode::InGame => CONTROL_INGAME_SIZE,
    };
    let max_x = origin.x + (bounds.width - w).max(0.0);
    let max_y = origin.y + (bounds.height - h).max(0.0);
    let x = match mode {
        WindowMode::ChampSelect => origin.x + CONTROL_PICK_X,
        WindowMode::Overlay | WindowMode::InGame => origin.x + CONTROL_MARGIN,
    }
    .clamp(origin.x, max_x);
    let y = match mode {
        WindowMode::ChampSelect => origin.y + ((bounds.height - h) / 2.0).max(0.0),
        WindowMode::Overlay => origin.y + (bounds.height - h - CONTROL_MARGIN).max(0.0),
        WindowMode::InGame => origin.y + CONTROL_MARGIN,
    }
    .clamp(origin.y, max_y);
    (LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

/// Place the normal control window in either compact status mode or expanded
/// pick mode. Automatic layout does not force focus, so game input is not
/// stolen during phase transitions.
pub fn apply_control_layout(app: &AppHandle, mode: WindowMode) {
    let Some(win) = app.get_webview_window("control") else {
        return;
    };
    let monitor = win
        .current_monitor()
        .ok()
        .flatten()
        .or_else(|| win.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let (pos, size) = control_position(&monitor, mode);
    let _ = win.set_size(size);
    let _ = win.set_position(pos);
    let _ = win.show();
}

/// Show the normal control window on demand. Unlike automatic phase changes,
/// the explicit hotkey brings it to the front and focuses it.
pub fn show_control_window(app: &AppHandle) {
    let mode = app
        .try_state::<Arc<Engine>>()
        .map(|engine| current_window_mode(&engine))
        .unwrap_or(WindowMode::Overlay);
    apply_control_layout(app, mode);
    if let Some(win) = app.get_webview_window("control") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

pub fn current_window_mode(engine: &Engine) -> WindowMode {
    if engine.window_champselect.load(Ordering::SeqCst) {
        WindowMode::ChampSelect
    } else if engine.window_ingame.load(Ordering::SeqCst) {
        WindowMode::InGame
    } else {
        WindowMode::Overlay
    }
}

fn desired_window_mode(settings: &Settings, champselect: bool, in_game: bool) -> WindowMode {
    if champselect {
        WindowMode::ChampSelect
    } else if in_game && settings.presentation_mode == PresentationMode::Window {
        WindowMode::InGame
    } else {
        WindowMode::Overlay
    }
}

/// Recompute presentation from phase + settings and update windows only when
/// the effective mode changes.
pub fn apply_desired_window_mode(app: &AppHandle, engine: &Engine) {
    let mode = desired_window_mode(
        &engine.settings(),
        engine.phase_champselect.load(Ordering::SeqCst),
        engine.phase_in_game.load(Ordering::SeqCst),
    );
    if current_window_mode(engine) != mode {
        apply_window_mode(app, mode);
    }
}

/// Switch app presentation between compact control, pick control, and in-game
/// normal-window modes.
pub fn apply_window_mode(app: &AppHandle, mode: WindowMode) {
    if mode == WindowMode::InGame {
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.set_ignore_cursor_events(true);
            let _ = win.hide();
        }
    } else {
        apply_overlay_bounds(app);
    }
    apply_control_layout(app, mode);

    // Reset to click-through and clear the emergency override, keeping the
    // watcher's applied-state cache in sync with the actual window style.
    if let Some(engine) = app.try_state::<Arc<Engine>>() {
        engine.forced_interactive.store(false, Ordering::SeqCst);
        engine.interactive_applied.store(false, Ordering::SeqCst);
        engine
            .window_champselect
            .store(mode == WindowMode::ChampSelect, Ordering::SeqCst);
        engine
            .window_ingame
            .store(mode == WindowMode::InGame, Ordering::SeqCst);
    }
    let _ = app.emit("interactive", false);

    let _ = app.emit("window-mode", mode.as_str());
}

/// Drains champ-select sessions and imports runes whenever our pick changes.
/// Dedup by champion id makes the duplicate WS + REST sessions harmless.
pub async fn rune_processor(app: AppHandle, engine: Arc<Engine>, mut rx: UnboundedReceiver<Value>) {
    let mut last_imported: i64 = 0;

    while let Some(session) = rx.recv().await {
        // OPENLOL panel state: every session parses into a ChampSelectEvent,
        // emitted only on change (the poller emits the `active: false` end).
        if let Some(ev) = lcu::parse_champ_select(&session) {
            emit_champ_select(&app, &engine, ev);
        }

        if !engine.settings().auto_import_runes {
            continue;
        }
        let Some(pick) = lcu::parse_my_pick(&session) else {
            last_imported = 0; // pick cleared; allow re-import of the same champ
            continue;
        };
        if pick.champion_id == last_imported {
            continue;
        }

        let rec = match engine
            .provider
            .runes(pick.champion_id, pick.position.as_deref())
            .await
        {
            Ok(rec) => rec,
            Err(e) => {
                log(&app, "error", format!("rune lookup failed: {e}"));
                continue;
            }
        };

        let payload = RunePagePayload {
            name: rec.name.clone(),
            primary_style_id: rec.primary_style_id,
            sub_style_id: rec.sub_style_id,
            selected_perk_ids: rec.selected_perk_ids,
            current: true,
        };
        if let Err(e) = lcu::apply_runes(&payload).await {
            log(&app, "error", format!("rune import failed: {e}"));
            continue;
        }

        last_imported = pick.champion_id;
        log(&app, "info", format!("Imported runes: {}", rec.name));
        let _ = app.emit(
            "rune-imported",
            RuneImportedEvent {
                champion_id: pick.champion_id,
                page_name: rec.name,
            },
        );
    }
}

/// Ladder position as a single comparable number, for promote/demote
/// detection. Apex tiers have no real division ("NA" → 0).
fn rank_value(tier: &str, division: &str) -> i32 {
    const TIERS: [&str; 10] = [
        "IRON",
        "BRONZE",
        "SILVER",
        "GOLD",
        "PLATINUM",
        "EMERALD",
        "DIAMOND",
        "MASTER",
        "GRANDMASTER",
        "CHALLENGER",
    ];
    let t = TIERS
        .iter()
        .position(|&t| t == tier)
        .map_or(-1, |i| i as i32);
    let d = match division {
        "III" => 1,
        "II" => 2,
        "I" => 3,
        _ => 0,
    };
    t * 4 + d
}

/// How many recent games the profile chip shows.
const RECENT_GAMES: usize = 10;
/// Refresh the match history at least every N polls (~1 min) even without a
/// detected ranked result, so normal games / ARAMs show up too.
const HISTORY_REFRESH_POLLS: u32 = 30;

/// Tracks phase + in-game state for the UI. During champ select it also pushes
/// the current session to `tx` as a fallback in case the WebSocket missed it.
pub async fn poller(app: AppHandle, engine: Arc<Engine>, tx: UnboundedSender<Value>) {
    let mut prev_phase = Phase::None;
    let mut prev_summoner: Option<lcu::SummonerInfo> = None;
    let mut recent_games: Option<Vec<lcu::RecentGame>> = None;
    let mut history_poll_age = HISTORY_REFRESH_POLLS; // refresh on first poll
    let mut platform_resolved = false;
    loop {
        // In mock mode the UI is driven by synthetic state; don't fight it.
        if engine.mock.load(Ordering::Relaxed) {
            tokio::time::sleep(POLL_INTERVAL).await;
            continue;
        }

        let phase = lcu::fetch_phase().await;
        let client_up = phase.is_ok();
        let phase = phase.unwrap_or(Phase::None);

        engine
            .phase_champselect
            .store(phase == Phase::ChampSelect, Ordering::SeqCst);

        // Leaving champ select closes the OPENLOL panel — the WebSocket has no
        // "session gone" signal we consume, so the poller owns the inactive
        // sentinel.
        if phase != Phase::ChampSelect && prev_phase == Phase::ChampSelect {
            emit_champ_select(&app, &engine, ChampSelectEvent::default());
        }
        let game_just_ended = prev_phase == Phase::InProgress && phase != Phase::InProgress;
        prev_phase = phase;

        // Logged-in summoner + solo rank/LP for the profile chip. Emitted on
        // every poll like `phase` — the frontend may register its listener
        // after the first poll, so a deduped one-shot emit can be lost.
        let mut ranked_result_landed = false;
        if client_up {
            match lcu::fetch_summoner().await {
                Ok(info) => {
                    // A ranked result landed when the solo W/L count grew.
                    // Compare LP around it for the post-game banner.
                    if let Some(prev) = &prev_summoner {
                        let games = |s: &lcu::SummonerInfo| s.solo_wins + s.solo_losses;
                        if !prev.solo_tier.is_empty() && games(&info) > games(prev) {
                            ranked_result_landed = true;
                            let old = rank_value(&prev.solo_tier, &prev.solo_division);
                            let new = rank_value(&info.solo_tier, &info.solo_division);
                            let _ = app.emit(
                                "lp-change",
                                LpChangeEvent {
                                    win: info.solo_wins > prev.solo_wins,
                                    lp_delta: info.solo_lp - prev.solo_lp,
                                    tier: info.solo_tier.clone(),
                                    division: info.solo_division.clone(),
                                    lp: info.solo_lp,
                                    rank_change: match new.cmp(&old) {
                                        std::cmp::Ordering::Greater => "promoted".into(),
                                        std::cmp::Ordering::Less => "demoted".into(),
                                        std::cmp::Ordering::Equal => String::new(),
                                    },
                                },
                            );
                        }
                    }
                    let _ = app.emit("summoner", info.clone());
                    prev_summoner = Some(info);
                }
                Err(e) => log(&app, "warn", format!("summoner fetch failed: {e}")),
            }
        } else {
            prev_summoner = None;
            recent_games = None;
            let _ = app.emit("summoner", Value::Null);
        }

        // Recent-games strip. The fetch is local but heavier than the rank
        // call, so refresh only when a game just ended (phase left InProgress
        // or a ranked result landed) or on the ~1 min fallback timer; the
        // cached list is re-emitted every poll (same listener-race reasoning).
        if client_up {
            history_poll_age += 1;
            if recent_games.is_none()
                || game_just_ended
                || ranked_result_landed
                || history_poll_age >= HISTORY_REFRESH_POLLS
            {
                match lcu::fetch_recent_matches(RECENT_GAMES).await {
                    Ok(games) => {
                        recent_games = Some(games);
                        history_poll_age = 0;
                    }
                    Err(e) => log(&app, "warn", format!("match history fetch failed: {e}")),
                }
            }
            if let Some(games) = &recent_games {
                let _ = app.emit("match-history", games);
            }
        }

        // Resolve the player's region into the provider once per client run.
        if client_up && !platform_resolved {
            match lcu::fetch_platform_id().await {
                Ok(platform_id) => {
                    log(&app, "info", format!("platform resolved: {platform_id}"));
                    engine.provider.set_platform_id(&platform_id);
                    platform_resolved = true;
                }
                Err(e) => log(&app, "warn", format!("region lookup failed: {e}")),
            }
        } else if !client_up {
            platform_resolved = false;
        }

        if phase == Phase::ChampSelect {
            if let Ok(Some(session)) = lcu::fetch_session().await {
                let _ = tx.send(session);
            }
        }

        // In-game item recommendations (Live Client Data API — polling only).
        let mut in_game = false;
        if let Some(snapshot) = engine.live.snapshot().await {
            in_game = true;
            let threats = classify_threats(&snapshot);
            let items = engine.provider.items(&snapshot).await.unwrap_or_default();
            let skill_order = engine.provider.skill_order(&snapshot).await.ok();

            let _ = app.emit(
                "recommendations",
                RecommendationsEvent {
                    self_champion: snapshot.self_champion.clone(),
                    self_raw_name: snapshot.self_raw_name.clone(),
                    self_position: snapshot.self_position.clone(),
                    enemies: snapshot.enemies.clone(),
                    threats,
                    skill_order,
                    items,
                },
            );
        }
        engine.phase_in_game.store(in_game, Ordering::SeqCst);

        let _ = app.emit(
            "phase",
            PhaseEvent {
                phase: phase.label().to_string(),
                client_up,
                in_game,
            },
        );
        apply_desired_window_mode(&app, &engine);

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
