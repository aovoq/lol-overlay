//! Shared serialize-able types used across the overlay crates and frontend events.

pub mod champ_select;
pub mod lcu;
pub mod recommendation;
pub mod snapshot;

pub use champ_select::ChampSelectEvent;
pub use lcu::{MyPick, Phase, RecentGame, RunePagePayload, SummonerInfo};
pub use recommendation::{
    CounterEntry, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder, ThreatProfile,
    TierEntry,
};
pub use snapshot::{EnemyChampion, GameSnapshot};
