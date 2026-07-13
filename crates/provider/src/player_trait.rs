//! Player-stat provider interface and capability metadata.

use async_trait::async_trait;
use overlay_types::{MatchPage, PlayerChampionStats, PlayerProfile, PlayerRef, RefreshResult};
use serde::{Deserialize, Serialize};

use crate::error::Result;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilities {
    pub builds: bool,
    pub player_profile: bool,
    pub match_history: bool,
    pub champion_stats: bool,
    pub live_game: bool,
    pub direct_api: bool,
    pub site_refresh: bool,
    pub regions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDescriptor {
    pub id: String,
    pub label: String,
    pub capabilities: ProviderCapabilities,
}

#[async_trait]
pub trait PlayerStatsProvider: Send + Sync {
    async fn profile(&self, player: &PlayerRef, force: bool) -> Result<PlayerProfile>;

    async fn recent_matches(
        &self,
        player: &PlayerRef,
        cursor: Option<&str>,
        queue: Option<i64>,
        force: bool,
    ) -> Result<MatchPage>;

    async fn champion_stats(
        &self,
        player: &PlayerRef,
        season: Option<&str>,
        queue: Option<&str>,
        role: Option<&str>,
        force: bool,
    ) -> Result<Vec<PlayerChampionStats>>;

    async fn refresh(&self, player: &PlayerRef) -> Result<RefreshResult>;

    fn capabilities(&self) -> ProviderCapabilities;
}
