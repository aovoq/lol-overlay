use std::collections::HashSet;

use overlay_types::{CounterEntry, ItemRecommendation, RuneBuild, RuneRecommendation};

pub const MIN_MATCHUP_GAMES: i64 = 30;

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
