//! Provider recommendation types shared with the frontend.

use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// Describes the population and freshness behind a normalized provider result.
/// Optional fields remain absent when the upstream source does not disclose them.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataProvenance {
    pub provider: String,
    pub region: Option<String>,
    pub patch: Option<String>,
    pub rank: Option<String>,
    pub sample_window: Option<String>,
    pub fetched_at: u64,
    pub estimated: bool,
    pub fallback_from: Option<String>,
}

impl DataProvenance {
    pub fn now(provider: impl Into<String>) -> Self {
        let fetched_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            provider: provider.into(),
            region: None,
            patch: None,
            rank: None,
            sample_window: None,
            fetched_at,
            estimated: false,
            fallback_from: None,
        }
    }
}

/// Damage-profile of the enemy team, derived from their champions. A real
/// provider would compute this from champion data; it drives armor/MR choices.
#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreatProfile {
    pub ad_count: u8,
    pub ap_count: u8,
    pub tank_count: u8,
    pub cc_heavy: bool,
}

/// One recommended item.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemRecommendation {
    pub item_id: i64,
    pub name: String,
    /// 0.0–1.0 confidence/priority used purely for ordering in the UI.
    pub score: f32,
    pub reason: String,
}

/// Recommended skill leveling order. Skill ids follow Riot's convention:
/// 1 = Q, 2 = W, 3 = E, 4 = R.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOrder {
    /// Basic-skill max priority, e.g. `[3, 1, 2]` = E > Q > W.
    pub max_order: Vec<i64>,
    /// Level-by-level order as provided by the data source.
    pub level_order: Vec<i64>,
    pub win_rate: f64,
    pub games: i64,
}

/// A rune page recommendation, provider-side mirror of `RunePagePayload`.
#[derive(Debug, Clone, Serialize)]
pub struct RuneRecommendation {
    pub name: String,
    pub primary_style_id: i64,
    pub sub_style_id: i64,
    pub selected_perk_ids: Vec<i64>,
}

/// One row of the per-role tier list (champ-select "strong picks" / "ban targets").
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TierEntry {
    pub champion_id: i64,
    /// 0..1
    pub win_rate: f64,
    /// Win-rate change vs the previous patch, in percentage points.
    /// `None` means the provider did not expose a comparable prior sample.
    pub win_rate_delta: Option<f64>,
    /// Estimated games this patch. `None` is distinct from a real zero-game sample.
    pub games: Option<i64>,
    /// 0..1
    pub pick_rate: f64,
    /// 0..1
    pub ban_rate: f64,
    pub provenance: DataProvenance,
}

/// A champion that counters the queried champion. `win_rate` is the COUNTER
/// champion's win rate against the subject (0..1, already inverted from the
/// subject's perspective).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CounterEntry {
    pub champion_id: i64,
    pub win_rate: f64,
    pub games: i64,
}

/// A full rune-page recommendation for the champ-select panel, including stat
/// shards and summoner spells, with the stats that back it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuneBuild {
    pub page_name: String,
    /// DeepLoL lane the data came from ("Jungle", …).
    pub lane: String,
    /// 0..1
    pub win_rate: f64,
    pub games: i64,
    pub primary_style_id: i64,
    pub sub_style_id: i64,
    /// [keystone, p1, p2, p3]
    pub primary_perk_ids: Vec<i64>,
    /// [s1, s2]
    pub sub_perk_ids: Vec<i64>,
    /// [offense, flex, defense]
    pub shard_ids: Vec<i64>,
    /// [spell1, spell2]; empty = unknown
    pub spell_ids: Vec<i64>,
    /// True when built against a specific enemy (matchup tab).
    pub matchup: bool,
}
