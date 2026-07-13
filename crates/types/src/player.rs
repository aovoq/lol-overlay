//! Provider-neutral player-stat contracts.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRef {
    pub platform_id: String,
    pub game_name: String,
    pub tag_line: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerIdentity {
    pub platform_id: String,
    pub game_name: String,
    pub tag_line: String,
    pub puuid: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "provider", content = "data", rename_all = "lowercase")]
pub enum ProviderExtras {
    Deeplol(Value),
    Ugg(Value),
    Opgg(Value),
    #[default]
    None,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshAvailability {
    pub app_refresh: bool,
    pub site_refresh: bool,
    pub cooldown_until: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RankedEntry {
    pub queue: String,
    pub tier: Option<String>,
    pub division: Option<String>,
    pub lp: Option<i64>,
    pub wins: Option<i64>,
    pub losses: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeasonRank {
    pub season: String,
    pub queue: String,
    pub tier: Option<String>,
    pub division: Option<String>,
    pub lp: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerProfile {
    pub source: String,
    pub identity: PlayerIdentity,
    pub level: Option<i64>,
    pub profile_icon_id: Option<i64>,
    pub ranks: Vec<RankedEntry>,
    pub previous_seasons: Vec<SeasonRank>,
    pub ladder_rank: Option<i64>,
    pub ladder_percentile: Option<f64>,
    pub fetched_at: i64,
    pub refresh: RefreshAvailability,
    pub extras: ProviderExtras,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchParticipant {
    pub puuid: Option<String>,
    pub game_name: Option<String>,
    pub tag_line: Option<String>,
    pub champion_id: i64,
    pub team_id: i64,
    pub role: Option<String>,
    pub win: bool,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub items: Vec<i64>,
    pub extras: ProviderExtras,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerMatch {
    pub match_id: String,
    pub started_at: i64,
    pub duration_seconds: i64,
    pub queue_id: i64,
    pub remake: bool,
    pub champion_id: i64,
    pub role: Option<String>,
    pub win: bool,
    pub kills: i64,
    pub deaths: i64,
    pub assists: i64,
    pub cs: Option<i64>,
    pub items: Vec<i64>,
    pub spell_ids: Vec<i64>,
    pub perk_ids: Vec<i64>,
    pub participants: Vec<MatchParticipant>,
    pub extras: ProviderExtras,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchFailure {
    pub match_id: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatchPage {
    pub source: String,
    pub matches: Vec<PlayerMatch>,
    pub next_cursor: Option<String>,
    pub partial_failures: Vec<MatchFailure>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerChampionStats {
    pub source: String,
    pub champion_id: i64,
    pub games: i64,
    pub wins: i64,
    pub losses: i64,
    pub win_rate: f64,
    pub kda: Option<f64>,
    pub cs_per_minute: Option<f64>,
    pub role: Option<String>,
    pub queue: String,
    pub extras: ProviderExtras,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshResult {
    pub source: String,
    pub cache_invalidated: bool,
    pub mutation_performed: bool,
    pub refreshed_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn player_contract_serializes_camel_case_and_tagged_extras() {
        let profile = PlayerProfile {
            source: "deeplol".into(),
            identity: PlayerIdentity {
                platform_id: "JP1".into(),
                game_name: "Player".into(),
                tag_line: "JP1".into(),
                puuid: Some("p".into()),
            },
            level: None,
            profile_icon_id: Some(1),
            ranks: vec![],
            previous_seasons: vec![],
            ladder_rank: None,
            ladder_percentile: None,
            fetched_at: 123,
            refresh: RefreshAvailability {
                app_refresh: true,
                ..RefreshAvailability::default()
            },
            extras: ProviderExtras::Deeplol(json!({"aiScore": 73.2})),
        };
        let value = serde_json::to_value(profile).expect("serialize");
        assert_eq!(value["profileIconId"], 1);
        assert_eq!(value["fetchedAt"], 123);
        assert_eq!(value["extras"]["provider"], "deeplol");
        assert_eq!(value["extras"]["data"]["aiScore"], 73.2);
    }
}
