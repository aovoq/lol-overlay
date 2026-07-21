//! Anonymous LOL.PS JSON transport with a short TTL cache.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use overlay_provider::{ProviderError, Result};
use reqwest::header::{CONTENT_TYPE, RETRY_AFTER};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::sync::RwLock;

use crate::types::{Selected, SummaryResponse, SummaryRow, TierResponse, VersionInfo};

const LOLPS_BASE: &str = "https://lol.ps";
// LOL.PS advertises `max-age=1800` on summary/tier responses. Keep our cache
// aligned with that freshness contract instead of inheriting the 6-hour TTL
// used by providers whose datasets update less frequently.
const CACHE_TTL: Duration = Duration::from_mins(30);
const RETRY_ATTEMPTS: usize = 2;
const RETRY_DELAY: Duration = Duration::from_millis(250);
const REGION: i64 = 0;
const TIER: i64 = 2;

struct Cached<T> {
    loaded_at: Instant,
    value: Arc<T>,
}

impl<T> Cached<T> {
    fn fresh(&self) -> Option<Arc<T>> {
        (self.loaded_at.elapsed() < CACHE_TTL).then(|| self.value.clone())
    }
}

pub(crate) struct LolpsApi {
    http: reqwest::Client,
    base_url: String,
    versions: RwLock<Option<Cached<Vec<VersionInfo>>>>,
    summaries: RwLock<HashMap<(i64, i64, i64), Cached<SummaryResponse>>>,
    tiers: RwLock<HashMap<(i64, i64), Cached<TierResponse>>>,
}

impl LolpsApi {
    pub fn new() -> Result<Self> {
        Self::with_base_url(LOLPS_BASE)
    }

    pub(crate) fn with_base_url(base_url: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
            )
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            versions: RwLock::new(None),
            summaries: RwLock::new(HashMap::new()),
            tiers: RwLock::new(HashMap::new()),
        })
    }

    pub async fn summary(&self, champion_id: i64, lane_id: i64) -> Result<Selected<SummaryRow>> {
        let versions = self.versions().await?;
        let current = versions.first().ok_or(ProviderError::NotEnoughData)?;
        match self
            .summary_at(current.version_id, champion_id, lane_id)
            .await
        {
            Ok(row) => Ok(Selected {
                value: row,
                version: current.clone(),
                fallback_from: None,
            }),
            Err(ProviderError::NotEnoughData) => {
                let previous = versions.get(1).ok_or(ProviderError::NotEnoughData)?;
                let row = self
                    .summary_at(previous.version_id, champion_id, lane_id)
                    .await?;
                Ok(Selected {
                    value: row,
                    version: previous.clone(),
                    fallback_from: Some(current.description.clone()),
                })
            }
            Err(error) => Err(error),
        }
    }

    pub async fn tier_list(&self, lane_id: i64) -> Result<Selected<Arc<TierResponse>>> {
        let versions = self.versions().await?;
        let current = versions.first().ok_or(ProviderError::NotEnoughData)?;
        match self.tier_at(current.version_id, lane_id).await {
            Ok(value) if has_tier_data(&value, lane_id) => Ok(Selected {
                value,
                version: current.clone(),
                fallback_from: None,
            }),
            Ok(_) | Err(ProviderError::NotEnoughData) => {
                let previous = versions.get(1).ok_or(ProviderError::NotEnoughData)?;
                let value = self.tier_at(previous.version_id, lane_id).await?;
                if !has_tier_data(&value, lane_id) {
                    return Err(ProviderError::NotEnoughData);
                }
                Ok(Selected {
                    value,
                    version: previous.clone(),
                    fallback_from: Some(current.description.clone()),
                })
            }
            Err(error) => Err(error),
        }
    }

    pub async fn versions(&self) -> Result<Arc<Vec<VersionInfo>>> {
        if let Some(hit) = self.versions.read().await.as_ref().and_then(Cached::fresh) {
            return Ok(hit);
        }
        // Any champion page exposes the same version metadata. Champion 41 is
        // a stable, arbitrary page used only to obtain the SvelteKit payload.
        let value: Value = self
            .fetch_json(&format!("{}/champ/41/__data.json", self.base_url))
            .await?;
        let mut versions = decode_versions(&value)?;
        // LOL.PS currently marks every retained version `isActive: true`, so
        // that field cannot identify the current patch. ISO patch_date is the
        // effective primary key; version_id breaks ties deterministically.
        versions.sort_by(|a, b| {
            b.patch_date
                .cmp(&a.patch_date)
                .then_with(|| b.version_id.cmp(&a.version_id))
        });
        let value = Arc::new(versions);
        *self.versions.write().await = Some(Cached {
            loaded_at: Instant::now(),
            value: value.clone(),
        });
        Ok(value)
    }

    async fn summary_at(
        &self,
        version_id: i64,
        champion_id: i64,
        lane_id: i64,
    ) -> Result<SummaryRow> {
        let key = (version_id, champion_id, lane_id);
        if let Some(hit) = self
            .summaries
            .read()
            .await
            .get(&key)
            .and_then(Cached::fresh)
        {
            return select_summary(&hit, champion_id, lane_id);
        }
        let url = format!(
            "{}/api/champ/{champion_id}/summary.json?region={REGION}&version={version_id}&tier={TIER}&lane={lane_id}",
            self.base_url
        );
        let response: SummaryResponse = self.fetch_json(&url).await?;
        let value = Arc::new(response);
        self.summaries.write().await.insert(
            key,
            Cached {
                loaded_at: Instant::now(),
                value: value.clone(),
            },
        );
        select_summary(&value, champion_id, lane_id)
    }

    async fn tier_at(&self, version_id: i64, lane_id: i64) -> Result<Arc<TierResponse>> {
        let key = (version_id, lane_id);
        if let Some(hit) = self.tiers.read().await.get(&key).and_then(Cached::fresh) {
            return Ok(hit);
        }
        let url = format!(
            "{}/api/statistics/tierlist.json?region={REGION}&version={version_id}&tier={TIER}&lane={lane_id}",
            self.base_url
        );
        let value: Arc<TierResponse> = Arc::new(self.fetch_json(&url).await?);
        self.tiers.write().await.insert(
            key,
            Cached {
                loaded_at: Instant::now(),
                value: value.clone(),
            },
        );
        Ok(value)
    }

    async fn fetch_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let response = self.send_with_retry(url).await?;
        if response.status().as_u16() == 429 {
            let retry_after = response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok());
            return Err(ProviderError::RateLimited { retry_after });
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            let diagnostic: String = body.chars().take(512).collect();
            return Err(ProviderError::Other(format!(
                "LOL.PS HTTP {status}: {diagnostic}"
            )));
        }
        let is_json = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
        if !is_json {
            return Err(ProviderError::InvalidData(
                "LOL.PS returned a non-JSON response".into(),
            ));
        }
        let body = response.bytes().await?;
        serde_json::from_slice(&body)
            .map_err(|error| ProviderError::InvalidData(format!("LOL.PS JSON: {error}")))
    }

    async fn send_with_retry(&self, url: &str) -> Result<reqwest::Response> {
        let mut attempt = 0;
        loop {
            match self.http.get(url).send().await {
                Ok(response) if response.status().is_server_error() && attempt < RETRY_ATTEMPTS => {
                    attempt += 1;
                    tokio::time::sleep(RETRY_DELAY * attempt as u32).await;
                }
                Err(error)
                    if (error.is_connect() || error.is_timeout() || error.is_request())
                        && attempt < RETRY_ATTEMPTS =>
                {
                    attempt += 1;
                    tokio::time::sleep(RETRY_DELAY * attempt as u32).await;
                }
                Ok(response) => return Ok(response),
                Err(error) => return Err(error.into()),
            }
        }
    }
}

fn has_tier_data(response: &TierResponse, lane_id: i64) -> bool {
    response
        .data
        .iter()
        .any(|row| row.lane_id == lane_id && row.count > 0)
}

pub(crate) fn select_summary(
    response: &SummaryResponse,
    champion_id: i64,
    lane_id: i64,
) -> Result<SummaryRow> {
    let mut candidates = response
        .data
        .iter()
        .filter(|row| row.build_type_id == 0 && row.champion_id == champion_id && row.count > 0);
    if lane_id == -1 {
        candidates.max_by_key(|row| row.count).cloned()
    } else {
        candidates.find(|row| row.lane_id == lane_id).cloned()
    }
    .ok_or(ProviderError::NotEnoughData)
}

pub(crate) fn decode_versions(payload: &Value) -> Result<Vec<VersionInfo>> {
    let data = payload
        .get("nodes")
        .and_then(|nodes| nodes.get(1))
        .and_then(|node| node.get("data"))
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::InvalidData("LOL.PS version data is missing".into()))?;
    let root = data
        .first()
        .and_then(Value::as_object)
        .ok_or_else(|| ProviderError::InvalidData("LOL.PS version root is missing".into()))?;
    let versions_ref = ref_index(
        root.get("versionInfo")
            .ok_or_else(|| ProviderError::InvalidData("LOL.PS versionInfo is missing".into()))?,
    )?;
    let version_refs = data
        .get(versions_ref)
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::InvalidData("LOL.PS versionInfo is invalid".into()))?;
    let versions = version_refs
        .iter()
        .map(|version_ref| {
            let object = data
                .get(ref_index(version_ref)?)
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    ProviderError::InvalidData("LOL.PS version row is invalid".into())
                })?;
            Ok(VersionInfo {
                version_id: resolve(data, object, "versionId")?
                    .as_i64()
                    .ok_or_else(|| invalid_version_field("versionId"))?,
                description: resolve(data, object, "description")?
                    .as_str()
                    .ok_or_else(|| invalid_version_field("description"))?
                    .to_string(),
                patch_date: resolve(data, object, "patchDate")?
                    .as_str()
                    .ok_or_else(|| invalid_version_field("patchDate"))?
                    .to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if versions.is_empty() {
        return Err(ProviderError::NotEnoughData);
    }
    Ok(versions)
}

fn resolve<'a>(
    data: &'a [Value],
    object: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<&'a Value> {
    let index = object
        .get(field)
        .ok_or_else(|| invalid_version_field(field))
        .and_then(ref_index)?;
    data.get(index).ok_or_else(|| invalid_version_field(field))
}

fn ref_index(value: &Value) -> Result<usize> {
    value
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| ProviderError::InvalidData("LOL.PS devalue reference is invalid".into()))
}

fn invalid_version_field(field: &str) -> ProviderError {
    ProviderError::InvalidData(format!("LOL.PS version field {field} is invalid"))
}
