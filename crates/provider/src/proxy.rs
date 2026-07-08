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
    Lolalytics,
    Opgg,
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
            "lolalytics" => Some(Self::Lolalytics),
            "opgg" => Some(Self::Opgg),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Deeplol => "deeplol",
            Self::Ugg => "ugg",
            Self::Lolalytics => "lolalytics",
            Self::Opgg => "opgg",
        }
    }
}

/// Routes every [`BuildProvider`] call to the currently active backend.
/// Switching is O(1); each backend keeps its own caches, so flipping
/// back and forth costs nothing after warm-up. Calls are never retried against
/// another provider; fallback policy lives inside each active provider.
pub struct ProviderProxy {
    providers: HashMap<ProviderKind, Arc<dyn BuildProvider>>,
    active: RwLock<ProviderKind>,
}

impl ProviderProxy {
    pub fn new(
        initial: ProviderKind,
        providers: impl IntoIterator<Item = (ProviderKind, Arc<dyn BuildProvider>)>,
    ) -> Result<Self> {
        let providers = providers.into_iter().collect::<HashMap<_, _>>();
        if !providers.contains_key(&initial) {
            return Err(ProviderError::Other(format!(
                "provider {initial:?} not registered"
            )));
        }
        Ok(Self {
            providers,
            active: RwLock::new(initial),
        })
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
        kinds.sort_by_key(ProviderKind::as_str);
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

#[cfg(test)]
mod tests {
    use super::*;

    struct StubProvider {
        item_id: i64,
    }

    #[async_trait]
    impl BuildProvider for StubProvider {
        async fn items(&self, _snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
            Ok(vec![ItemRecommendation {
                item_id: self.item_id,
                name: format!("Item {}", self.item_id),
                score: 1.0,
                reason: "test".into(),
            }])
        }

        async fn runes(
            &self,
            _champion_id: i64,
            _role: Option<&str>,
        ) -> Result<RuneRecommendation> {
            Err(ProviderError::NotEnoughData)
        }
    }

    fn snapshot() -> GameSnapshot {
        GameSnapshot {
            game_mode: "CLASSIC".into(),
            game_time: 0.0,
            self_champion: "Ahri".into(),
            self_raw_name: "Ahri".into(),
            self_position: "middle".into(),
            enemies: Vec::new(),
            allies: Vec::new(),
        }
    }

    #[test]
    fn new_rejects_unregistered_initial_provider() {
        let result = ProviderProxy::new(
            ProviderKind::Ugg,
            [(
                ProviderKind::Deeplol,
                Arc::new(StubProvider { item_id: 1 }) as Arc<dyn BuildProvider>,
            )],
        );

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn routes_to_active_provider() {
        let proxy = ProviderProxy::new(
            ProviderKind::Deeplol,
            [
                (
                    ProviderKind::Deeplol,
                    Arc::new(StubProvider { item_id: 1 }) as Arc<dyn BuildProvider>,
                ),
                (
                    ProviderKind::Ugg,
                    Arc::new(StubProvider { item_id: 2 }) as Arc<dyn BuildProvider>,
                ),
            ],
        )
        .expect("proxy");

        assert_eq!(proxy.items(&snapshot()).await.expect("items")[0].item_id, 1);
        proxy.set_active(ProviderKind::Ugg).expect("switch");
        assert_eq!(proxy.items(&snapshot()).await.expect("items")[0].item_id, 2);
    }
}
