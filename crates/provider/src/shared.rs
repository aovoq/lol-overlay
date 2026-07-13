use std::collections::HashSet;

use overlay_types::{
    CounterEntry, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder, TierEntry,
};

use crate::error::{ProviderError, Result};

pub const MIN_MATCHUP_GAMES: i64 = 30;

fn rate(value: f64, field: &str) -> Result<()> {
    if value.is_finite() && (0.0..=1.0).contains(&value) {
        Ok(())
    } else {
        Err(ProviderError::InvalidData(format!(
            "{field} must be finite and within 0..=1"
        )))
    }
}

pub fn normalize_items(items: Vec<ItemRecommendation>) -> Result<Vec<ItemRecommendation>> {
    let mut seen = HashSet::new();
    for item in &items {
        if item.item_id <= 0 || !item.score.is_finite() || !(0.0..=1.0).contains(&item.score) {
            return Err(ProviderError::InvalidData(
                "invalid item recommendation".into(),
            ));
        }
        if !seen.insert(item.item_id) {
            return Err(ProviderError::InvalidData(format!(
                "duplicate item {}",
                item.item_id
            )));
        }
    }
    Ok(items)
}

pub fn normalize_skill_order(order: SkillOrder) -> Result<SkillOrder> {
    rate(order.win_rate, "skill win_rate")?;
    if order.games < 0
        || order.max_order.iter().any(|skill| !(1..=3).contains(skill))
        || order
            .level_order
            .iter()
            .any(|skill| !(1..=4).contains(skill))
    {
        return Err(ProviderError::InvalidData("invalid skill order".into()));
    }
    Ok(order)
}

pub fn normalize_rune_recommendation(runes: RuneRecommendation) -> Result<RuneRecommendation> {
    if runes.primary_style_id <= 0
        || runes.sub_style_id <= 0
        || runes.selected_perk_ids.len() < 6
        || runes.selected_perk_ids.iter().any(|id| *id <= 0)
    {
        return Err(ProviderError::InvalidData(
            "invalid rune page structure".into(),
        ));
    }
    Ok(runes)
}

pub fn normalize_rune_build(runes: RuneBuild) -> Result<RuneBuild> {
    rate(runes.win_rate, "rune win_rate")?;
    if runes.games < 0
        || runes.primary_style_id <= 0
        || runes.sub_style_id <= 0
        || runes.primary_perk_ids.len() != 4
        || runes.sub_perk_ids.len() != 2
        || runes.shard_ids.len() != 3
        || (!runes.spell_ids.is_empty() && runes.spell_ids.len() != 2)
        || runes
            .primary_perk_ids
            .iter()
            .chain(&runes.sub_perk_ids)
            .chain(&runes.shard_ids)
            .chain(&runes.spell_ids)
            .any(|id| *id <= 0)
    {
        return Err(ProviderError::InvalidData(
            "invalid rune build structure".into(),
        ));
    }
    Ok(runes)
}

pub fn normalize_tier_entries(mut entries: Vec<TierEntry>) -> Result<Vec<TierEntry>> {
    let mut seen = HashSet::new();
    for entry in &entries {
        rate(entry.win_rate, "tier win_rate")?;
        rate(entry.pick_rate, "tier pick_rate")?;
        rate(entry.ban_rate, "tier ban_rate")?;
        if entry.champion_id <= 0
            || entry.games.is_some_and(|games| games < 0)
            || entry.win_rate_delta.is_some_and(|delta| !delta.is_finite())
            || entry.provenance.provider.is_empty()
            || entry.provenance.fetched_at == 0
            || !seen.insert(entry.champion_id)
        {
            return Err(ProviderError::InvalidData("invalid tier entry".into()));
        }
    }
    entries.sort_by(|a, b| b.win_rate.total_cmp(&a.win_rate));
    Ok(entries)
}

pub fn normalize_counter_entries(mut entries: Vec<CounterEntry>) -> Result<Vec<CounterEntry>> {
    let mut seen = HashSet::new();
    for entry in &entries {
        rate(entry.win_rate, "counter win_rate")?;
        if entry.champion_id <= 0
            || entry.games < MIN_MATCHUP_GAMES
            || !seen.insert(entry.champion_id)
        {
            return Err(ProviderError::InvalidData("invalid counter entry".into()));
        }
    }
    entries.sort_by(|a, b| b.win_rate.total_cmp(&a.win_rate));
    entries.truncate(8);
    Ok(entries)
}

#[must_use]
pub fn item_recommendations(
    item_ids: impl IntoIterator<Item = i64>,
    item_name: impl Fn(i64) -> String,
    win_rate_percent: f64,
    games: i64,
) -> Vec<ItemRecommendation> {
    let mut seen = HashSet::new();
    item_ids
        .into_iter()
        .enumerate()
        .filter_map(|(index, item_id)| {
            if item_id == 0 || !seen.insert(item_id) {
                return None;
            }
            let reason = if index == 0 {
                format!("Core build · {win_rate_percent:.0}% WR · {games} games")
            } else {
                "Core build".to_string()
            };
            Some(ItemRecommendation {
                item_id,
                name: item_name(item_id),
                score: (1.0 - index as f32 * 0.08).max(0.2),
                reason,
            })
        })
        .collect()
}

#[must_use]
pub fn counter_entries_from_subject_losses(
    rows: impl IntoIterator<Item = (i64, f64, i64)>,
) -> Vec<CounterEntry> {
    let mut entries: Vec<CounterEntry> = rows
        .into_iter()
        .filter(|(_, _, games)| *games >= MIN_MATCHUP_GAMES)
        .map(|(champion_id, subject_win_rate, games)| CounterEntry {
            champion_id,
            win_rate: 1.0 - subject_win_rate,
            games,
        })
        .collect();
    entries.sort_by(|a, b| b.win_rate.total_cmp(&a.win_rate));
    entries.truncate(8);
    entries
}

#[must_use]
pub fn rune_recommendation(source: &str, build: RuneBuild) -> RuneRecommendation {
    let mut selected_perk_ids = build.primary_perk_ids;
    selected_perk_ids.extend(build.sub_perk_ids);
    selected_perk_ids.extend(build.shard_ids);
    RuneRecommendation {
        name: format!(
            "{source} {} ({:.0}% WR)",
            build.lane,
            build.win_rate * 100.0
        ),
        primary_style_id: build.primary_style_id,
        sub_style_id: build.sub_style_id,
        selected_perk_ids,
    }
}

#[must_use]
pub fn split_primary_secondary_runes(rune_ids: &[i64]) -> (Vec<i64>, Vec<i64>) {
    if rune_ids.len() >= 6 {
        (rune_ids[..4].to_vec(), rune_ids[4..6].to_vec())
    } else if rune_ids.len() >= 4 {
        (rune_ids[..4].to_vec(), rune_ids[4..].to_vec())
    } else {
        (rune_ids.to_vec(), Vec::new())
    }
}

/// Generate the provider-independent normalization checks for a provider
/// fixture. A new adapter can opt into the same CI contract with one macro call.
#[macro_export]
macro_rules! build_provider_contract_suite {
    ($name:ident, $provider:literal) => {
        #[test]
        fn $name() {
            let tier = vec![overlay_types::TierEntry {
                champion_id: 1,
                win_rate: 0.51,
                win_rate_delta: None,
                games: None,
                pick_rate: 0.1,
                ban_rate: 0.1,
                provenance: overlay_types::recommendation::DataProvenance::now($provider),
            }];
            let counters = vec![overlay_types::CounterEntry {
                champion_id: 2,
                win_rate: 0.52,
                games: $crate::MIN_MATCHUP_GAMES,
            }];
            $crate::normalize_tier_entries(tier).expect("tier contract");
            $crate::normalize_counter_entries(counters).expect("counter contract");
        }
    };
}

#[cfg(test)]
mod tests {
    use overlay_types::{DataProvenance, TierEntry};

    use super::*;

    fn provenance() -> DataProvenance {
        DataProvenance::now("fixture")
    }

    #[test]
    fn rejects_non_finite_rates_and_duplicate_items() {
        let duplicate = vec![
            ItemRecommendation {
                item_id: 1001,
                name: "Boots".into(),
                score: 1.0,
                reason: "fixture".into(),
            },
            ItemRecommendation {
                item_id: 1001,
                name: "Boots".into(),
                score: 0.8,
                reason: "fixture".into(),
            },
        ];
        assert!(matches!(
            normalize_items(duplicate),
            Err(ProviderError::InvalidData(_))
        ));

        let invalid = vec![TierEntry {
            champion_id: 1,
            win_rate: f64::NAN,
            win_rate_delta: None,
            games: None,
            pick_rate: 0.1,
            ban_rate: 0.1,
            provenance: provenance(),
        }];
        assert!(matches!(
            normalize_tier_entries(invalid),
            Err(ProviderError::InvalidData(_))
        ));
    }

    #[test]
    fn normalizes_shared_sort_and_cap_rules() {
        let tier = normalize_tier_entries(vec![
            TierEntry {
                champion_id: 1,
                win_rate: 0.51,
                win_rate_delta: None,
                games: None,
                pick_rate: 0.1,
                ban_rate: 0.1,
                provenance: provenance(),
            },
            TierEntry {
                champion_id: 2,
                win_rate: 0.55,
                win_rate_delta: Some(0.0),
                games: Some(0),
                pick_rate: 0.1,
                ban_rate: 0.1,
                provenance: provenance(),
            },
        ])
        .expect("tier");
        assert_eq!(tier[0].champion_id, 2);
        assert_eq!(tier[0].games, Some(0));
        assert_eq!(tier[1].games, None);

        let counters = normalize_counter_entries(
            (1..=10)
                .map(|champion_id| CounterEntry {
                    champion_id,
                    win_rate: 0.4 + champion_id as f64 / 100.0,
                    games: MIN_MATCHUP_GAMES,
                })
                .collect(),
        )
        .expect("counters");
        assert_eq!(counters.len(), 8);
        assert_eq!(counters[0].champion_id, 10);
    }

    #[test]
    fn rejects_malformed_rune_structure_and_small_counter_samples() {
        let runes = RuneBuild {
            page_name: "bad".into(),
            lane: "Middle".into(),
            win_rate: 0.5,
            games: 10,
            primary_style_id: 8100,
            sub_style_id: 8200,
            primary_perk_ids: vec![8112],
            sub_perk_ids: vec![],
            shard_ids: vec![],
            spell_ids: vec![4],
            matchup: false,
        };
        assert!(matches!(
            normalize_rune_build(runes),
            Err(ProviderError::InvalidData(_))
        ));
        assert!(matches!(
            normalize_counter_entries(vec![CounterEntry {
                champion_id: 1,
                win_rate: 0.55,
                games: MIN_MATCHUP_GAMES - 1,
            }]),
            Err(ProviderError::InvalidData(_))
        ));
    }
}
