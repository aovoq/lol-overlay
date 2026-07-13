//! Live Client snapshot types shared with providers and frontend events.

use serde::Serialize;

/// A champion on the enemy team, normalized for the rest of the app.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnemyChampion {
    /// Localized display name (for the UI).
    pub name: String,
    /// English name for locale-independent logic (e.g. "Talon", "Chogath").
    pub raw_name: String,
    pub position: String,
    pub items: Vec<i64>,
}

/// One participant (either team) with the identity data that becomes
/// available on the load screen — payload of the "game-players" event.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GamePlayer {
    /// Riot ID, e.g. "Faker#KR1" ("" when unavailable).
    pub riot_id: String,
    /// Localized champion display name (for the UI).
    pub name: String,
    /// English champion name for id lookups (e.g. "Talon").
    pub raw_name: String,
    /// "TOP" | "JUNGLE" | "MIDDLE" | "BOTTOM" | "UTILITY" | "".
    pub position: String,
    pub ally: bool,
}

/// The slice of game state we care about: who we are and who we face.
#[derive(Debug, Clone, Serialize)]
pub struct GameSnapshot {
    pub game_mode: String,
    pub game_time: f64,
    /// Localized display name of our champion (for the UI).
    pub self_champion: String,
    /// English name of our champion (e.g. "Talon"), for id lookups in the
    /// data layer. Empty when spectating / before spawn.
    pub self_raw_name: String,
    pub self_position: String,
    pub enemies: Vec<EnemyChampion>,
    pub allies: Vec<String>,
    /// All ten participants incl. Riot IDs (empty when spectating).
    pub players: Vec<GamePlayer>,
}
