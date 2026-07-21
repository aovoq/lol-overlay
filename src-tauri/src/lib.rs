//! lol-overlay — a lightweight, Tauri-based League of Legends overlay.
//!
//! App modules (`src-tauri/src/`):
//!   * `engine`      — shared state + the poller / rune-import tasks
//!   * `events`      — payloads emitted to the frontend
//!   * `hittest`     — region-based click-through (always-clickable headers)
//!   * `mobile`      — ephemeral Cloudflare relay pairing for the iPhone sideboard
//!   * `mock`        — debug mode driving synthetic state (Ctrl+Shift+D)
//!   * `hotkeys`     — global shortcuts
//!   * `commands`    — frontend-invokable commands
//!
//! Workspace crates: `overlay-lcu`, `overlay-live-client`, `overlay-provider`, …

mod commands;
mod engine;
mod error;
mod events;
mod hittest;
#[cfg(desktop)]
mod hotkeys;
mod mobile;
mod mock;

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use serde_json::Value;
use tauri::{AppHandle, Manager, WindowEvent};
use tokio::sync::mpsc::UnboundedSender;

use overlay_ddragon::DdragonClient;
use overlay_provider::{
    BuildProvider, BuildProviderProxy, PlayerStatsProvider, PlayerStatsProxy, ProviderKind,
};
use overlay_provider_deeplol::DeepLolProvider;
use overlay_provider_lolalytics::LolalyticsProvider;
use overlay_provider_lolps::LolpsProvider;
use overlay_provider_opgg::OpggProvider;
use overlay_provider_ugg::UggProvider;

use crate::engine::{Engine, MockStage, Settings, UiLayout, WindowMode};
use overlay_live_client::LiveClient;

fn create_player_stats_proxy(
    deeplol: Arc<DeepLolProvider>,
    opgg: Arc<OpggProvider>,
) -> overlay_provider::Result<PlayerStatsProxy> {
    // U.GG is intentionally build-only. Its player GraphQL endpoint (`POST /api`)
    // returns a Cloudflare challenge to anonymous direct clients, and the
    // server-rendered Apollo state does not contain match history. Do not add it
    // here until U.GG exposes a stable anonymous JSON contract covering the full
    // PlayerStatsProvider surface. See docs/ugg-chrome-api-investigation.md.
    PlayerStatsProxy::new(
        ProviderKind::Deeplol,
        [
            (
                ProviderKind::Deeplol,
                deeplol as Arc<dyn PlayerStatsProvider>,
            ),
            (ProviderKind::Opgg, opgg as Arc<dyn PlayerStatsProvider>),
        ],
    )
}

fn create_build_provider_proxy(
    ddragon: Arc<DdragonClient>,
    deeplol: Arc<DeepLolProvider>,
    opgg: Arc<OpggProvider>,
) -> overlay_provider::Result<BuildProviderProxy> {
    let providers: [(ProviderKind, Arc<dyn BuildProvider>); 5] = [
        (ProviderKind::Deeplol, deeplol),
        (
            ProviderKind::Ugg,
            Arc::new(UggProvider::new(ddragon.clone())?),
        ),
        (
            ProviderKind::Lolalytics,
            Arc::new(LolalyticsProvider::new(ddragon.clone())?),
        ),
        (ProviderKind::Lolps, Arc::new(LolpsProvider::new(ddragon)?)),
        (ProviderKind::Opgg, opgg),
    ];
    BuildProviderProxy::new(ProviderKind::Deeplol, providers)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let ddragon = Arc::new(DdragonClient::new());
    let deeplol =
        Arc::new(DeepLolProvider::new(ddragon.clone()).expect("failed to build DeepLoL provider"));
    let opgg =
        Arc::new(OpggProvider::new(ddragon.clone()).expect("failed to build op.gg provider"));
    let proxy = create_build_provider_proxy(ddragon, deeplol.clone(), opgg.clone())
        .expect("failed to build provider proxy");
    let provider = Arc::new(proxy);
    let player_provider = Arc::new(
        create_player_stats_proxy(deeplol, opgg).expect("failed to build player stats proxy"),
    );

    let engine = Arc::new(Engine {
        provider,
        player_provider,
        live: LiveClient::new().expect("failed to build Live Client http client"),
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
        current_matchmaking: Mutex::new(None),
        hit_regions: Mutex::new(Vec::new()),
        drag_active: AtomicBool::new(false),
        forced_interactive: AtomicBool::new(false),
        interactive_applied: AtomicBool::new(false),
        window_champselect: AtomicBool::new(false),
        window_ingame: AtomicBool::new(false),
        phase_champselect: AtomicBool::new(false),
        phase_in_game: AtomicBool::new(false),
        mobile: mobile::MobileRelay::new().expect("failed to build mobile relay client"),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(engine.clone())
        .setup(move |app| {
            engine.init_store(
                app.handle(),
                app.path().app_config_dir()?.join("settings.json"),
            )?;

            // Resume the mobile relay session a previous run left behind
            // (dev rebuilds kill the process without revoking; the paired
            // phone keeps listening to that session).
            engine
                .mobile
                .set_store_path(app.path().app_config_dir()?.join("mobile-session.json"));
            {
                let relay = engine.mobile.clone();
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move { relay.restore(&handle).await });
            }

            if let Some(win) = app.get_webview_window("main") {
                // Start click-through so the overlay never steals game clicks.
                let _ = win.set_ignore_cursor_events(true);
            }
            // Cover the whole monitor so panels anchor to the real screen edges
            // regardless of resolution / HiDPI scaling. The normal control
            // window starts centered unless the user has moved it before.
            engine::apply_overlay_bounds(app.handle());
            engine::apply_control_layout(app.handle(), WindowMode::Overlay);

            #[cfg(desktop)]
            hotkeys::setup(app, engine.clone())?;

            // Champ-select WebSocket → channel → rune processor (event-driven);
            // the poller tracks phase / in-game state and feeds the same channel.
            let handle = app.handle().clone();
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
            spawn_champ_select_subscription(handle.clone(), tx.clone());
            spawn_matchmaking_subscription(handle.clone(), engine.clone());
            spawn_mobile_command_poller(handle.clone(), engine.clone());

            tauri::async_runtime::spawn(engine::rune_processor(handle.clone(), engine.clone(), rx));
            tauri::async_runtime::spawn(engine::poller(handle.clone(), engine.clone(), tx));
            // Region-based click-through: headers stay clickable, the rest of
            // the overlay passes clicks to the game.
            tauri::async_runtime::spawn(hittest::cursor_watcher(handle, engine.clone()));
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "control" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::get_app_snapshot,
            commands::set_auto_import,
            commands::set_auto_open_champion,
            commands::set_auto_open_live,
            commands::set_interactive,
            commands::set_hit_regions,
            commands::set_drag_active,
            commands::set_ingame_collapsed,
            commands::set_import_spells,
            commands::set_spells_flipped,
            commands::set_presentation_mode,
            commands::get_ui_layout,
            commands::set_ingame_panel_position,
            commands::set_control_window_geometry,
            commands::get_tier_list,
            commands::get_counters,
            commands::get_rune_build,
            commands::get_build_details,
            commands::import_build,
            commands::set_developer_mode,
            commands::get_mock_stage,
            commands::set_mock_stage,
            commands::get_data_source,
            commands::get_current_player_ref,
            commands::get_player_stats_source,
            commands::list_player_stats_sources,
            commands::set_player_stats_source,
            commands::get_player_profile,
            commands::get_player_matches,
            commands::get_player_champion_stats,
            commands::refresh_player_data,
            commands::list_data_sources,
            commands::set_data_source,
            commands::get_mobile_pairing,
            commands::start_mobile_pairing,
            commands::stop_mobile_pairing,
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                // Graceful quit: revoke the relay session so the paired phone
                // is told the desktop went away instead of waiting forever.
                let relay = app_handle.state::<Arc<Engine>>().mobile.clone();
                let _ = tauri::async_runtime::block_on(tokio::time::timeout(
                    Duration::from_secs(3),
                    async move { relay.shutdown().await },
                ));
            }
        });
}

#[cfg(test)]
mod player_registration_tests {
    use super::*;

    #[test]
    fn production_player_registry_contains_only_deeplol_and_opgg() {
        let ddragon = Arc::new(DdragonClient::new());
        let deeplol = Arc::new(DeepLolProvider::new(ddragon.clone()).unwrap());
        let opgg = Arc::new(OpggProvider::new(ddragon).unwrap());
        let proxy = create_player_stats_proxy(deeplol, opgg).unwrap();
        let ids = proxy
            .available()
            .into_iter()
            .map(|descriptor| descriptor.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["deeplol", "opgg"]);
        assert!(proxy.set_active(ProviderKind::Ugg).is_err());
        assert!(proxy.set_active(ProviderKind::Lolalytics).is_err());
        assert!(proxy.set_active(ProviderKind::Lolps).is_err());
    }

    #[test]
    fn production_build_registry_contains_lolps() {
        let ddragon = Arc::new(DdragonClient::new());
        let deeplol = Arc::new(DeepLolProvider::new(ddragon.clone()).unwrap());
        let opgg = Arc::new(OpggProvider::new(ddragon.clone()).unwrap());
        let proxy = create_build_provider_proxy(ddragon, deeplol, opgg).unwrap();
        let ids = proxy
            .available()
            .into_iter()
            .map(|kind| kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["deeplol", "lolalytics", "lolps", "opgg", "ugg"]);
    }
}

const CHAMP_SELECT_WS_RETRY_DELAY: Duration = Duration::from_secs(3);
const READY_CHECK_COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(250);
const IDLE_COMMAND_STATE_CHECK_INTERVAL: Duration = Duration::from_millis(100);

fn spawn_champ_select_subscription(handle: AppHandle, tx: UnboundedSender<Value>) {
    tauri::async_runtime::spawn(async move {
        let mut logged_waiting_for_client = false;

        loop {
            match overlay_lcu::subscribe_champ_select(tx.clone()).await {
                Ok(subscription) => {
                    logged_waiting_for_client = false;
                    events::log(&handle, "info", "Subscribed to champ-select websocket");

                    while !subscription.is_finished() {
                        tokio::time::sleep(CHAMP_SELECT_WS_RETRY_DELAY).await;
                    }

                    events::log(
                        &handle,
                        "warn",
                        format!(
                            "Champ-select websocket stopped; retrying in {}s",
                            CHAMP_SELECT_WS_RETRY_DELAY.as_secs()
                        ),
                    );
                }
                Err(err) => {
                    if !logged_waiting_for_client {
                        events::log(
                            &handle,
                            "warn",
                            format!(
                                "Failed to subscribe to champ-select websocket; retrying in {}s: {err}",
                                CHAMP_SELECT_WS_RETRY_DELAY.as_secs()
                            ),
                        );
                        logged_waiting_for_client = true;
                    }
                }
            }
            tokio::time::sleep(CHAMP_SELECT_WS_RETRY_DELAY).await;
        }
    });
}

async fn refresh_mobile_matchmaking(handle: &AppHandle, engine: &Arc<Engine>) {
    if engine.mock.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }

    let (phase, matchmaking) =
        tokio::join!(overlay_lcu::fetch_phase(), overlay_lcu::fetch_matchmaking());
    let (Ok(phase), Ok(matchmaking)) = (phase, matchmaking) else {
        return;
    };

    let should_publish = {
        let mut current = engine.current_matchmaking.lock();
        let changed_matchmaking = current.is_some() || matchmaking.is_some();
        current.clone_from(&matchmaking);
        changed_matchmaking
    };
    // An initial subscription can happen while a game is already running.
    // Only matchmaking transitions may replace the phone's current game view.
    if should_publish {
        engine
            .mobile
            .publish_idle(handle, phase.label(), true, matchmaking.as_ref());
    }
}

fn spawn_matchmaking_subscription(handle: AppHandle, engine: Arc<Engine>) {
    tauri::async_runtime::spawn(async move {
        let mut logged_waiting_for_client = false;

        loop {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
            match overlay_lcu::subscribe_matchmaking(tx).await {
                Ok(subscription) => {
                    logged_waiting_for_client = false;
                    events::log(&handle, "info", "Subscribed to matchmaking websocket");
                    refresh_mobile_matchmaking(&handle, &engine).await;

                    loop {
                        tokio::select! {
                            event = rx.recv() => {
                                if event.is_none() {
                                    break;
                                }
                                refresh_mobile_matchmaking(&handle, &engine).await;
                            }
                            () = tokio::time::sleep(CHAMP_SELECT_WS_RETRY_DELAY) => {
                                if subscription.is_finished() {
                                    break;
                                }
                            }
                        }
                    }

                    events::log(
                        &handle,
                        "warn",
                        format!(
                            "Matchmaking websocket stopped; retrying in {}s",
                            CHAMP_SELECT_WS_RETRY_DELAY.as_secs()
                        ),
                    );
                }
                Err(err) => {
                    if !logged_waiting_for_client {
                        events::log(
                            &handle,
                            "warn",
                            format!(
                                "Failed to subscribe to matchmaking websocket; retrying in {}s: {err}",
                                CHAMP_SELECT_WS_RETRY_DELAY.as_secs()
                            ),
                        );
                        logged_waiting_for_client = true;
                    }
                }
            }
            tokio::time::sleep(CHAMP_SELECT_WS_RETRY_DELAY).await;
        }
    });
}

fn spawn_mobile_command_poller(handle: AppHandle, engine: Arc<Engine>) {
    tauri::async_runtime::spawn(async move {
        loop {
            let ready_check_active = engine
                .current_matchmaking
                .lock()
                .as_ref()
                .is_some_and(|state| state.state == "readyCheck");
            if ready_check_active && !engine.mock.load(std::sync::atomic::Ordering::Relaxed) {
                engine.mobile.poll_commands(&handle).await;
                tokio::time::sleep(READY_CHECK_COMMAND_POLL_INTERVAL).await;
            } else {
                tokio::time::sleep(IDLE_COMMAND_STATE_CHECK_INTERVAL).await;
            }
        }
    });
}
