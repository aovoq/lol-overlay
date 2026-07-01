//! Commands invoked from the frontend (`@tauri-apps/api` `invoke`).
//!
//! The HEXGATE data commands are thin proxies to the provider — it caches
//! per (patch, role, champion), so after the first load these are instant.
//! Errors cross the boundary as their `Display` string; the frontend branches
//! on the literal `"not-enough-data"` (`Error::NotEnoughData`).

use std::sync::atomic::Ordering;
use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::engine::{Engine, PanelPosition, Settings, UiLayout, WindowPosition};
use crate::error;
use crate::events::{log, RuneImportedEvent};
use crate::hittest::HitRegion;
use overlay_lcu::{self as lcu, RunePagePayload};
use overlay_provider::BuildProvider;
use overlay_provider::ProviderKind;
use overlay_types::{CounterEntry, RuneBuild, TierEntry};

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
pub fn set_auto_import(engine: State<'_, Arc<Engine>>, enabled: bool) -> error::Result<()> {
    {
        engine.settings.lock().unwrap().auto_import_runes = enabled;
    }
    engine.persist()
}

#[tauri::command]
pub fn set_pinned(engine: State<'_, Arc<Engine>>, pinned: bool) -> error::Result<()> {
    {
        engine.settings.lock().unwrap().pinned = pinned;
    }
    engine.persist()
}

#[tauri::command]
pub fn set_import_spells(engine: State<'_, Arc<Engine>>, enabled: bool) -> error::Result<()> {
    {
        engine.settings.lock().unwrap().import_spells = enabled;
    }
    engine.persist()
}

#[tauri::command]
pub fn set_spells_flipped(engine: State<'_, Arc<Engine>>, flipped: bool) -> error::Result<()> {
    {
        engine.settings.lock().unwrap().spells_flipped = flipped;
    }
    engine.persist()
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
        engine.ui_layout.lock().unwrap().ingame_panel = Some(PanelPosition { left, top });
    }
    engine.persist()
}

#[tauri::command]
pub fn set_champselect_window_position(
    engine: State<'_, Arc<Engine>>,
    x: f64,
    y: f64,
) -> error::Result<()> {
    if !x.is_finite() || !y.is_finite() {
        return Err(error::Error::Other("invalid window position".into()));
    }
    {
        engine.ui_layout.lock().unwrap().champselect_window = Some(WindowPosition { x, y });
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
    *engine.hit_regions.lock().unwrap() = regions;
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
        engine.ui_layout.lock().unwrap().ingame_collapsed = collapsed;
    }
    engine.persist()
}

/// Tier list for a role (strong picks / ban targets).
#[tauri::command]
pub async fn get_tier_list(
    engine: State<'_, Arc<Engine>>,
    role: String,
) -> error::Result<Vec<TierEntry>> {
    engine.provider.tier_list(&role).await.map_err(|e| {
        eprintln!("get_tier_list failed role={role:?}: {e}");
        e.into()
    })
}

/// Champions that counter `champion_id` in `role`, best counters first.
#[tauri::command]
pub async fn get_counters(
    engine: State<'_, Arc<Engine>>,
    champion_id: i64,
    role: String,
) -> error::Result<Vec<CounterEntry>> {
    engine
        .provider
        .counters(champion_id, &role)
        .await
        .map_err(|e| {
            eprintln!("get_counters failed champion_id={champion_id} role={role:?}: {e}");
            e.into()
        })
}

/// Detailed rune page (incl. shards + spells). `enemy_champion_id` asks for a
/// matchup-specific page; thin matchups can still return "not-enough-data".
#[tauri::command]
pub async fn get_rune_build(
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
            eprintln!(
                "get_rune_build failed champion_id={champion_id} role={role:?} enemy_champion_id={enemy_champion_id:?}: {e}"
            );
            e.into()
        })
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
        engine.settings.lock().unwrap().data_source = parsed;
    }
    engine.persist()?;
    let _ = app.emit("data-source", kind);
    Ok(())
}
