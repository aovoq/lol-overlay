//! Serde types for LoLalytics' internal `mega` JSON API.
//!
//! Only the fields this provider consumes are modeled; every response also
//! carries `cache` / `response` metadata that we deliberately ignore. All
//! percentages arrive as 0–100 numbers (e.g. `wr: 52.2`), so callers divide by
//! 100 to reach the 0..1 contract of [`overlay_types`].

use std::collections::HashMap;

use serde::{Deserialize, Deserializer};

/// LoLalytics is inconsistent about number encoding — some percentages arrive
/// as JSON numbers, others as strings (`stats.wr` is `"51.32"`). Accept either.
fn lenient_f64<'de, D>(deserializer: D) -> std::result::Result<f64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(f64),
        Str(String),
    }
    match NumOrStr::deserialize(deserializer)? {
        NumOrStr::Num(n) => Ok(n),
        NumOrStr::Str(s) => s.trim().parse().map_err(serde::de::Error::custom),
    }
}

/// `ep=build-itemset`: single-item and boot popularity for a champion/lane.
#[derive(Debug, Clone, Deserialize)]
pub struct ItemSetResponse {
    #[serde(rename = "itemSets")]
    pub item_sets: ItemSets,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ItemSets {
    /// Core single items, in LoLalytics' recommended order (NOT sorted by
    /// games). Each row is `[item_id, games, wins]`; the id is a string because
    /// sibling sets (`itemSet2`, …) key multi-item combos as `"3161_3071"`.
    #[serde(rename = "itemSet1", default)]
    pub item_set1: Vec<ItemStat>,
    /// Boots by popularity, same row shape as [`Self::item_set1`].
    #[serde(rename = "itemBootSet1", default)]
    pub item_boot_set1: Vec<ItemStat>,
}

/// `[item_id, games, appearances]` — the id is a string for combo
/// compatibility; the second number is the lane's sample size and the third is
/// how often the item shows up (a popularity count, NOT a win count). The
/// champion's real win rate comes from the `counter` header instead.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct ItemStat(pub String, pub i64, pub i64);

/// `ep=build-earlyset`: starting item combos.
#[derive(Debug, Clone, Deserialize)]
pub struct EarlySetResponse {
    /// Each row is `[underscore_joined_item_ids, win_rate, pick_rate, games]`,
    /// sorted by games descending.
    #[serde(rename = "earlySet", default)]
    pub early_set: Vec<EarlyStat>,
}

/// `[combo, win_rate, pick_rate, games]`. Only the combo is consumed; the
/// trailing stats are modeled to document the wire shape.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct EarlyStat(pub String, pub f64, pub f64, pub i64);

/// `ep=counter`: header stats + the champions that counter the subject.
#[derive(Debug, Clone, Deserialize)]
pub struct CounterResponse {
    pub stats: CounterStats,
    /// Sorted by `vsWr` descending (strongest counters first).
    #[serde(default)]
    pub counters: Vec<CounterRow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CounterStats {
    /// The lane LoLalytics considers the champion's main lane.
    #[serde(rename = "defaultLane", default)]
    pub default_lane: String,
    /// The champion's win rate in this lane, 0–100.
    #[serde(default, deserialize_with = "lenient_f64")]
    pub wr: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CounterRow {
    pub cid: i64,
    /// The counter champion's win rate against the subject, 0–100.
    #[serde(rename = "vsWr")]
    pub vs_wr: f64,
    /// Games in this matchup.
    pub n: i64,
}

/// `ep=tier`: the site-wide tier list.
///
/// Nesting is `tier[group][lane][cid]`; `group` keys are opaque champion
/// buckets and a `cid` appears under exactly one group per lane, so flattening
/// across all groups yields each champion once per lane.
#[derive(Debug, Clone, Deserialize)]
pub struct TierResponse {
    #[serde(default)]
    pub tier: HashMap<String, TierGroup>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TierGroup {
    #[serde(default)]
    pub lane: HashMap<String, TierLane>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TierLane {
    /// champion id (as a string key) → its stats in this lane.
    #[serde(default)]
    pub cid: HashMap<String, TierChampion>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TierChampion {
    /// Win rate, 0–100.
    pub wr: f64,
    /// Pick rate, 0–100.
    pub pr: f64,
    /// Ban rate, 0–100.
    pub br: f64,
    pub games: i64,
}
