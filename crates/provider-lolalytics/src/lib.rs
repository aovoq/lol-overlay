//! LoLalytics stats provider implementing [`BuildProvider`].
//!
//! Data comes from LoLalytics' internal `mega` JSON API (the same endpoint the
//! site's Qwik frontend calls). Four endpoints are used:
//!
//! * `ep=build-itemset` / `ep=build-earlyset` — item popularity → [`items`].
//! * `ep=counter` — the champions that counter the subject → [`counters`].
//! * `ep=tier` — the site-wide tier list → [`tier_list`].
//!
//! **Runes, skill order and summoner spells are intentionally unsupported.**
//! LoLalytics serves the primary build object (runes/skills/spells) only inside
//! its server-rendered HTML — there is no clean JSON endpoint for it — so
//! [`runes`], [`skill_order`] and [`rune_build`] return [`ProviderError::NotEnoughData`].
//! Use DeepLoL or u.gg when rune auto-import is needed.
//!
//! Champion-name↔id and item-id→name come from Data Dragon, shared with the
//! other providers so icon versions line up.
//!
//! [`items`]: LolalyticsProvider::items
//! [`counters`]: LolalyticsProvider::counters
//! [`tier_list`]: LolalyticsProvider::tier_list
//! [`runes`]: LolalyticsProvider::runes
//! [`skill_order`]: LolalyticsProvider::skill_order
//! [`rune_build`]: LolalyticsProvider::rune_build

mod api;
mod types;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use overlay_ddragon::{normalize, DdragonClient};
use overlay_provider::{
    counter_entries_from_subject_losses, item_recommendations, BuildProvider, CounterEntry,
    ItemRecommendation, ProviderError, Result, RuneRecommendation,
};
use overlay_types::{GameSnapshot, TierEntry};

use crate::api::LolalyticsApi;
use crate::types::EarlyStat;

/// Pick rates below this (as a 0..1 fraction) are fringe off-meta picks and are
/// dropped from the tier list — mirrors the DeepLoL provider's 0.5% floor.
const MIN_TIER_PICK_RATE: f64 = 0.005;

pub struct LolalyticsProvider {
    ddragon: Arc<DdragonClient>,
    api: LolalyticsApi,
}

impl LolalyticsProvider {
    pub fn new(ddragon: Arc<DdragonClient>) -> Result<Self> {
        Ok(Self {
            ddragon,
            api: LolalyticsApi::new()?,
        })
    }

    async fn champion_id(&self, raw_name: &str) -> Option<i64> {
        let champs = self.ddragon.champions().await.ok()?;
        champs.name_to_id.get(&normalize(raw_name)).copied()
    }

    /// LoLalytics champion slug — the Data Dragon alias, lowercased
    /// (`MonkeyKing` → `monkeyking`, `Chogath` → `chogath`).
    async fn champion_slug(&self, champion_id: i64) -> Result<String> {
        let champs = self
            .ddragon
            .champions()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        champs
            .id_to_image
            .get(&champion_id)
            .map(|alias| alias.to_ascii_lowercase())
            .ok_or_else(|| ProviderError::Other(format!("unknown champion id: {champion_id}")))
    }

    /// Resolve the LoLalytics lane for a build/counter query. When the LCU
    /// position is missing or unmapped we ask the `counter` endpoint (cached)
    /// for the champion's own default lane so item/counter data stays on-role.
    async fn resolve_lane(&self, slug: &str, position: &str) -> String {
        if let Some(lane) = lol_lane(position) {
            return lane.to_string();
        }
        if let Ok(counter) = self.api.get_counter(slug, "middle").await {
            let default_lane = counter.stats.default_lane.clone();
            if !default_lane.is_empty() {
                return default_lane;
            }
        }
        "middle".to_string()
    }
}

#[async_trait]
impl BuildProvider for LolalyticsProvider {
    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
        let id = self
            .champion_id(&snapshot.self_raw_name)
            .await
            .ok_or_else(|| {
                ProviderError::Other(format!("unknown champion: {:?}", snapshot.self_champion))
            })?;
        let slug = self.champion_slug(id).await?;
        let lane = self.resolve_lane(&slug, &snapshot.self_position).await;

        let itemset = self.api.get_itemset(&slug, &lane).await?;
        // Starting items are a nice-to-have: never fail the whole panel for them.
        let earlyset = self.api.get_earlyset(&slug, &lane).await.ok();

        let mut build_ids: Vec<i64> = Vec::new();
        if let Some(EarlyStat(combo, ..)) = earlyset.as_ref().and_then(|e| e.early_set.first()) {
            build_ids.extend(parse_item_combo(combo));
        }
        build_ids.extend(
            itemset
                .item_sets
                .item_set1
                .iter()
                .filter_map(|s| s.0.parse::<i64>().ok())
                .take(6),
        );
        if let Some(boot) = itemset.item_sets.item_boot_set1.first() {
            if let Ok(boot_id) = boot.0.parse::<i64>() {
                build_ids.push(boot_id);
            }
        }

        // The itemset rows carry popularity counts, not wins, so the headline
        // win rate comes from the champion's lane header (`counter`), which is
        // cached and often already warmed by lane resolution. Sample size is
        // the most-picked core item's game count.
        let win_rate = self
            .api
            .get_counter(&slug, &lane)
            .await
            .ok()
            .map_or(0.0, |c| c.stats.wr);
        let games = itemset.item_sets.item_set1.first().map_or(0, |s| s.1);

        let items = self
            .ddragon
            .items()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        let recs = item_recommendations(
            build_ids,
            |item_id| {
                items
                    .get(&item_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Item {item_id}"))
            },
            win_rate,
            games,
        );
        if recs.is_empty() {
            return Err(ProviderError::Other("build had no items".into()));
        }
        Ok(recs)
    }

    /// Not available: see the module docs. LoLalytics gates rune data behind
    /// server-rendered HTML only.
    async fn runes(&self, _champion_id: i64, _role: Option<&str>) -> Result<RuneRecommendation> {
        Err(ProviderError::NotEnoughData)
    }

    async fn tier_list(&self, role: &str) -> Result<Vec<TierEntry>> {
        let lane = lol_lane(role)
            .ok_or_else(|| ProviderError::Other(format!("unknown role: {role:?}")))?;
        let tier = self.api.get_tier().await?;
        let rows = tier_entries(&tier, lane);
        if rows.is_empty() {
            return Err(ProviderError::NotEnoughData);
        }
        Ok(rows)
    }

    async fn counters(&self, champion_id: i64, role: &str) -> Result<Vec<CounterEntry>> {
        let slug = self.champion_slug(champion_id).await?;
        let lane = self.resolve_lane(&slug, role).await;

        let counter = self.api.get_counter(&slug, &lane).await?;
        let entries = counter_entries(&counter.counters);
        if !entries.is_empty() {
            return Ok(entries);
        }

        // The requested lane may be off-role for this champion (few/no matchup
        // games). Fall back to the champion's own default lane once.
        let default_lane = counter.stats.default_lane.clone();
        if default_lane.is_empty() || default_lane == lane {
            return Ok(entries);
        }
        let fallback = self.api.get_counter(&slug, &default_lane).await?;
        Ok(counter_entries(&fallback.counters))
    }

    async fn champion_names(&self, champion_id: i64) -> Option<(String, String)> {
        let champs = self.ddragon.champions().await.ok()?;
        Some((
            champs.id_to_name.get(&champion_id)?.clone(),
            champs.id_to_image.get(&champion_id)?.clone(),
        ))
    }
}

/// LCU/Live-Client position string → LoLalytics lane. `None` for unknown /
/// unmapped positions (the caller decides whether to fall back).
fn lol_lane(position: &str) -> Option<&'static str> {
    Some(match position.to_ascii_lowercase().as_str() {
        "top" => "top",
        "jungle" => "jungle",
        "middle" | "mid" => "middle",
        "bottom" | "bot" | "adc" => "bottom",
        "utility" | "support" | "supporter" => "support",
        _ => return None,
    })
}

/// Split an underscore-joined item combo (`"1055_1037_2021"`) into item ids,
/// dropping any non-numeric fragment.
fn parse_item_combo(combo: &str) -> Vec<i64> {
    combo
        .split('_')
        .filter_map(|p| p.parse::<i64>().ok())
        .collect()
}

/// Flatten every champion bucket of the `tier` payload for one lane into sorted
/// [`TierEntry`] rows. A champion can appear once per lane; if it somehow
/// repeats we keep the row with the most games. Win-rate delta is left at 0.0
/// (the 30-day aggregate has no previous-patch baseline to diff against).
fn tier_entries(tier: &types::TierResponse, lane: &str) -> Vec<TierEntry> {
    let mut by_champion: HashMap<i64, TierEntry> = HashMap::new();
    for group in tier.tier.values() {
        let Some(lane_data) = group.lane.get(lane) else {
            continue;
        };
        for (cid, champ) in &lane_data.cid {
            let Ok(champion_id) = cid.parse::<i64>() else {
                continue;
            };
            let pick_rate = champ.pr / 100.0;
            if pick_rate < MIN_TIER_PICK_RATE {
                continue;
            }
            let entry = TierEntry {
                champion_id,
                win_rate: champ.wr / 100.0,
                win_rate_delta: 0.0,
                games: champ.games,
                pick_rate,
                ban_rate: champ.br / 100.0,
            };
            by_champion
                .entry(champion_id)
                .and_modify(|existing| {
                    if champ.games > existing.games {
                        *existing = entry.clone();
                    }
                })
                .or_insert(entry);
        }
    }
    let mut rows: Vec<TierEntry> = by_champion.into_values().collect();
    rows.sort_by(|a, b| b.win_rate.total_cmp(&a.win_rate));
    rows
}

/// LoLalytics `counters` rows list the counter champion's win rate against the
/// subject (`vsWr`, 0–100). Convert to the shared contract, whose helper wants
/// the *subject's* win rate and re-inverts it, then filters thin matchups and
/// keeps the top 8.
fn counter_entries(rows: &[types::CounterRow]) -> Vec<CounterEntry> {
    counter_entries_from_subject_losses(rows.iter().map(|r| (r.cid, 1.0 - r.vs_wr / 100.0, r.n)))
}

#[cfg(test)]
mod tests;
