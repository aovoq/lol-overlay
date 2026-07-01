//! lol-overlay — a lightweight, Tauri-based League of Legends overlay.
//!
//! App modules (`src-tauri/src/`):
//!   * `engine`      — shared state + the poller / rune-import tasks
//!   * `events`      — payloads emitted to the frontend
//!   * `hittest`     — region-based click-through (always-clickable headers)
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
mod mock;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tauri::{Manager, WindowEvent};

use overlay_ddragon::DdragonClient;
use overlay_provider::{ProviderKind, ProviderProxy};
use overlay_provider_deeplol::DeepLolProvider;
use overlay_provider_ugg::UggProvider;

use crate::engine::{Engine, MockStage, Settings};
use overlay_live_client::LiveClient;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let ddragon = Arc::new(DdragonClient::new());
    let mut proxy = ProviderProxy::new(ProviderKind::Deeplol);
    proxy.register(
        ProviderKind::Deeplol,
        Arc::new(DeepLolProvider::new(ddragon.clone()).expect("failed to build DeepLoL provider")),
    );
    proxy.register(
        ProviderKind::Ugg,
        Arc::new(UggProvider::new(ddragon.clone()).expect("failed to build u.gg provider")),
    );
    let provider = Arc::new(proxy);

    let engine = Arc::new(Engine {
        provider,
        live: LiveClient::new().expect("failed to build Live Client http client"),
        settings: Mutex::new(Settings::default()),
        ui_layout: Mutex::new(Default::default()),
        store_path: Mutex::new(None),
        mock: AtomicBool::new(false),
        mock_stage: Mutex::new(MockStage::Off),
        last_champ_select: Mutex::new(None),
        hit_regions: Mutex::new(Vec::new()),
        drag_active: AtomicBool::new(false),
        forced_interactive: AtomicBool::new(false),
        interactive_applied: AtomicBool::new(false),
        window_champselect: AtomicBool::new(false),
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(engine.clone())
        .setup(move |app| {
            engine.init_store(app.path().app_config_dir()?.join("settings.json"))?;

            if let Some(win) = app.get_webview_window("main") {
                // Start click-through so the overlay never steals game clicks.
                let _ = win.set_ignore_cursor_events(true);
            }
            // Cover the whole monitor so panels anchor to the real screen edges
            // regardless of resolution / HiDPI scaling. The normal control
            // window starts as a compact status window near the lower-left.
            engine::apply_overlay_bounds(app.handle());
            engine::apply_control_layout(app.handle(), false);

            #[cfg(desktop)]
            hotkeys::setup(app, engine.clone())?;

            // Champ-select WebSocket → channel → rune processor (event-driven);
            // the poller tracks phase / in-game state and feeds the same channel.
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
            overlay_lcu::subscribe_champ_select(tx.clone())
                .expect("failed to subscribe to champ-select");

            let handle = app.handle().clone();
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
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_auto_import,
            commands::set_interactive,
            commands::set_hit_regions,
            commands::set_drag_active,
            commands::set_ingame_collapsed,
            commands::set_pinned,
            commands::set_import_spells,
            commands::set_spells_flipped,
            commands::get_ui_layout,
            commands::set_ingame_panel_position,
            commands::set_champselect_window_position,
            commands::get_tier_list,
            commands::get_counters,
            commands::get_rune_build,
            commands::import_build,
            commands::get_data_source,
            commands::list_data_sources,
            commands::set_data_source,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
