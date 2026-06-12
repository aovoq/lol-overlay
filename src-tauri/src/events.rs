//! Payloads emitted to the overlay frontend, plus a small log helper.
//!
//! Field names are camelCase to match the TypeScript interfaces in `src/main.ts`.

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::live_client::EnemyChampion;
use crate::provider::{ItemRecommendation, SkillOrder, ThreatProfile};

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PhaseEvent {
    pub phase: String,
    pub client_up: bool,
    pub in_game: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecommendationsEvent {
    pub self_champion: String,
    /// Data Dragon image id ("Chogath"), for the panel's champion icon.
    pub self_raw_name: String,
    pub self_position: String,
    pub enemies: Vec<EnemyChampion>,
    pub threats: ThreatProfile,
    pub skill_order: Option<SkillOrder>,
    pub items: Vec<ItemRecommendation>,
}

/// Emitted once when a ranked game's result lands (solo-queue W/L count
/// changed between polls).
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LpChangeEvent {
    pub win: bool,
    /// New LP minus old LP. Misleading across a promotion/demotion — the
    /// frontend shows `rank_change` instead when it is non-empty.
    pub lp_delta: i64,
    pub tier: String,
    pub division: String,
    pub lp: i64,
    /// "promoted" | "demoted" | "" (no tier/division change).
    pub rank_change: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RuneImportedEvent {
    pub champion_id: i64,
    pub page_name: String,
}

/// Champ-select state for the HEXGATE panel. Emitted on every parsed change of
/// the champ-select session (deduped by equality), and once with
/// `active: false` when champ select ends.
#[derive(Serialize, Clone, PartialEq, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChampSelectEvent {
    pub active: bool,
    /// "top" | "jungle" | "middle" | "bottom" | "utility" | "" (unknown).
    pub my_role: String,
    /// Hovered or locked champion (0 = none). See `my_locked`.
    pub my_champion_id: i64,
    pub my_locked: bool,
    /// 5 slots in cell order; 0 = not picked/revealed yet.
    pub my_team_champion_ids: Vec<i64>,
    pub enemy_champion_ids: Vec<i64>,
    pub my_bans: Vec<i64>,
    pub enemy_bans: Vec<i64>,
    /// "PLANNING" | "BAN_PICK" | "FINALIZATION" | "GAME_STARTING" | "".
    pub timer_phase: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LogEvent {
    level: String,
    message: String,
}

/// Forward a log line to the frontend console (visible in devtools).
pub fn log(app: &AppHandle, level: &str, message: impl Into<String>) {
    let _ = app.emit(
        "log",
        LogEvent {
            level: level.into(),
            message: message.into(),
        },
    );
}
