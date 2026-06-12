//! LCU-related shared types.

use serde::Serialize;

/// Gameflow phases we branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Phase {
    None,
    Lobby,
    Matchmaking,
    ChampSelect,
    InProgress,
    Other,
}

impl Phase {
    pub fn from_api(s: &str) -> Self {
        match s {
            "None" => Phase::None,
            "Lobby" => Phase::Lobby,
            "Matchmaking" | "ReadyCheck" => Phase::Matchmaking,
            "ChampSelect" => Phase::ChampSelect,
            "GameStart" | "InProgress" => Phase::InProgress,
            _ => Phase::Other,
        }
    }

    /// Stable string label sent to the frontend.
    pub fn label(self) -> &'static str {
        match self {
            Phase::None => "None",
            Phase::Lobby => "Lobby",
            Phase::Matchmaking => "Matchmaking",
            Phase::ChampSelect => "ChampSelect",
            Phase::InProgress => "InProgress",
            Phase::Other => "Other",
        }
    }
}

/// Our current pick in champ select.
#[derive(Debug, Clone)]
pub struct MyPick {
    pub champion_id: i64,
    /// LCU position string: "top" | "jungle" | "middle" | "bottom" | "utility".
    pub position: Option<String>,
}

/// A rune page to POST into the client.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunePagePayload {
    pub name: String,
    pub primary_style_id: i64,
    pub sub_style_id: i64,
    pub selected_perk_ids: Vec<i64>,
    pub current: bool,
}

/// Logged-in summoner + current solo-queue standing, emitted to the frontend
/// as the `summoner` event (camelCase to match the TS interface).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummonerInfo {
    pub game_name: String,
    pub tag_line: String,
    pub level: i64,
    pub profile_icon_id: i64,
    /// "" when unranked.
    pub solo_tier: String,
    /// Roman numeral ("II"); "NA" for apex tiers.
    pub solo_division: String,
    pub solo_lp: i64,
    pub solo_wins: i64,
    pub solo_losses: i64,
}

/// One game from the local match history, reduced to what the profile chip
/// shows. Emitted to the frontend inside `match-history` (newest first).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentGame {
    pub champion_id: i64,
    pub win: bool,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub queue_id: i64,
    /// Unix millis.
    pub game_creation: i64,
}
