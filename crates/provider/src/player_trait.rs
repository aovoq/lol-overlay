//! Player-stat provider interface and capability metadata.

use async_trait::async_trait;
use std::collections::HashSet;

use overlay_types::{
    MatchPage, PlayerChampionStats, PlayerProfile, PlayerRef, ProviderExtras, RefreshResult,
};
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

/// Provider-produced values used by the reusable offline Player Stats contract.
///
/// Adapters build this from their own raw parser fixtures, so the suite verifies
/// actual mappings instead of merely checking hand-written shared DTOs.
pub struct PlayerProviderContractFixture {
    pub profile: PlayerProfile,
    pub pages: Vec<MatchPage>,
    pub champions: Vec<PlayerChampionStats>,
    pub refresh: RefreshResult,
    pub capabilities: ProviderCapabilities,
}

fn extras_match_source(extras: &ProviderExtras, source: &str) -> bool {
    matches!(
        (extras, source),
        (ProviderExtras::Deeplol(_), "deeplol") | (ProviderExtras::Opgg(_), "opgg")
    )
}

/// Validate provider-neutral units, ordering, missing values, pagination, and
/// provider provenance for a fixture emitted by one Player Stats adapter.
pub fn validate_player_provider_contract(
    source: &str,
    fixture: &PlayerProviderContractFixture,
) -> Result<()> {
    let invalid = |message: &str| crate::ProviderError::InvalidData(message.into());
    let profile = &fixture.profile;
    if profile.source != source
        || profile.identity.platform_id.is_empty()
        || profile.identity.game_name.is_empty()
        || profile.identity.tag_line.is_empty()
        || profile.fetched_at <= 0
        || !profile.refresh.app_refresh
        || profile.refresh.site_refresh != fixture.capabilities.site_refresh
        || !extras_match_source(&profile.extras, source)
    {
        return Err(invalid("profile violates the shared Player Stats contract"));
    }
    if profile
        .ladder_percentile
        .is_some_and(|value| !value.is_finite() || !(0.0..=100.0).contains(&value))
    {
        return Err(invalid("ladder percentile must use percentage units"));
    }

    if fixture.pages.len() < 2 || fixture.pages[0].next_cursor.is_none() {
        return Err(invalid("contract fixture must prove pagination"));
    }
    let mut match_ids = HashSet::new();
    for page in &fixture.pages {
        if page.source != source || page.fetched_at <= 0 {
            return Err(invalid("match page lost source or freshness metadata"));
        }
        let mut previous_started_at = i64::MAX;
        for game in &page.matches {
            if game.match_id.is_empty()
                || !match_ids.insert(game.match_id.as_str())
                || game.started_at > previous_started_at
                || game.duration_seconds < 0
                || game.champion_id <= 0
                || game.kills < 0
                || game.deaths < 0
                || game.assists < 0
                || !extras_match_source(&game.extras, source)
            {
                return Err(invalid(
                    "match violates shared units, ordering, or provenance",
                ));
            }
            previous_started_at = game.started_at;
        }
        for failure in &page.partial_failures {
            if failure.match_id.is_empty() || !match_ids.insert(failure.match_id.as_str()) {
                return Err(invalid(
                    "partial failure duplicated or omitted its match ID",
                ));
            }
        }
    }

    let mut champion_keys = HashSet::new();
    for row in &fixture.champions {
        let key = (row.champion_id, row.queue.as_str(), row.role.as_deref());
        if row.source != source
            || row.champion_id <= 0
            || !champion_keys.insert(key)
            || row.games < 0
            || row.wins < 0
            || row.losses < 0
            || row.wins.saturating_add(row.losses) > row.games
            || !row.win_rate.is_finite()
            || !(0.0..=1.0).contains(&row.win_rate)
            || row
                .kda
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || row
                .cs_per_minute
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || !extras_match_source(&row.extras, source)
        {
            return Err(invalid("champion row violates shared units or provenance"));
        }
    }

    if fixture.refresh.source != source
        || !fixture.refresh.cache_invalidated
        || fixture.refresh.mutation_performed != fixture.capabilities.site_refresh
        || fixture.refresh.refreshed_at <= 0
        || !fixture.capabilities.player_profile
        || !fixture.capabilities.match_history
        || !fixture.capabilities.champion_stats
        || fixture.capabilities.live_game
    {
        return Err(invalid("capability or refresh contract mismatch"));
    }
    Ok(())
}

/// Apply the same Player Stats contract suite to an adapter parser fixture.
/// A new provider opts in with one macro call.
#[macro_export]
macro_rules! player_provider_contract_suite {
    ($name:ident, $source:literal, $fixture:expr) => {
        #[test]
        fn $name() {
            let fixture = $fixture;
            $crate::validate_player_provider_contract($source, &fixture)
                .expect("shared Player Stats provider contract");
        }
    };
}
