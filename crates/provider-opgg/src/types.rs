//! Serde types for the clean `"data"` props op.gg ships for runes, counters,
//! the per-lane tier list, and skill order (see [`crate::flight`] module docs
//! for why only some sections have one).
//!
//! Unlike LoLalytics, op.gg is consistent about number encoding *within* a
//! given payload, but the payloads disagree with each other: a [`RunePage`]'s
//! own `pick_rate`/`win_rate` and [`SkillBuild::win_rate`] are 0..1 fractions,
//! while every percentage nested inside a rune page ([`Perk::win_rate`] and
//! friends, via `#[serde(default)]`-less plain `f64` fields not modeled
//! here), [`CounterRow::win_rate`], and every [`TierRow`] rate are 0..100.
//! Each field doc below says which.

use serde::Deserialize;

/// One primary/secondary rune combo, sorted by popularity (`rune_pages[0]` is
/// the site's top recommendation).
#[derive(Debug, Clone, Deserialize)]
pub struct RunePage {
    pub play: i64,
    /// 0..1. Not consumed today; kept to document the wire shape.
    #[allow(dead_code)]
    pub pick_rate: f64,
    /// 0..1.
    pub win_rate: f64,
    /// Style-pair variants sharing this keystone (almost always a single
    /// entry); `builds[0]` is what op.gg recommends.
    #[serde(default)]
    pub builds: Vec<RuneBuildData>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuneBuildData {
    pub primary_perk_style: PerkStyle,
    pub perk_sub_style: PerkStyle,
    /// 4 rows (keystone + 3 perk slots), each a list of the row's candidate
    /// perks. Exactly one candidate per row has `is_active: true` — that's
    /// the recommended pick.
    #[serde(default)]
    pub main_runes: Vec<Vec<Perk>>,
    /// 3 candidate rows for the secondary tree; exactly 2 across all rows are
    /// active (the site picks 2 of the tree's rows).
    #[serde(default)]
    pub sub_runes: Vec<Vec<Perk>>,
    /// 3 rows (offense/flex/defense), each exactly one active.
    #[serde(default)]
    pub shards: Vec<Vec<Perk>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PerkStyle {
    pub id: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Perk {
    pub id: i64,
    #[serde(default, rename = "isActive")]
    pub is_active: bool,
}

/// One row of the `"data"` prop backing a champion's counters/matchup table:
/// this champion's stats specifically against `champion`.
#[derive(Debug, Clone, Deserialize)]
pub struct CounterRow {
    pub play: i64,
    /// The **subject** champion's (not the opponent's) win rate in this
    /// matchup, 0..100 — confirmed against the page's own "Aatrox 50.39% /
    /// Yone 49.61%" matchup header, which lists the subject first.
    pub win_rate: f64,
    pub champion: CounterChampion,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CounterChampion {
    /// Data Dragon alias, lowercased (`"monkeyking"`, `"chogath"`) — the same
    /// slug op.gg uses in its own champion URLs.
    pub key: String,
}

/// One row of the `"data"` prop backing `/lol/champions?position=<lane>`: a
/// single champion's stats in that lane, for the site-wide tier list.
/// op.gg has no cross-lane "all roles" tier list — each lane is a separate
/// fetch — so this only ever represents one lane at a time.
#[derive(Debug, Clone, Deserialize)]
pub struct TierRow {
    /// Data Dragon alias, lowercased — same convention as [`CounterChampion::key`].
    pub key: String,
    /// 0..100.
    #[serde(rename = "positionWinRate")]
    pub win_rate: f64,
    /// 0..100.
    #[serde(rename = "positionPickRate")]
    pub pick_rate: f64,
    /// 0..100.
    #[serde(rename = "positionBanRate")]
    pub ban_rate: f64,
}

/// One row of the `"skill_masteries"` data prop on `/skills[/lane]`: a
/// basic-skill max-priority combo (e.g. `["Q","E","W"]`) and the concrete
/// level-by-level variants players actually use with it. Sorted by
/// popularity; `[0]` is what op.gg recommends.
#[derive(Debug, Clone, Deserialize)]
pub struct SkillMastery {
    /// Max-priority letters, e.g. `["Q","E","W"]` (no `"R"` — the ultimate is
    /// always maxed on its own schedule, independent of this ordering).
    pub ids: Vec<String>,
    /// Concrete level-by-level orders sharing this max priority, sorted by
    /// popularity; `builds[0]` is what op.gg recommends.
    #[serde(default)]
    pub builds: Vec<SkillBuild>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillBuild {
    /// One letter (`"Q"`/`"W"`/`"E"`/`"R"`) per level, in pick order. Only
    /// covers early-to-mid game (typically 15 entries) — op.gg doesn't track
    /// leveling past that.
    pub order: Vec<String>,
    pub play: i64,
    /// 0..1.
    pub win_rate: f64,
}
