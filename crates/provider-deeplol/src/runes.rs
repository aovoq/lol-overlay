use std::cmp::Reverse;
use std::collections::HashMap;

use crate::types::OtpEntry;

const OFFENSE_SHARDS: &[i64] = &[5008, 5005, 5007];
const FLEX_SHARDS: &[i64] = &[5008, 5010, 5001];
const DEFENSE_SHARDS: &[i64] = &[5011, 5013, 5001];

/// LCU expects stat shards as [offense, flex, defense]. DeepLoL's aggregate
/// build endpoint has returned the same three IDs in other orders, which makes
/// page creation fail even though each individual shard is valid.
pub(super) fn normalize_stat_shards(shards: &[i64]) -> Vec<i64> {
    if valid_stat_shards(shards) {
        return shards.to_vec();
    }

    let reversed: Vec<i64> = shards.iter().rev().copied().collect();
    if valid_stat_shards(&reversed) {
        return reversed;
    }

    vec![
        shard_for_slot(shards, 0, OFFENSE_SHARDS, 5008),
        shard_for_slot(shards, 1, FLEX_SHARDS, 5008),
        shard_for_slot(shards, 2, DEFENSE_SHARDS, 5011),
    ]
}

fn valid_stat_shards(shards: &[i64]) -> bool {
    shards.len() == 3
        && OFFENSE_SHARDS.contains(&shards[0])
        && FLEX_SHARDS.contains(&shards[1])
        && DEFENSE_SHARDS.contains(&shards[2])
}

fn shard_for_slot(shards: &[i64], slot: usize, allowed: &[i64], fallback: i64) -> i64 {
    if let Some(id) = shards.get(slot).filter(|id| allowed.contains(id)) {
        return *id;
    }

    shards
        .iter()
        .copied()
        .find(|id| allowed.contains(id))
        .unwrap_or(fallback)
}

/// One consensus rune page distilled from individual OTP matchup games.
pub(super) struct AggregatedPage {
    pub(super) primary_style: i64,
    /// [keystone, p1, p2, p3]
    pub(super) primary_perks: Vec<i64>,
    pub(super) sub_style: i64,
    /// [s1, s2]
    pub(super) sub_perks: Vec<i64>,
    /// [offense, flex, defense]
    pub(super) shards: Vec<i64>,
    /// [spell1, spell2]; empty when no game had a usable spell pair.
    pub(super) spells: Vec<i64>,
}

/// Build the consensus page: group games by (primary style, keystone) so
/// different archetypes don't blend into an invalid hybrid, then take the
/// per-slot mode inside the largest group. OTP slot layout: `perk_0` =
/// keystone, `perk_1..3` = primary minors, `perk_4..5` = secondary minors.
pub(super) fn aggregate_otp(samples: &[&OtpEntry]) -> Option<AggregatedPage> {
    let mut groups: HashMap<(i64, i64), Vec<&OtpEntry>> = HashMap::new();
    for sample in samples {
        groups
            .entry((sample.rune.perk_primary_style, sample.rune.perk_0))
            .or_default()
            .push(sample);
    }
    // Largest archetype wins; ties break on the smaller key for determinism.
    let ((primary_style, keystone), group) = groups
        .into_iter()
        .max_by_key(|(key, values)| (values.len(), Reverse(*key)))?;

    // Secondary tree: mode the style first, then mode the minors only among
    // games that used that style. Mixing trees per slot could otherwise
    // produce a page no client would accept.
    let sub_style = mode(group.iter().map(|sample| sample.rune.perk_sub_style));
    let sub_group: Vec<&&OtpEntry> = group
        .iter()
        .filter(|sample| sample.rune.perk_sub_style == sub_style)
        .collect();

    Some(AggregatedPage {
        primary_style,
        sub_style,
        primary_perks: vec![
            keystone,
            mode(group.iter().map(|sample| sample.rune.perk_1)),
            mode(group.iter().map(|sample| sample.rune.perk_2)),
            mode(group.iter().map(|sample| sample.rune.perk_3)),
        ],
        sub_perks: vec![
            mode(sub_group.iter().map(|sample| sample.rune.perk_4)),
            mode(sub_group.iter().map(|sample| sample.rune.perk_5)),
        ],
        shards: normalize_stat_shards(&[
            mode(group.iter().map(|sample| sample.rune.stat_perk_0)),
            mode(group.iter().map(|sample| sample.rune.stat_perk_1)),
            mode(group.iter().map(|sample| sample.rune.stat_perk_2)),
        ]),
        // Spells are independent of the rune archetype, so count them across
        // all of the lane's games, not just the winning rune group.
        spells: most_common_spell_pair(samples),
    })
}

/// Most common summoner-spell pair across games. Flash sits in either slot
/// depending on the player's keybind, so `[4,11]` and `[11,4]` count as the
/// same pair; the output keeps whichever orientation occurred more often.
fn most_common_spell_pair(samples: &[&OtpEntry]) -> Vec<i64> {
    // normalized (min,max) pair -> (count as-is, count swapped)
    let mut counts: HashMap<(i64, i64), (usize, usize)> = HashMap::new();
    for sample in samples {
        let (a, b) = (sample.spell.spell_1, sample.spell.spell_2);
        if a <= 0 || b <= 0 {
            continue;
        }
        let key = (a.min(b), a.max(b));
        let slot = counts.entry(key).or_default();
        if (a, b) == key {
            slot.0 += 1;
        } else {
            slot.1 += 1;
        }
    }
    let Some((key, (as_is, swapped))) = counts
        .into_iter()
        .max_by_key(|&(key, (as_is, swapped))| (as_is + swapped, Reverse(key)))
    else {
        return Vec::new();
    };
    if swapped > as_is {
        vec![key.1, key.0]
    } else {
        vec![key.0, key.1]
    }
}

/// Most frequent positive value; ties break on the smaller value for
/// determinism. 0 only when the input has no positive values.
fn mode<I: Iterator<Item = i64>>(values: I) -> i64 {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for value in values {
        if value > 0 {
            *counts.entry(value).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(value, count)| (count, Reverse(value)))
        .map(|(value, _)| value)
        .unwrap_or(0)
}
