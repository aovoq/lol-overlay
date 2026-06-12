//! Data-source abstraction.
//!
//! Everything the overlay needs from "the internet" — item recommendations and
//! rune pages — flows through [`BuildProvider`]. Today the only implementation
//! is [`hardcoded::HardcodedProvider`], but swapping in a real backend (a stats
//! API you build from Match-V5, a scraped dataset, an AI model, …) means writing
//! one more `impl BuildProvider` and changing one line in `lib.rs`.

use async_trait::async_trait;

use overlay_types::{
    CounterEntry, GameSnapshot, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder,
    TierEntry,
};

use crate::error::{ProviderError, Result};

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
        Err(ProviderError::NotEnoughData)
    }

    /// Best rune page for a champion in a role (role is the LCU position string,
    /// e.g. `"middle"`). `None` role means use a generic page.
    async fn runes(&self, champion_id: i64, role: Option<&str>) -> Result<RuneRecommendation>;

    /// Tier list for a role (LCU position string). Sorted by win rate desc.
    async fn tier_list(&self, _role: &str) -> Result<Vec<TierEntry>> {
        Err(ProviderError::NotEnoughData)
    }

    /// Champions that counter `champion_id` in `role`, best counters first.
    async fn counters(&self, _champion_id: i64, _role: &str) -> Result<Vec<CounterEntry>> {
        Err(ProviderError::NotEnoughData)
    }

    /// Detailed rune page (incl. shards + spells). With `enemy_champion_id`,
    /// build a matchup-specific page or fail with `Error::NotEnoughData`.
    async fn rune_build(
        &self,
        _champion_id: i64,
        _role: Option<&str>,
        _enemy_champion_id: Option<i64>,
    ) -> Result<RuneBuild> {
        Err(ProviderError::NotEnoughData)
    }

    /// Display name + Data Dragon image id for a champion
    /// (`("Cho'Gath", "Chogath")`). Lets the debug/mock scenarios be built
    /// from live data instead of hardcoded champions; `None` when unknown.
    async fn champion_names(&self, _champion_id: i64) -> Option<(String, String)> {
        None
    }
}
