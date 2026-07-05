//! HTTP client + process-lifetime cache for u.gg stats2 API.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use overlay_provider::{ProviderError, Result};
use serde_json::Value;
use tokio::sync::RwLock;

use crate::types::default_overview::OverviewData;
use crate::types::mappings::{Build, Mode, Rank, Region, Role};
use crate::types::matchups::{MatchupData, Matchups};
use crate::types::overview::{ChampOverview, Overview};

const API_VERSIONS_URL: &str =
    "https://static.bigbrain.gg/assets/lol/riot_patch_update/prod/ugg/ugg-api-versions.json";
const STATS_BASE: &str = "https://stats2.u.gg/lol/1.5";
const CACHE_TTL: Duration = Duration::from_hours(6);
const RETRY_ATTEMPTS: usize = 2;
const RETRY_DELAY: Duration = Duration::from_millis(250);

pub type UggApiVersions = HashMap<String, HashMap<String, String>>;

struct UggStatic {
    loaded_at: Instant,
    patch: String,
    prev_patch: Option<String>,
    api_versions: UggApiVersions,
}

pub struct UggApi {
    http: reqwest::Client,
    static_data: RwLock<Option<UggStatic>>,
    overview_cache: RwLock<HashMap<String, ChampOverview>>,
    matchups_cache: RwLock<HashMap<String, Matchups>>,
    champion_ranking_cache: RwLock<HashMap<String, Value>>,
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
            champion_ranking_cache: RwLock::new(HashMap::new()),
        })
    }

    /// Data Dragon `"15.12.1"` → u.gg patch key `"15_12"`.
    #[must_use]
    pub fn patch_from_ddragon(version: &str) -> String {
        version.split('.').take(2).collect::<Vec<_>>().join("_")
    }

    pub async fn ensure_static(&self, ddragon_version: &str) -> Result<()> {
        let patch = Self::patch_from_ddragon(ddragon_version);
        {
            let guard = self.static_data.read().await;
            if guard.as_ref().is_some_and(|static_data| {
                static_data.patch == patch && static_data.loaded_at.elapsed() < CACHE_TTL
            }) {
                return Ok(());
            }
        }

        let api_versions: UggApiVersions = self
            .http
            .get(API_VERSIONS_URL)
            .send_with_retry()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let prev_patch = previous_patch_key(&patch, &api_versions);

        let mut guard = self.static_data.write().await;
        *guard = Some(UggStatic {
            loaded_at: Instant::now(),
            patch,
            prev_patch,
            api_versions,
        });
        drop(guard);
        self.overview_cache.write().await.clear();
        self.matchups_cache.write().await.clear();
        self.champion_ranking_cache.write().await.clear();
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

    pub async fn previous_patch(&self) -> Result<Option<String>> {
        self.static_data
            .read()
            .await
            .as_ref()
            .map(|s| s.prev_patch.clone())
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

    fn champion_ranking_api_version(api_versions: &UggApiVersions, patch: &str) -> String {
        if let Some(versions) = api_versions.get(patch) {
            if let Some(v) = versions.get("champion_ranking") {
                return v.clone();
            }
        }
        "1.5.0".to_string()
    }

    /// Site-wide tier list data for a region / queue / rank bracket.
    pub async fn get_champion_ranking(
        &self,
        region: &str,
        mode: Mode,
        rank_tier: &str,
    ) -> Result<Value> {
        let patch = self.patch().await?;
        self.get_champion_ranking_for_patch(region, mode, rank_tier, &patch)
            .await
    }

    /// Site-wide tier list data for a specific patch.
    pub async fn get_champion_ranking_for_patch(
        &self,
        region: &str,
        mode: Mode,
        rank_tier: &str,
        patch: &str,
    ) -> Result<Value> {
        let api_versions = self.api_versions().await?;
        let api_version = Self::champion_ranking_api_version(&api_versions, patch);
        let cache_key = format!(
            "champion_ranking/{region}/{patch}/{}/{rank_tier}/{api_version}",
            mode.to_api_string()
        );

        {
            let guard = self.champion_ranking_cache.read().await;
            if let Some(cached) = guard.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        let data_path = format!(
            "champion_ranking/{region}/{patch}/{}/{rank_tier}/{api_version}.json",
            mode.to_api_string()
        );
        let url = format!("{STATS_BASE}/{data_path}");
        let data: Value = self
            .http
            .get(&url)
            .send_with_retry()
            .await?
            .error_for_status()?
            .json()
            .await?;

        self.champion_ranking_cache
            .write()
            .await
            .insert(cache_key, data.clone());
        Ok(data)
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
            .send_with_retry()
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

    pub async fn get_matchup_overview(
        &self,
        champ_key: &str,
        enemy_champ_key: &str,
        mode: Mode,
        build: Build,
    ) -> Result<ChampOverview> {
        let patch = self.patch().await?;
        let api_versions = self.api_versions().await?;
        let api_version = Self::overview_api_version(&api_versions, &patch);
        let cache_key = format!(
            "overview-matchup/{patch}/{}/{champ_key}_{enemy_champ_key}/{api_version}",
            mode.to_api_string()
        );

        {
            let guard = self.overview_cache.read().await;
            if let Some(cached) = guard.get(&cache_key) {
                return Ok(cached.clone());
            }
        }

        let data_path = format!(
            "{}/{}/{}/matchups/{champ_key}_{enemy_champ_key}/{api_version}",
            build.to_api_string(),
            patch,
            mode.to_api_string()
        );
        let url = format!("{STATS_BASE}/{data_path}.json");
        let data: ChampOverview = self
            .http
            .get(&url)
            .send_with_retry()
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
        self.get_matchups_for_patch(champ_key, mode, &patch).await
    }

    pub async fn get_matchups_for_patch(
        &self,
        champ_key: &str,
        mode: Mode,
        patch: &str,
    ) -> Result<Matchups> {
        let api_versions = self.api_versions().await?;
        let api_version = Self::matchups_api_version(&api_versions, patch);
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

        let data_path = format!("{patch}/{}/{champ_key}/{api_version}", mode.to_api_string());
        let url = format!("{STATS_BASE}/matchups/{data_path}.json");
        let data: Matchups = self
            .http
            .get(&url)
            .send_with_retry()
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

trait RequestBuilderRetryExt {
    async fn send_with_retry(self) -> std::result::Result<reqwest::Response, reqwest::Error>;
}

impl RequestBuilderRetryExt for reqwest::RequestBuilder {
    async fn send_with_retry(self) -> std::result::Result<reqwest::Response, reqwest::Error> {
        let request = self;
        let mut attempt = 0;
        loop {
            let Some(next) = request.try_clone() else {
                return request.send().await;
            };
            match next.send().await {
                Ok(response) if response.status().is_server_error() && attempt < RETRY_ATTEMPTS => {
                    attempt += 1;
                    tokio::time::sleep(RETRY_DELAY * attempt as u32).await;
                }
                Err(err)
                    if (err.is_connect() || err.is_timeout() || err.is_request())
                        && attempt < RETRY_ATTEMPTS =>
                {
                    attempt += 1;
                    tokio::time::sleep(RETRY_DELAY * attempt as u32).await;
                }
                result => return result,
            }
        }
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
        let Some(data_by_role) = Rank::preferred_order()
            .iter()
            .find_map(|rank| data.get(&reg).and_then(|region_data| region_data.get(rank)))
        else {
            continue;
        };

        if let Some((resolved_role, wrapped)) = data_by_role.get_key_value(&role) {
            let Overview::Default(d) = &wrapped.data;
            return Ok((d.clone(), *resolved_role));
        }

        if let Some((resolved_role, wrapped)) = data_by_role
            .iter()
            .max_by_key(|(_, wrapped)| wrapped.data.matches())
        {
            let Overview::Default(d) = &wrapped.data;
            return Ok((d.clone(), *resolved_role));
        }
    }

    Err(ProviderError::Other("no overview data for champion".into()))
}

/// Pick matchup data for `region`/`role`, falling back to World and the
/// role with the most games (mirrors reference `get_matchups`).
pub fn select_matchups(data: &Matchups, region: Region, role: Role) -> Result<(MatchupData, Role)> {
    for reg in [region, Region::World] {
        let Some(data_by_role) = Rank::preferred_order()
            .iter()
            .find_map(|rank| data.get(&reg).and_then(|region_data| region_data.get(rank)))
        else {
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

fn parse_patch_key(key: &str) -> Option<(u16, u16)> {
    let (major, minor) = key.split_once('_')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

fn previous_patch_key(current: &str, api_versions: &UggApiVersions) -> Option<String> {
    let current = parse_patch_key(current)?;
    api_versions
        .keys()
        .filter_map(|key| parse_patch_key(key).map(|parsed| (key, parsed)))
        .filter(|(_, parsed)| *parsed < current)
        .max_by_key(|(_, parsed)| *parsed)
        .map(|(key, _)| key.clone())
}

#[cfg(test)]
mod tests {
    use super::{previous_patch_key, UggApi};
    use std::collections::HashMap;

    #[test]
    fn patch_from_ddragon_strips_patch_segment() {
        assert_eq!(UggApi::patch_from_ddragon("15.12.1"), "15_12");
        assert_eq!(UggApi::patch_from_ddragon("16.11"), "16_11");
    }

    #[test]
    fn previous_patch_key_picks_nearest_lower_patch() {
        let api_versions = HashMap::from([
            ("15_24".to_string(), HashMap::new()),
            ("16_1".to_string(), HashMap::new()),
            ("16_10".to_string(), HashMap::new()),
            ("16_12".to_string(), HashMap::new()),
        ]);

        assert_eq!(
            previous_patch_key("16_12", &api_versions),
            Some("16_10".to_string())
        );
        assert_eq!(
            previous_patch_key("16_1", &api_versions),
            Some("15_24".to_string())
        );
    }
}
