//! lol-overlay — a lightweight, Tauri-based League of Legends overlay.
//!
//! Modules:
//!   * `live_client` — reads in-game state from the Live Client Data API.
//!   * `lcu`         — LCU access via `irelia` (phase, champ select, runes).
//!   * `provider`    — pluggable data source for items & rune pages.
//!   * `engine`      — shared state + the poller / rune-import tasks.
//!   * `events`      — payloads emitted to the frontend.
//!   * `hittest`     — region-based click-through (always-clickable headers).
//!   * `mock`        — debug mode driving synthetic state (Ctrl+Shift+D).
//!   * `hotkeys`     — global shortcuts.
//!   * `commands`    — frontend-invokable commands.

mod commands;
mod engine;
mod error;
mod events;
mod hittest;
#[cfg(desktop)]
mod hotkeys;
mod lcu;
mod live_client;
mod mock;
mod provider;

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tauri::Manager;

use crate::engine::{Engine, MockStage, Settings};
use crate::live_client::LiveClient;
use crate::provider::deeplol::DeepLolProvider;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // The data source. `DeepLolProvider` pulls real meta builds & runes from
    // DeepLoL; swap in `HardcodedProvider` for a fully-offline fallback.
    let engine = Arc::new(Engine {
        provider: Arc::new(DeepLolProvider::new().expect("failed to build data provider")),
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
                // Cover the whole monitor so panels anchor to the real screen
                // edges regardless of resolution / HiDPI scaling. LoL borderless
                // fills the monitor, so a monitor-sized overlay lines up with it.
                if let Ok(Some(monitor)) = win.primary_monitor() {
                    let (pos, size) = engine::overlay_bounds(&monitor);
                    let _ = win.set_position(pos);
                    let _ = win.set_size(size);
                }
                // Start click-through so the overlay never steals game clicks.
                let _ = win.set_ignore_cursor_events(true);
            }

            #[cfg(desktop)]
            hotkeys::setup(app, engine.clone())?;

            // Champ-select WebSocket → channel → rune processor (event-driven);
            // the poller tracks phase / in-game state and feeds the same channel.
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Value>();
            let ws = lcu::subscribe_champ_select(tx.clone());
            std::mem::forget(ws); // keep alive for the app's lifetime

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(engine::rune_processor(handle.clone(), engine.clone(), rx));
            tauri::async_runtime::spawn(engine::poller(handle.clone(), engine.clone(), tx));
            // Region-based click-through: headers stay clickable, the rest of
            // the overlay passes clicks to the game.
            tauri::async_runtime::spawn(hittest::cursor_watcher(handle, engine.clone()));
            Ok(())
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
