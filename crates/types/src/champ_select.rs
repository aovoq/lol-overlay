//! Champ-select state for the HEXGATE panel.

use serde::Serialize;

/// Emitted on every parsed change of the champ-select session (deduped by
/// equality), and once with `active: false` when champ select ends.
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
