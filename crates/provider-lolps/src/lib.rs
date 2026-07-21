//! LOL.PS build provider for the site's anonymous JSON endpoints.
//!
//! The population is deliberately fixed to Korea, Emerald+, latest patch.
//! Only the lane follows the current League position. LOL.PS is build-only;
//! it does not implement player-history APIs or authenticated matchup builds.

mod api;
mod types;

use std::fmt::Write as _;
use std::sync::Arc;

use async_trait::async_trait;
use overlay_ddragon::{normalize, DdragonClient};
use overlay_provider::{
    counter_entries_from_subject_losses, item_recommendations, rune_recommendation, BuildProvider,
    CounterEntry, ItemRecommendation, ProviderError, Result, RuneBuild, RuneRecommendation,
    SkillOrder,
};
use overlay_types::{DataProvenance, GameSnapshot, TierEntry};

use crate::api::LolpsApi;
use crate::types::{Selected, SummaryRow, TierRow};

const PROVIDER_ID: &str = "lolps";
const PROVIDER_LABEL: &str = "LOL.PS";

pub struct LolpsProvider {
    ddragon: Arc<DdragonClient>,
    api: LolpsApi,
}

impl LolpsProvider {
    pub fn new(ddragon: Arc<DdragonClient>) -> Result<Self> {
        Ok(Self {
            ddragon,
            api: LolpsApi::new()?,
        })
    }

    async fn champion_id(&self, raw_name: &str) -> Result<i64> {
        let champions = self
            .ddragon
            .champions()
            .await
            .map_err(|error| ProviderError::Other(error.to_string()))?;
        champions
            .name_to_id
            .get(&normalize(raw_name))
            .copied()
            .ok_or_else(|| ProviderError::Other(format!("unknown champion: {raw_name}")))
    }
}

#[async_trait]
impl BuildProvider for LolpsProvider {
    fn set_platform_id(&self, _platform_id: &str) {
        // Intentional: this adapter always queries the KR population.
    }

    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
        let lane = optional_lane_id(Some(&snapshot.self_position))?;
        let champion_id = self.champion_id(&snapshot.self_raw_name).await?;
        let selected = self.api.summary(champion_id, lane).await?;
        let item_names = self
            .ddragon
            .items()
            .await
            .map_err(|error| ProviderError::Other(error.to_string()))?;
        let ids = item_ids(&selected.value);
        if ids.is_empty() {
            return Err(ProviderError::NotEnoughData);
        }
        let (win_rate, games, stats_label) = match (
            selected.value.top1_three_core_winrate.as_deref(),
            selected.value.top1_three_core_count,
        ) {
            (Some(win_rate), Some(games)) if games > 0 => (
                parse_percent(win_rate, "item winRate")?,
                games,
                "3-core build",
            ),
            _ => (
                parse_percent(&selected.value.win_rate, "champion winRate")?,
                selected.value.count,
                "champion stats",
            ),
        };
        let mut items = item_recommendations(
            ids,
            |id| {
                item_names
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| format!("Item {id}"))
            },
            win_rate * 100.0,
            games,
        );
        apply_item_reasons(
            &mut items,
            &selected.value,
            win_rate,
            games,
            stats_label,
            selected.fallback_from.as_deref(),
        );
        Ok(items)
    }

    async fn skill_order(&self, snapshot: &GameSnapshot) -> Result<SkillOrder> {
        let lane = optional_lane_id(Some(&snapshot.self_position))?;
        let champion_id = self.champion_id(&snapshot.self_raw_name).await?;
        let selected = self.api.summary(champion_id, lane).await?;
        skill_order(&selected.value)
    }

    async fn runes(&self, champion_id: i64, role: Option<&str>) -> Result<RuneRecommendation> {
        let build = self.rune_build(champion_id, role, None).await?;
        Ok(rune_recommendation(PROVIDER_LABEL, build))
    }

    async fn tier_list(&self, role: &str) -> Result<Vec<TierEntry>> {
        let lane_id = required_lane_id(role)?;
        let selected = self.api.tier_list(lane_id).await?;
        let entries = selected
            .value
            .data
            .iter()
            .filter(|row| row.lane_id == lane_id && row.count > 0)
            .map(|row| tier_entry(row, &selected))
            .collect::<Result<Vec<_>>>()?;
        if entries.is_empty() {
            return Err(ProviderError::NotEnoughData);
        }
        Ok(entries)
    }

    async fn counters(&self, champion_id: i64, role: &str) -> Result<Vec<CounterEntry>> {
        let lane_id = required_lane_id(role)?;
        let selected = self.api.summary(champion_id, lane_id).await?;
        let counters = counter_entries(&selected.value)?;
        if counters.is_empty() {
            return Err(ProviderError::NotEnoughData);
        }
        Ok(counters)
    }

    async fn rune_build(
        &self,
        champion_id: i64,
        role: Option<&str>,
        enemy_champion_id: Option<i64>,
    ) -> Result<RuneBuild> {
        if enemy_champion_id.is_some() {
            return Err(ProviderError::NotEnoughData);
        }
        let lane = optional_lane_id(role)?;
        let selected = self.api.summary(champion_id, lane).await?;
        rune_build(&selected, role_label(lane))
    }

    async fn champion_names(&self, champion_id: i64) -> Option<(String, String)> {
        let champions = self.ddragon.champions().await.ok()?;
        Some((
            champions.id_to_name.get(&champion_id)?.clone(),
            champions.id_to_image.get(&champion_id)?.clone(),
        ))
    }
}

fn lane_id(role: &str) -> Option<i64> {
    match role.to_ascii_lowercase().as_str() {
        "top" => Some(0),
        "jungle" => Some(1),
        "middle" | "mid" => Some(2),
        "bottom" | "bot" | "adc" => Some(3),
        "utility" | "support" | "supporter" => Some(4),
        _ => None,
    }
}

fn optional_lane_id(role: Option<&str>) -> Result<i64> {
    match role {
        None | Some("") => Ok(-1),
        Some(role) => required_lane_id(role),
    }
}

fn required_lane_id(role: &str) -> Result<i64> {
    lane_id(role).ok_or_else(|| ProviderError::Other(format!("unknown role: {role:?}")))
}

fn role_label(lane_id: i64) -> &'static str {
    match lane_id {
        0 => "Top",
        1 => "Jungle",
        2 => "Middle",
        3 => "Bottom",
        4 => "Support",
        _ => "Primary role",
    }
}

fn parse_percent(value: &str, field: &str) -> Result<f64> {
    let percent = value
        .parse::<f64>()
        .map_err(|_| ProviderError::InvalidData(format!("LOL.PS {field} is invalid: {value:?}")))?;
    if !percent.is_finite() || !(0.0..=100.0).contains(&percent) {
        return Err(ProviderError::InvalidData(format!(
            "LOL.PS {field} must be within 0..=100: {value:?}"
        )));
    }
    Ok(percent / 100.0)
}

fn item_ids(row: &SummaryRow) -> Vec<i64> {
    row.starting_item_id_list
        .iter()
        .flatten()
        .copied()
        .chain(row.core_item_id_list.iter().copied())
        .chain(row.shoes_id)
        .collect()
}

fn apply_item_reasons(
    items: &mut [ItemRecommendation],
    row: &SummaryRow,
    win_rate: f64,
    games: i64,
    stats_label: &str,
    fallback_from: Option<&str>,
) {
    for item in &mut *items {
        item.reason = if row
            .starting_item_id_list
            .iter()
            .flatten()
            .any(|id| *id == item.item_id)
        {
            "Starting item".into()
        } else if row.shoes_id == Some(item.item_id) {
            "Boots".into()
        } else {
            "Core item".into()
        };
    }
    if let Some(first) = items.first_mut() {
        write!(
            first.reason,
            " · {stats_label} {:.0}% WR · {games} games",
            win_rate * 100.0
        )
        .expect("writing to String cannot fail");
        if let Some(from) = fallback_from {
            write!(first.reason, " · fallback from {from}").expect("writing to String cannot fail");
        }
    }
}

fn skill_id(value: &str) -> Option<i64> {
    match value.to_ascii_uppercase().as_str() {
        "Q" => Some(1),
        "W" => Some(2),
        "E" => Some(3),
        "R" => Some(4),
        _ => None,
    }
}

fn skill_order(row: &SummaryRow) -> Result<SkillOrder> {
    let max_order = row
        .skill_master_list
        .iter()
        .map(|skill| skill_id(skill).ok_or_else(|| invalid_skill(skill)))
        .collect::<Result<Vec<_>>>()?;
    let level_order = row
        .skill_lv15_list
        .iter()
        .map(|skill| skill_id(skill).ok_or_else(|| invalid_skill(skill)))
        .collect::<Result<Vec<_>>>()?;
    if max_order.len() != 3 || level_order.len() != 15 || row.skill_master_count <= 0 {
        return Err(ProviderError::NotEnoughData);
    }
    Ok(SkillOrder {
        max_order,
        level_order,
        win_rate: parse_percent(&row.skill_master_winrate, "skill winRate")?,
        games: row.skill_master_count,
    })
}

fn invalid_skill(skill: &str) -> ProviderError {
    ProviderError::InvalidData(format!("LOL.PS returned unknown skill {skill}"))
}

fn rune_build(selected: &Selected<SummaryRow>, lane: &str) -> Result<RuneBuild> {
    let row = &selected.value;
    if row.rune_total_count <= 0 {
        return Err(ProviderError::NotEnoughData);
    }
    let required = [
        row.main_rune_category,
        row.sub_rune_category,
        row.main_rune1,
        row.main_rune2,
        row.main_rune3,
        row.main_rune4,
        row.sub_rune1,
        row.sub_rune2,
        row.statperk1_id,
        row.statperk2_id,
        row.statperk3_id,
        row.spell1_id,
        row.spell2_id,
    ]
    .into_iter()
    .collect::<Option<Vec<_>>>()
    .ok_or(ProviderError::NotEnoughData)?;
    let fallback = selected
        .fallback_from
        .as_ref()
        .map_or(String::new(), |from| format!(" · fallback from {from}"));
    Ok(RuneBuild {
        page_name: format!(
            "LOL.PS {} {}{}",
            selected.version.description, lane, fallback
        ),
        lane: lane.to_string(),
        win_rate: parse_percent(&row.rune_total_winrate, "rune winRate")?,
        games: row.rune_total_count,
        primary_style_id: required[0],
        sub_style_id: required[1],
        primary_perk_ids: required[2..6].to_vec(),
        sub_perk_ids: required[6..8].to_vec(),
        shard_ids: required[8..11].to_vec(),
        spell_ids: required[11..13].to_vec(),
        matchup: false,
    })
}

fn counter_entries(row: &SummaryRow) -> Result<Vec<CounterEntry>> {
    if row.counter_champion_id_list.len() != row.counter_winrate_list.len()
        || row.counter_champion_id_list.len() != row.counter_count_list.len()
    {
        return Err(ProviderError::InvalidData(
            "LOL.PS counter arrays have different lengths".into(),
        ));
    }
    let rows = row
        .counter_champion_id_list
        .iter()
        .copied()
        .zip(row.counter_winrate_list.iter().copied())
        .zip(row.counter_count_list.iter().copied())
        .map(|((champion_id, subject_win_rate), games)| {
            if !subject_win_rate.is_finite() || !(0.0..=100.0).contains(&subject_win_rate) {
                return Err(ProviderError::InvalidData(
                    "LOL.PS counter winRate is invalid".into(),
                ));
            }
            Ok((champion_id, subject_win_rate / 100.0, games))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(counter_entries_from_subject_losses(rows))
}

fn tier_entry(
    row: &TierRow,
    selected: &Selected<Arc<crate::types::TierResponse>>,
) -> Result<TierEntry> {
    let mut provenance = DataProvenance::now(PROVIDER_ID);
    provenance.region = Some("KR".into());
    provenance.patch = Some(selected.version.description.clone());
    provenance.rank = Some("Emerald+".into());
    provenance.fallback_from.clone_from(&selected.fallback_from);
    Ok(TierEntry {
        champion_id: row.champion_id,
        win_rate: parse_percent(&row.win_rate, "tier winRate")?,
        win_rate_delta: None,
        games: Some(row.count),
        pick_rate: parse_percent(&row.pick_rate, "tier pickRate")?,
        ban_rate: parse_percent(&row.ban_rate, "tier banRate")?,
        provenance,
    })
}

#[cfg(test)]
mod tests;
