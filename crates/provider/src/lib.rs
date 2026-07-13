//! Pluggable build/rune data providers and shared threat heuristics.

mod build_proxy;
mod error;
mod hardcoded;
mod player_proxy;
mod player_trait;
mod proxy;
mod router;
mod shared;
mod threat;
mod trait_def;

pub use build_proxy::BuildProviderProxy;
pub use error::{ProviderError, Result};
pub use hardcoded::{champion_damage_type, DamageType, HardcodedProvider};
pub use overlay_types::{
    CounterEntry, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder, ThreatProfile,
    TierEntry,
};
pub use player_proxy::PlayerStatsProxy;
pub use player_trait::{
    validate_player_provider_contract, PlayerProviderContractFixture, PlayerStatsProvider,
    ProviderCapabilities, ProviderDescriptor,
};
pub use proxy::ProviderKind;
pub use router::ProviderRouter;
pub use shared::{
    counter_entries_from_subject_losses, item_recommendations, normalize_counter_entries,
    normalize_items, normalize_rune_build, normalize_rune_recommendation, normalize_skill_order,
    normalize_tier_entries, rune_recommendation, split_primary_secondary_runes, MIN_MATCHUP_GAMES,
};
pub use threat::classify_threats;
pub use trait_def::BuildProvider;
