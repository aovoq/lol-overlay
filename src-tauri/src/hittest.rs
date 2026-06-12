//! Region-based click-through.
//!
//! The OS only supports click-through per *window*, but the overlay wants it
//! per *element*: panel headers (and the whole champ-select panel) should
//! react to the mouse while everything else passes clicks to the game.
//!
//! The frontend reports the rects of its `data-hit` elements
//! (`set_hit_regions`), and [`cursor_watcher`] polls the global cursor:
//! click-through is disabled only while the cursor is inside a reported rect,
//! while a panel drag is in progress, or while the Ctrl+Shift+O emergency
//! override is on.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::engine::Engine;

/// One clickable region, in window-relative logical (CSS) pixels — the same
/// coordinates `getBoundingClientRect` reports on the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HitRegion {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

impl HitRegion {
    fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.left && x < self.left + self.width && y >= self.top && y < self.top + self.height
    }
}

pub fn point_in_regions(regions: &[HitRegion], x: f64, y: f64) -> bool {
    regions.iter().any(|r| r.contains(x, y))
}

/// Cursor sample rate. At ~60 Hz the worst case race — clicking within one
/// tick of crossing a region edge — is a single lost click, never a stray one.
const TICK: Duration = Duration::from_millis(16);

/// Polls the cursor and flips window click-through on transitions.
///
/// This task is the only writer of the actual window style while running;
/// `apply_window_mode` resets both the style and `interactive_applied`
/// together so the cached state never goes stale across mode switches.
pub async fn cursor_watcher(app: AppHandle, engine: Arc<Engine>) {
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        ticker.tick().await;
        let Some(win) = app.get_webview_window("main") else {
            continue;
        };

        let interactive = engine.forced_interactive.load(Ordering::SeqCst)
            || engine.drag_active.load(Ordering::SeqCst)
            || cursor_in_regions(&app, &win, &engine);

        if engine
            .interactive_applied
            .swap(interactive, Ordering::SeqCst)
            != interactive
        {
            let _ = win.set_ignore_cursor_events(!interactive);
        }
    }
}

fn cursor_in_regions(app: &AppHandle, win: &WebviewWindow, engine: &Engine) -> bool {
    let regions = engine.hit_regions.lock().unwrap().clone();
    if regions.is_empty() {
        return false;
    }
    let (Ok(cursor), Ok(origin), Ok(scale)) = (
        app.cursor_position(),
        win.outer_position(),
        win.scale_factor(),
    ) else {
        return false;
    };
    // Global physical cursor → window-relative logical (CSS) coordinates.
    // The window is undecorated, so its outer position is the webview origin.
    let x = (cursor.x - origin.x as f64) / scale;
    let y = (cursor.y - origin.y as f64) / scale;
    point_in_regions(&regions, x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: HitRegion = HitRegion {
        left: 100.0,
        top: 50.0,
        width: 200.0,
        height: 30.0,
    };

    #[test]
    fn point_inside_region_hits() {
        assert!(point_in_regions(&[HEADER], 150.0, 60.0));
        // Top-left corner is inclusive.
        assert!(point_in_regions(&[HEADER], 100.0, 50.0));
    }

    #[test]
    fn point_outside_region_misses() {
        assert!(!point_in_regions(&[HEADER], 99.9, 60.0));
        assert!(!point_in_regions(&[HEADER], 150.0, 49.9));
        // Bottom-right corner is exclusive.
        assert!(!point_in_regions(&[HEADER], 300.0, 80.0));
    }

    #[test]
    fn any_of_multiple_regions_hits() {
        let second = HitRegion {
            left: 0.0,
            top: 0.0,
            width: 10.0,
            height: 10.0,
        };
        assert!(point_in_regions(&[HEADER, second], 5.0, 5.0));
        assert!(!point_in_regions(&[], 5.0, 5.0));
    }
}
