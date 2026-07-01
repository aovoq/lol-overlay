//! Global hotkeys (desktop only):
//!   * Ctrl+Shift+O — show/focus the normal control window.
//!   * Ctrl+Shift+M — move the overlay to the next monitor.
//!   * Ctrl+Shift+D — cycle debug/mock mode (off → champ select → in game).

use std::sync::Arc;

use tauri::{App, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::engine::{Engine, MockStage};
use crate::events::{log, ChampSelectEvent, PhaseEvent};
use crate::mock::{mock_champ_select_loop, mock_loop};

/// Register the global shortcuts and their handler. Registration is best-effort:
/// a conflicting hotkey is logged rather than crashing the app.
pub fn setup(app: &App, engine: Arc<Engine>) -> tauri::Result<()> {
    let ctrl_shift = Modifiers::CONTROL | Modifiers::SHIFT;
    let toggle = Shortcut::new(Some(ctrl_shift), Code::KeyO);
    let cycle = Shortcut::new(Some(ctrl_shift), Code::KeyM);
    let mock = Shortcut::new(Some(ctrl_shift), Code::KeyD);
    let (toggle_h, cycle_h, mock_h) = (toggle, cycle, mock);

    let engine_hk = engine;

    app.handle().plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, shortcut, event| {
                if event.state != ShortcutState::Pressed {
                    return;
                }
                let Some(win) = app.get_webview_window("main") else {
                    return;
                };
                if shortcut == &toggle_h {
                    crate::engine::show_control_window(app);
                } else if shortcut == &cycle_h {
                    if let Ok(monitors) = win.available_monitors() {
                        if monitors.len() > 1 {
                            // Move to the monitor after the one the window is
                            // actually on — a free-running counter desyncs and
                            // can re-target the current monitor.
                            let cur = win
                                .current_monitor()
                                .ok()
                                .flatten()
                                .and_then(|c| {
                                    monitors.iter().position(|m| m.position() == c.position())
                                })
                                .unwrap_or(0);
                            let m = &monitors[(cur + 1) % monitors.len()];
                            // Park the window on the target monitor first so
                            // current_monitor() resolves to it, then resize
                            // using the target monitor's DPI.
                            let (pos, size) = crate::engine::overlay_bounds(m);
                            let _ = win.set_position(pos);
                            // Position first so the resize happens under the
                            // target monitor's DPI, then re-assert it after
                            // any DPI-change adjustment.
                            let _ = win.set_size(size);
                            let _ = win.set_position(pos);
                        }
                    }
                } else if shortcut == &mock_h {
                    // Cycle the mock scenario; each loop watches the stage and
                    // cleans up after itself (control mode, panel, UI reset).
                    let next = match engine_hk.mock_stage() {
                        MockStage::Off => MockStage::ChampSelect,
                        MockStage::ChampSelect => MockStage::InGame,
                        MockStage::InGame => MockStage::Off,
                    };
                    engine_hk.set_mock_stage(next);
                    eprintln!("mock stage: {next:?}");
                    log(app, "info", format!("mock stage: {next:?}"));
                    match next {
                        MockStage::ChampSelect => {
                            tauri::async_runtime::spawn(mock_champ_select_loop(
                                app.clone(),
                                engine_hk.clone(),
                            ));
                        }
                        MockStage::InGame => {
                            tauri::async_runtime::spawn(mock_loop(app.clone(), engine_hk.clone()));
                        }
                        MockStage::Off => {
                            let _ = app.emit("champ-select", ChampSelectEvent::default());
                            let _ = app.emit(
                                "phase",
                                PhaseEvent {
                                    phase: "None".into(),
                                    client_up: false,
                                    in_game: false,
                                },
                            );
                            crate::engine::apply_window_mode(app, false);
                        }
                    }
                }
            })
            .build(),
    )?;

    let gs = app.global_shortcut();
    for (sc, label) in [
        (toggle, "Ctrl+Shift+O"),
        (cycle, "Ctrl+Shift+M"),
        (mock, "Ctrl+Shift+D"),
    ] {
        if let Err(e) = gs.register(sc) {
            eprintln!("hotkey register failed ({label}): {e}");
        }
    }
    Ok(())
}
