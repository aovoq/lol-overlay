use std::collections::HashMap;

use serde::Deserialize;

/// Deserialize, mapping an explicit JSON `null` to `T::default()`.
pub(super) fn null_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
pub(super) struct VersionResponse {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) game_version_list: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct BuildResponse {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) build_by_lane: HashMap<String, LaneBuild>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub(super) struct LaneBuild {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) build_lst: Vec<BuildEntry>,
    /// Real per-lane champion games — also the games-calibration numerator.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) games: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) pick_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) ban_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) match_up: MatchUp,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub(super) struct MatchUp {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) strong_against: Vec<MatchUpEntry>,
    /// Champions the subject loses to, sorted ascending by the subject's
    /// `win_rate` (worst matchup first).
    #[serde(default, deserialize_with = "null_default")]
    pub(super) weak_against: Vec<MatchUpEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub(super) struct MatchUpEntry {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) games: i64,
    /// The subject champion's win rate vs this enemy (fraction 0..1).
    #[serde(default, deserialize_with = "null_default")]
    pub(super) win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) match_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) enemy_champion_id: i64,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct BuildEntry {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) rune: RuneBlock,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) item: ItemBlock,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) spell: SpellBlock,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) skill: SkillBlock,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) games: i64,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct RuneBlock {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) main_build: Vec<i64>,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) sub_build: Vec<i64>,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) stat_build: Vec<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct ItemBlock {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) build: Vec<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SpellBlock {
    /// Summoner spell ids, e.g. `[14, 4]` (Ignite + Flash).
    #[serde(default, deserialize_with = "null_default")]
    pub(super) build: Vec<i64>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct SkillBlock {
    /// Basic-skill max order, e.g. `[3, 1, 2]` = E > Q > W.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) build: Vec<i64>,
    /// Level-by-level skill order. Riot skill ids: 1 = Q, 2 = W, 3 = E, 4 = R.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) detail: Vec<i64>,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) games: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct RankResponse {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) champion_data_list: Vec<RankChampion>,
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct RankChampion {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) champion_id: i64,
    /// Keys: `Top|Jungle|Middle|Bot|Supporter|Aram|Total`.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) performance_dict: HashMap<String, RankPerformance>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[allow(dead_code)]
pub(super) struct RankPerformance {
    /// Fraction 0..1; 0 = the champion isn't played in this lane.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) pick_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) ban_rate: f64,
    /// 1-5, 0 = not played.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) tier: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) rank: i64,
    /// Rank-position movement, NOT a win-rate delta.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) rank_delta: i64,
    /// Always 0 in practice — unusable; see games calibration.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) games: i64,
}

#[derive(Debug, Deserialize)]
pub(super) struct MatchupStatsResponse {
    /// Whole node is JSON `null` for an invalid/unplayed pair.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) stats_by_position: HashMap<String, PositionStats>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[allow(dead_code)]
pub(super) struct PositionStats {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) games: i64,
    /// PERCENT 0-100 — the one DeepLoL payload that isn't a fraction.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) my_win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) enemy_win_rate: f64,
}

#[derive(Debug, Deserialize)]
pub(super) struct OtpResponse {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) match_up_list: Vec<OtpEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
pub(super) struct OtpEntry {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) position: String,
    /// 0 | 1.
    #[serde(default, deserialize_with = "null_default")]
    pub(super) win: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) rune: OtpRune,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) spell: OtpSpell,
}

/// One game's full rune page. Slot layout: `perk_0` = keystone, `perk_1..3` =
/// primary minors, `perk_4..5` = secondary minors.
#[derive(Debug, Default, Deserialize)]
pub(super) struct OtpRune {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) perk_0: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) perk_1: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) perk_2: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) perk_3: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) perk_4: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) perk_5: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) perk_primary_style: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) perk_sub_style: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) stat_perk_0: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) stat_perk_1: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) stat_perk_2: i64,
}

impl OtpRune {
    /// True when every slot is filled. A partial block would poison the
    /// per-slot mode, so such games are excluded from aggregation.
    pub(super) fn is_complete(&self) -> bool {
        self.perk_primary_style > 0
            && self.perk_sub_style > 0
            && [
                self.perk_0,
                self.perk_1,
                self.perk_2,
                self.perk_3,
                self.perk_4,
                self.perk_5,
                self.stat_perk_0,
                self.stat_perk_1,
                self.stat_perk_2,
            ]
            .iter()
            .all(|&perk| perk > 0)
    }
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct OtpSpell {
    #[serde(default, deserialize_with = "null_default")]
    pub(super) spell_1: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub(super) spell_2: i64,
}
