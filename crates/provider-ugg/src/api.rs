//! HTTP client + process-lifetime cache for u.gg stats2 API.

use std::collections::HashMap;
use std::time::Duration;

use overlay_provider::{ProviderError, Result};
use tokio::sync::RwLock;

use crate::types::default_overview::OverviewData;
use crate::types::mappings::{Build, Mode, Rank, Region, Role};
use crate::types::matchups::{MatchupData, Matchups};
use crate::types::overview::{ChampOverview, Overview};

const API_VERSIONS_URL: &str =
    "https://static.bigbrain.gg/assets/lol/riot_patch_update/prod/ugg/ugg-api-versions.json";
const STATS_BASE: &str = "https://stats2.u.gg/lol/1.5";

pub type UggApiVersions = HashMap<String, HashMap<String, String>>;

struct UggStatic {
    patch: String,
    api_versions: UggApiVersions,
}

pub struct UggApi {
    http: reqwest::Client,
    static_data: RwLock<Option<UggStatic>>,
    overview_cache: RwLock<HashMap<String, ChampOverview>>,
    matchups_cache: RwLock<HashMap<String, Matchups>>,
}

impl UggApi {
    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
            )
            .build()?;
        Ok(Self {
            http,
            static_data: RwLock::new(None),
            overview_cache: RwLock::new(HashMap::new()),
            matchups_cache: RwLock::new(HashMap::new()),
        })
    }

    /// Data Dragon `"15.12.1"` → u.gg patch key `"15_12"`.
    #[must_use]
    pub fn patch_from_ddragon(version: &str) -> String {
        let mut parts: Vec<&str> = version.split('.').collect();
        if parts.len() > 1 {
            parts.pop();
        }
        parts.join("_")
    }

    pub async fn ensure_static(&self, ddragon_version: &str) -> Result<()> {
        {
            let guard = self.static_data.read().await;
            if guard.is_some() {
                return Ok(());
            }
        }

        let patch = Self::patch_from_ddragon(ddragon_version);
        let api_versions: UggApiVersions = self
            .http
            .get(API_VERSIONS_URL)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let mut guard = self.static_data.write().await;
        if guard.is_none() {
            *guard = Some(UggStatic {
                patch,
                api_versions,
            });
        }
        Ok(())
    }

    async fn patch(&self) -> Result<String> {
        self.static_data
            .read()
            .await
            .as_ref()
            .map(|s| s.patch.clone())
            .ok_or_else(|| ProviderError::Other("ugg static data not initialized".into()))
    }

    async fn api_versions(&self) -> Result<UggApiVersions> {
        self.static_data
            .read()
            .await
            .as_ref()
            .map(|s| s.api_versions.clone())
            .ok_or_else(|| ProviderError::Other("ugg static data not initialized".into()))
    }

    fn overview_api_version(api_versions: &UggApiVersions, patch: &str) -> String {
        if let Some(versions) = api_versions.get(patch) {
            if let Some(v) = versions.get("overview") {
                return v.clone();
            }
        }
        "1.5.0".to_string()
    }

    fn matchups_api_version(api_versions: &UggApiVersions, patch: &str) -> String {
        if let Some(versions) = api_versions.get(patch) {
            if let Some(v) = versions.get("matchups") {
                return v.clone();
            }
        }
        "1.5.0".to_string()
    }

    pub async fn get_overview(
        &self,
        champ_key: &str,
        mode: Mode,
        build: Build,
    ) -> Result<ChampOverview> {
        let patch = self.patch().await?;
        let api_versions = self.api_versions().await?;
        let api_version = Self::overview_api_version(&api_versions, &patch);
        let cache_key = format!(
            "overview/{patch}/{}/{champ_key}/{api_version}",
            mode.to_api_string()
        );

        {
            let guard = self.overview_cache.read().await;
            if let Some(cached) = guard.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        let data_path = format!(
            "{}/{}/{}/{champ_key}/{api_version}",
            build.to_api_string(),
            patch,
            mode.to_api_string()
        );
        let url = format!("{STATS_BASE}/{data_path}.json");
        let data: ChampOverview = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        self.overview_cache
            .write()
            .await
            .insert(cache_key, data.clone());
        Ok(data)
    }

    pub async fn get_matchups(&self, champ_key: &str, mode: Mode) -> Result<Matchups> {
        let patch = self.patch().await?;
        let api_versions = self.api_versions().await?;
        let api_version = Self::matchups_api_version(&api_versions, &patch);
        let cache_key = format!(
            "matchups/{patch}/{}/{champ_key}/{api_version}",
            mode.to_api_string()
        );

        {
            let guard = self.matchups_cache.read().await;
            if let Some(cached) = guard.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        let data_path = format!(
            "{patch}/{}/{champ_key}/{api_version}",
            mode.to_api_string()
        );
        let url = format!("{STATS_BASE}/matchups/{data_path}.json");
        let data: Matchups = self
            .http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        self.matchups_cache
            .write()
            .await
            .insert(cache_key, data.clone());
        Ok(data)
    }
}

/// Pick overview data for `region`/`role`, falling back to World and the
/// role with the most games (mirrors reference `get_stats`).
pub fn select_overview(
    data: &ChampOverview,
    region: Region,
    role: Role,
) -> Result<(OverviewData, Role)> {
    for reg in [region, Region::World] {
        let Some(data_by_role) = Rank::preferred_order().iter().find_map(|rank| {
            data.get(&reg).and_then(|region_data| region_data.get(rank))
        }) else {
            continue;
        };

        if let Some((resolved_role, wrapped)) = data_by_role.get_key_value(&role) {
            if let Overview::Default(d) = &wrapped.data {
                return Ok((d.clone(), *resolved_role));
            }
        }

        if let Some((resolved_role, wrapped)) = data_by_role
            .iter()
            .max_by_key(|(_, wrapped)| wrapped.data.matches())
        {
            if let Overview::Default(d) = &wrapped.data {
                return Ok((d.clone(), *resolved_role));
            }
        }
    }

    Err(ProviderError::Other("no overview data for champion".into()))
}

/// Pick matchup data for `region`/`role`, falling back to World and the
/// role with the most games (mirrors reference `get_matchups`).
pub fn select_matchups(
    data: &Matchups,
    region: Region,
    role: Role,
) -> Result<(MatchupData, Role)> {
    for reg in [region, Region::World] {
        let Some(data_by_role) = Rank::preferred_order().iter().find_map(|rank| {
            data.get(&reg).and_then(|region_data| region_data.get(rank))
        }) else {
            continue;
        };

        if let Some((resolved_role, wrapped)) = data_by_role.get_key_value(&role) {
            return Ok((wrapped.data.clone(), *resolved_role));
        }

        if let Some((resolved_role, wrapped)) = data_by_role
            .iter()
            .max_by_key(|(_, wrapped)| wrapped.data.total_matches)
        {
            return Ok((wrapped.data.clone(), *resolved_role));
        }
    }

    Err(ProviderError::Other("no matchup data for champion".into()))
}

#[cfg(test)]
mod tests {
    use super::UggApi;

    #[test]
    fn patch_from_ddragon_strips_patch_segment() {
        assert_eq!(UggApi::patch_from_ddragon("15.12.1"), "15_12");
        assert_eq!(UggApi::patch_from_ddragon("16.11"), "16");
    }
}
