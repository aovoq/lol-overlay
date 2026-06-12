//! Pluggable build/rune data providers and shared threat heuristics.

mod error;
mod hardcoded;
mod proxy;
mod threat;
mod trait_def;

pub use error::{ProviderError, Result};
pub use hardcoded::{champion_damage_type, DamageType, HardcodedProvider};
pub use overlay_types::{
    CounterEntry, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder, ThreatProfile,
    TierEntry,
};
pub use proxy::{ProviderKind, ProviderProxy};
pub use threat::classify_threats;
pub use trait_def::BuildProvider;
