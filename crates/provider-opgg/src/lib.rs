//! op.gg stats provider implementing [`BuildProvider`].
//!
//! op.gg publishes no JSON API of its own; every value here is recovered from
//! the flight (React Server Component) payload embedded in the site's
//! server-rendered HTML — see the [`flight`] module docs for why, and
//! [`api`] for how the page types (`/build[/lane]`, `/counters[/lane]`,
//! `/lol/champions?position=<lane>`) are fetched and parsed.
//!
//! * **Items and summoner spells** come from the rendered element tree of
//!   the build page (no clean data prop exists for them).
//! * **Skill order** comes from a clean `skill_masteries` data prop, but on
//!   its own dedicated route (`/skills[/lane]`) rather than the build page —
//!   the build page renders the same table straight to elements (like items)
//!   with no clean prop, so fetching `/skills` separately trades one extra
//!   request for a real level-by-level order instead of just the 3-letter
//!   max-priority summary.
//! * **Runes** come from a clean `rune_pages` data prop on the build page —
//!   op.gg is the only one of this codebase's providers with rune data this
//!   complete, so [`runes`] and [`rune_build`] are fully supported,
//!   **including matchup-specific pages**: adding `?target_champion=<slug>`
//!   to the build page scopes every number on it (items included) to that
//!   specific matchup.
//! * **Counters** come from a clean `data` prop on the counters page.
//! * **Tier list** comes from a clean `data` prop too, but on a different
//!   route: `/lol/champions?position=<lane>` (one fetch per lane — op.gg has
//!   no combined "all lanes" list). This isn't the page's default HTML
//!   response (that ships a small unrelated "trending" preview instead); the
//!   `position` query param is what makes the server render the full,
//!   per-lane flight payload.
//!
//! [`runes`]: OpggProvider::runes
//! [`rune_build`]: OpggProvider::rune_build

mod api;
mod flight;
mod types;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use overlay_ddragon::{normalize, DdragonClient};
use overlay_provider::{
    counter_entries_from_subject_losses, item_recommendations, rune_recommendation, BuildProvider,
    CounterEntry, ItemRecommendation, ProviderError, Result, RuneRecommendation,
};
use overlay_types::{GameSnapshot, RuneBuild, SkillOrder, TierEntry};

use crate::api::{BuildPage, OpggApi};
use crate::types::{CounterRow, Perk, SkillMastery, TierRow};

/// LCU/Live-Client skill ids follow Riot's convention: 1=Q, 2=W, 3=E, 4=R.
fn skill_letter_id(letter: &str) -> Option<i64> {
    match letter {
        "Q" => Some(1),
        "W" => Some(2),
        "E" => Some(3),
        "R" => Some(4),
        _ => None,
    }
}

pub struct OpggProvider {
    ddragon: Arc<DdragonClient>,
    api: OpggApi,
}

impl OpggProvider {
    pub fn new(ddragon: Arc<DdragonClient>) -> Result<Self> {
        Ok(Self {
            ddragon,
            api: OpggApi::new()?,
        })
    }

    async fn champion_id(&self, raw_name: &str) -> Option<i64> {
        let champs = self.ddragon.champions().await.ok()?;
        champs.name_to_id.get(&normalize(raw_name)).copied()
    }

    /// op.gg champion slug — the Data Dragon alias, lowercased
    /// (`MonkeyKing` → `monkeyking`), same convention LoLalytics uses.
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

    async fn champion_display_name(&self, champion_id: i64) -> String {
        self.ddragon
            .champions()
            .await
            .ok()
            .and_then(|c| c.id_to_name.get(&champion_id).cloned())
            .unwrap_or_else(|| champion_id.to_string())
    }
}

#[async_trait]
impl BuildProvider for OpggProvider {
    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
        let id = self
            .champion_id(&snapshot.self_raw_name)
            .await
            .ok_or_else(|| {
                ProviderError::Other(format!("unknown champion: {:?}", snapshot.self_champion))
            })?;
        let slug = self.champion_slug(id).await?;
        let lane = opgg_lane(&snapshot.self_position);
        let page = self.api.get_build_page(&slug, lane, None).await?;

        let mut build_ids = Vec::new();
        build_ids.extend(page.starter_items.iter().copied());
        build_ids.extend(page.core_items.iter().take(6).copied());
        build_ids.extend(page.boots.iter().take(1).copied());

        let (win_rate, games) = top_rune_page_stats(&page);

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

    async fn skill_order(&self, snapshot: &GameSnapshot) -> Result<SkillOrder> {
        let id = self
            .champion_id(&snapshot.self_raw_name)
            .await
            .ok_or_else(|| {
                ProviderError::Other(format!("unknown champion: {:?}", snapshot.self_champion))
            })?;
        let slug = self.champion_slug(id).await?;
        let lane = opgg_lane(&snapshot.self_position);
        let masteries = self.api.get_skills(&slug, lane).await?;
        skill_order_from_masteries(&masteries).ok_or(ProviderError::NotEnoughData)
    }

    async fn runes(&self, champion_id: i64, role: Option<&str>) -> Result<RuneRecommendation> {
        Ok(rune_recommendation(
            "OP.GG",
            self.rune_build(champion_id, role, None).await?,
        ))
    }

    async fn counters(&self, champion_id: i64, role: &str) -> Result<Vec<CounterEntry>> {
        let slug = self.champion_slug(champion_id).await?;
        let lane = opgg_lane(role);
        let rows = self.api.get_counters(&slug, lane).await?;
        let champs = self
            .ddragon
            .champions()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        Ok(counter_entries(&rows, &champs.name_to_id))
    }

    async fn tier_list(&self, role: &str) -> Result<Vec<TierEntry>> {
        let lane = opgg_lane(role)
            .ok_or_else(|| ProviderError::Other(format!("unknown role: {role:?}")))?;
        let rows = self.api.get_tier_list(lane).await?;
        let champs = self
            .ddragon
            .champions()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        let entries = tier_entries(&rows, &champs.name_to_id);
        if entries.is_empty() {
            return Err(ProviderError::NotEnoughData);
        }
        Ok(entries)
    }

    async fn rune_build(
        &self,
        champion_id: i64,
        role: Option<&str>,
        enemy_champion_id: Option<i64>,
    ) -> Result<RuneBuild> {
        let slug = self.champion_slug(champion_id).await?;
        let lane = role.and_then(opgg_lane);

        // `?target_champion=<slug>` scopes every number on the build page to
        // that specific matchup (confirmed: sample sizes and win rates both
        // shrink/shift relative to the champion-wide page) — same page, same
        // parser, so the only difference from the solo path is this query
        // param and the resulting `matchup` flag.
        let enemy_slug = match enemy_champion_id {
            Some(enemy_id) => Some(self.champion_slug(enemy_id).await?),
            None => None,
        };
        let page = self
            .api
            .get_build_page(&slug, lane, enemy_slug.as_deref())
            .await?;

        let rune_page = page.runes.first().ok_or(ProviderError::NotEnoughData)?;
        let build = rune_page
            .builds
            .first()
            .ok_or(ProviderError::NotEnoughData)?;

        let primary_perk_ids = active_perk_ids(&build.main_runes);
        let sub_perk_ids = active_perk_ids(&build.sub_runes);
        let shard_ids = active_perk_ids(&build.shards);
        if primary_perk_ids.len() != 4 || sub_perk_ids.len() != 2 || shard_ids.len() != 3 {
            return Err(ProviderError::Other("incomplete rune data".into()));
        }

        let name = self.champion_display_name(champion_id).await;
        let lane_label = lane.unwrap_or("default").to_string();
        let page_name = match enemy_champion_id {
            Some(enemy_id) => {
                let foe = self.champion_display_name(enemy_id).await;
                format!("OP.GG {name} vs {foe} {lane_label}")
            }
            None => format!("OP.GG {name} {lane_label}"),
        };
        Ok(RuneBuild {
            page_name,
            lane: lane_label,
            win_rate: rune_page.win_rate,
            games: rune_page.play,
            primary_style_id: build.primary_perk_style.id,
            sub_style_id: build.perk_sub_style.id,
            primary_perk_ids,
            sub_perk_ids,
            shard_ids,
            spell_ids: page.spell_ids.clone(),
            matchup: enemy_champion_id.is_some(),
        })
    }

    async fn champion_names(&self, champion_id: i64) -> Option<(String, String)> {
        let champs = self.ddragon.champions().await.ok()?;
        Some((
            champs.id_to_name.get(&champion_id)?.clone(),
            champs.id_to_image.get(&champion_id)?.clone(),
        ))
    }
}

/// The site's headline win rate/games for the champion, taken from its
/// top-recommended rune page (the build page has no separate "champion
/// overview" data prop — see [`api`] module docs). `0.0`/`0` if unknown.
/// Win rate is 0..100.
fn top_rune_page_stats(page: &BuildPage) -> (f64, i64) {
    page.runes
        .first()
        .map(|rp| (rp.win_rate * 100.0, rp.play))
        .unwrap_or((0.0, 0))
}

/// Flatten a perk table's rows and keep only the active (recommended) ids, in
/// row order. Every rune/shard table op.gg ships has exactly one active
/// candidate per row it wants selected (all 4 main-tree rows, 2 of the 3
/// secondary-tree rows, all 3 shard rows), so this recovers `[keystone, p1,
/// p2, p3]` / `[s1, s2]` / `[offense, flex, defense]` uniformly.
fn active_perk_ids(rows: &[Vec<Perk>]) -> Vec<i64> {
    rows.iter()
        .flatten()
        .filter(|perk| perk.is_active)
        .map(|perk| perk.id)
        .collect()
}

/// Map op.gg's slug-keyed matchup rows to the shared [`CounterEntry`]
/// contract. `win_rate` on each row is already the subject's own win rate in
/// that matchup (see [`crate::types::CounterRow`]), so it's passed straight
/// through to the shared helper without inverting it here.
fn counter_entries(rows: &[CounterRow], slug_to_id: &HashMap<String, i64>) -> Vec<CounterEntry> {
    counter_entries_from_subject_losses(rows.iter().filter_map(|row| {
        let champion_id = slug_to_id.get(&normalize(&row.champion.key)).copied()?;
        Some((champion_id, row.win_rate / 100.0, row.play))
    }))
}

/// Map one lane's op.gg tier rows to the shared [`TierEntry`] contract,
/// sorted by win rate descending. `games` is left at 0 (unknown) and
/// `win_rate_delta` at 0.0 — op.gg's tier rows carry a `rank_prev` (previous
/// patch's rank) but no previous win rate to diff against, and no raw sample
/// count at all, only percentages.
fn tier_entries(rows: &[TierRow], slug_to_id: &HashMap<String, i64>) -> Vec<TierEntry> {
    let mut entries: Vec<TierEntry> = rows
        .iter()
        .filter_map(|row| {
            let champion_id = slug_to_id.get(&normalize(&row.key)).copied()?;
            Some(TierEntry {
                champion_id,
                win_rate: row.win_rate / 100.0,
                win_rate_delta: 0.0,
                games: 0,
                pick_rate: row.pick_rate / 100.0,
                ban_rate: row.ban_rate / 100.0,
            })
        })
        .collect();
    entries.sort_by(|a, b| b.win_rate.total_cmp(&a.win_rate));
    entries
}

/// Convert the top-recommended [`SkillMastery`] into a [`SkillOrder`].
/// `None` when there's no mastery data or its max-priority letters don't map
/// to any known skill (both signal "nothing usable here" to the caller).
fn skill_order_from_masteries(masteries: &[SkillMastery]) -> Option<SkillOrder> {
    let mastery = masteries.first()?;
    let build = mastery.builds.first()?;
    let max_order: Vec<i64> = mastery
        .ids
        .iter()
        .filter_map(|l| skill_letter_id(l))
        .collect();
    if max_order.is_empty() {
        return None;
    }
    let level_order: Vec<i64> = build
        .order
        .iter()
        .filter_map(|l| skill_letter_id(l))
        .collect();
    Some(SkillOrder {
        max_order,
        level_order,
        win_rate: build.win_rate,
        games: build.play,
    })
}

/// LCU/Live-Client position string → op.gg lane path segment. `None` for
/// unknown/unmapped positions — callers omit the segment and let op.gg's own
/// server pick the champion's default lane.
fn opgg_lane(position: &str) -> Option<&'static str> {
    Some(match position.to_ascii_lowercase().as_str() {
        "top" => "top",
        "jungle" => "jungle",
        "middle" | "mid" => "mid",
        "bottom" | "bot" | "adc" => "adc",
        "utility" | "support" | "supporter" => "support",
        _ => return None,
    })
}

#[cfg(test)]
mod tests;
