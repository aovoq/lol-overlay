//! Anonymous LOL.PS JSON transport with a short TTL cache.

use std::sync::Arc;
use std::time::Duration;

use overlay_provider::{fetch_json, ProviderError, Result, TtlCache, WINDOWS_USER_AGENT};
use serde_json::Value;

use crate::types::{Selected, SummaryResponse, SummaryRow, TierResponse, VersionInfo};

const LOLPS_BASE: &str = "https://lol.ps";
// LOL.PS advertises `max-age=1800` on summary/tier responses. Keep our cache
// aligned with that freshness contract instead of inheriting the 6-hour TTL
// used by providers whose datasets update less frequently.
const CACHE_TTL: Duration = Duration::from_mins(30);
const REGION: i64 = 0;
const TIER: i64 = 2;

pub(crate) struct LolpsApi {
    http: reqwest::Client,
    base_url: String,
    versions: TtlCache<(), Vec<VersionInfo>>,
    summaries: TtlCache<(i64, i64, i64), SummaryResponse>,
    tiers: TtlCache<(i64, i64), TierResponse>,
}

impl LolpsApi {
    pub fn new() -> Result<Self> {
        Self::with_base_url(LOLPS_BASE)
    }

    pub(crate) fn with_base_url(base_url: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(WINDOWS_USER_AGENT)
            .build()?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
            versions: TtlCache::new(CACHE_TTL),
            summaries: TtlCache::new(CACHE_TTL),
            tiers: TtlCache::new(CACHE_TTL),
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
        self.versions
            .get_or_fetch((), || async {
                // Any champion page exposes the same version metadata.
                // Champion 41 is a stable, arbitrary page used only to obtain
                // the SvelteKit payload.
                let value: Value = fetch_json(
                    &self.http,
                    &format!("{}/champ/41/__data.json", self.base_url),
                    "LOL.PS",
                )
                .await?;
                let mut versions = decode_versions(&value)?;
                // LOL.PS currently marks every retained version
                // `isActive: true`, so that field cannot identify the current
                // patch. ISO patch_date is the effective primary key;
                // version_id breaks ties deterministically.
                versions.sort_by(|a, b| {
                    b.patch_date
                        .cmp(&a.patch_date)
                        .then_with(|| b.version_id.cmp(&a.version_id))
                });
                Ok(versions)
            })
            .await
    }

    async fn summary_at(
        &self,
        version_id: i64,
        champion_id: i64,
        lane_id: i64,
    ) -> Result<SummaryRow> {
        let key = (version_id, champion_id, lane_id);
        let value = self
            .summaries
            .get_or_fetch(key, || async {
                let url = format!(
                    "{}/api/champ/{champion_id}/summary.json?region={REGION}&version={version_id}&tier={TIER}&lane={lane_id}",
                    self.base_url
                );
                fetch_json(&self.http, &url, "LOL.PS").await
            })
            .await?;
        select_summary(&value, champion_id, lane_id)
    }

    async fn tier_at(&self, version_id: i64, lane_id: i64) -> Result<Arc<TierResponse>> {
        self.tiers
            .get_or_fetch((version_id, lane_id), || async {
                let url = format!(
                    "{}/api/statistics/tierlist.json?region={REGION}&version={version_id}&tier={TIER}&lane={lane_id}",
                    self.base_url
                );
                fetch_json(&self.http, &url, "LOL.PS").await
            })
            .await
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
