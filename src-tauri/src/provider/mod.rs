//! Data-source abstraction.
//!
//! Everything the overlay needs from "the internet" — item recommendations and
//! rune pages — flows through [`BuildProvider`]. Today the only implementation
//! is [`hardcoded::HardcodedProvider`], but swapping in a real backend (a stats
//! API you build from Match-V5, a scraped dataset, an AI model, …) means writing
//! one more `impl BuildProvider` and changing one line in `lib.rs`.

pub mod deeplol;
pub mod hardcoded;

use async_trait::async_trait;
use serde::Serialize;

use crate::error::Result;
use crate::live_client::GameSnapshot;

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

/// A rune page recommendation, provider-side mirror of `lcu::RunePagePayload`.
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
    /// Win-rate change vs the previous patch, in percentage points (0.0 = unknown).
    pub win_rate_delta: f64,
    /// Estimated games this patch (0 = unknown; UI falls back to pick rate).
    pub games: i64,
    /// 0..1
    pub pick_rate: f64,
    /// 0..1
    pub ban_rate: f64,
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

#[async_trait]
pub trait BuildProvider: Send + Sync {
    /// Point stat queries at the player's region ("JP1", "KR", …). Called once
    /// when the LCU reveals the login region; providers without a region
    /// concept ignore it.
    fn set_platform_id(&self, _platform_id: &str) {}

    /// Items to build, given the current game snapshot (who we are and who we
    /// face). `snapshot.self_champion` / `enemies` carry the relevant context.
    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>>;

    /// Skill leveling order for the current champion/role.
    async fn skill_order(&self, _snapshot: &GameSnapshot) -> Result<SkillOrder> {
        Err(crate::error::Error::NotEnoughData)
    }

    /// Best rune page for a champion in a role (role is the LCU position string,
    /// e.g. `"middle"`). `None` role means use a generic page.
    async fn runes(&self, champion_id: i64, role: Option<&str>) -> Result<RuneRecommendation>;

    /// Tier list for a role (LCU position string). Sorted by win rate desc.
    async fn tier_list(&self, _role: &str) -> Result<Vec<TierEntry>> {
        Err(crate::error::Error::NotEnoughData)
    }

    /// Champions that counter `champion_id` in `role`, best counters first.
    async fn counters(&self, _champion_id: i64, _role: &str) -> Result<Vec<CounterEntry>> {
        Err(crate::error::Error::NotEnoughData)
    }

    /// Detailed rune page (incl. shards + spells). With `enemy_champion_id`,
    /// build a matchup-specific page or fail with `Error::NotEnoughData`.
    async fn rune_build(
        &self,
        _champion_id: i64,
        _role: Option<&str>,
        _enemy_champion_id: Option<i64>,
    ) -> Result<RuneBuild> {
        Err(crate::error::Error::NotEnoughData)
    }

    /// Display name + Data Dragon image id for a champion
    /// (`("Cho'Gath", "Chogath")`). Lets the debug/mock scenarios be built
    /// from live data instead of hardcoded champions; `None` when unknown.
    async fn champion_names(&self, _champion_id: i64) -> Option<(String, String)> {
        None
    }
}

/// Cheap heuristic threat classifier shared by providers. This is a placeholder
/// — it keys off a tiny built-in champion list. The real version belongs in the
/// data layer with full champion damage data.
pub fn classify_threats(snapshot: &GameSnapshot) -> ThreatProfile {
    let mut p = ThreatProfile::default();
    for e in &snapshot.enemies {
        match hardcoded::champion_damage_type(&e.raw_name) {
            hardcoded::DamageType::Physical => p.ad_count += 1,
            hardcoded::DamageType::Magic => p.ap_count += 1,
            hardcoded::DamageType::Tank => p.tank_count += 1,
            hardcoded::DamageType::Unknown => {}
        }
    }
    p.cc_heavy = snapshot.enemies.len() >= 3 && p.tank_count >= 2;
    p
}
