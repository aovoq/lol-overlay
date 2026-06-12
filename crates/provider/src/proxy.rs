//! Runtime data-source routing.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use overlay_types::{
    CounterEntry, GameSnapshot, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder,
    TierEntry,
};

use crate::error::{ProviderError, Result};
use crate::trait_def::BuildProvider;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    #[default]
    Deeplol,
    Ugg,
}

impl<'de> Deserialize<'de> for ProviderKind {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::parse(&s).unwrap_or_default())
    }
}

impl ProviderKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "deeplol" => Some(Self::Deeplol),
            "ugg" => Some(Self::Ugg),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deeplol => "deeplol",
            Self::Ugg => "ugg",
        }
    }
}

/// Routes every [`BuildProvider`] call to the currently active backend.
/// Switching is O(1); each backend keeps its own caches, so flipping
/// back and forth costs nothing after warm-up.
pub struct ProviderProxy {
    providers: HashMap<ProviderKind, Arc<dyn BuildProvider>>,
    active: RwLock<ProviderKind>,
}

impl ProviderProxy {
    pub fn new(initial: ProviderKind) -> Self {
        Self {
            providers: HashMap::new(),
            active: RwLock::new(initial),
        }
    }

    pub fn register(&mut self, kind: ProviderKind, provider: Arc<dyn BuildProvider>) {
        self.providers.insert(kind, provider);
    }

    pub fn set_active(&self, kind: ProviderKind) -> Result<()> {
        if !self.providers.contains_key(&kind) {
            return Err(ProviderError::Other(format!(
                "provider {kind:?} not registered"
            )));
        }
        *self.active.write().unwrap() = kind;
        Ok(())
    }

    pub fn active(&self) -> ProviderKind {
        *self.active.read().unwrap()
    }

    pub fn available(&self) -> Vec<ProviderKind> {
        let mut kinds: Vec<_> = self.providers.keys().copied().collect();
        kinds.sort_by_key(|k| k.as_str());
        kinds
    }

    fn current(&self) -> Arc<dyn BuildProvider> {
        let kind = *self.active.read().unwrap();
        self.providers
            .get(&kind)
            .expect("active provider must be registered")
            .clone()
    }
}

#[async_trait]
impl BuildProvider for ProviderProxy {
    fn set_platform_id(&self, platform_id: &str) {
        for provider in self.providers.values() {
            provider.set_platform_id(platform_id);
        }
    }

    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
        self.current().items(snapshot).await
    }

    async fn skill_order(&self, snapshot: &GameSnapshot) -> Result<SkillOrder> {
        self.current().skill_order(snapshot).await
    }

    async fn runes(&self, champion_id: i64, role: Option<&str>) -> Result<RuneRecommendation> {
        self.current().runes(champion_id, role).await
    }

    async fn tier_list(&self, role: &str) -> Result<Vec<TierEntry>> {
        self.current().tier_list(role).await
    }

    async fn counters(&self, champion_id: i64, role: &str) -> Result<Vec<CounterEntry>> {
        self.current().counters(champion_id, role).await
    }

    async fn rune_build(
        &self,
        champion_id: i64,
        role: Option<&str>,
        enemy_champion_id: Option<i64>,
    ) -> Result<RuneBuild> {
        self.current()
            .rune_build(champion_id, role, enemy_champion_id)
            .await
    }

    async fn champion_names(&self, champion_id: i64) -> Option<(String, String)> {
        self.current().champion_names(champion_id).await
    }
}
