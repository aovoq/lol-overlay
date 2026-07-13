//! Runtime routing for champion build providers.

use std::sync::Arc;

use async_trait::async_trait;
use overlay_types::{
    CounterEntry, GameSnapshot, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder,
    TierEntry,
};

use crate::error::Result;
use crate::proxy::ProviderKind;
use crate::router::ProviderRouter;
use crate::shared::{
    normalize_counter_entries, normalize_items, normalize_rune_build,
    normalize_rune_recommendation, normalize_skill_order, normalize_tier_entries,
};
use crate::trait_def::BuildProvider;

/// Thin forwarding wrapper for the active [`BuildProvider`].
pub struct BuildProviderProxy {
    router: ProviderRouter<ProviderKind, dyn BuildProvider>,
}

impl BuildProviderProxy {
    pub fn new(
        initial: ProviderKind,
        providers: impl IntoIterator<Item = (ProviderKind, Arc<dyn BuildProvider>)>,
    ) -> Result<Self> {
        Ok(Self {
            router: ProviderRouter::new(initial, providers)?,
        })
    }

    pub fn set_active(&self, kind: ProviderKind) -> Result<()> {
        self.router.set_active(kind)
    }

    pub fn active(&self) -> ProviderKind {
        self.router.active()
    }

    pub fn available(&self) -> Vec<ProviderKind> {
        self.router.available_by(|a, b| a.as_str().cmp(b.as_str()))
    }
}

#[async_trait]
impl BuildProvider for BuildProviderProxy {
    fn set_platform_id(&self, platform_id: &str) {
        for provider in self.router.registered() {
            provider.set_platform_id(platform_id);
        }
    }

    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
        normalize_items(self.router.current().items(snapshot).await?)
    }

    async fn skill_order(&self, snapshot: &GameSnapshot) -> Result<SkillOrder> {
        normalize_skill_order(self.router.current().skill_order(snapshot).await?)
    }

    async fn runes(&self, champion_id: i64, role: Option<&str>) -> Result<RuneRecommendation> {
        normalize_rune_recommendation(self.router.current().runes(champion_id, role).await?)
    }

    async fn tier_list(&self, role: &str) -> Result<Vec<TierEntry>> {
        normalize_tier_entries(self.router.current().tier_list(role).await?)
    }

    async fn counters(&self, champion_id: i64, role: &str) -> Result<Vec<CounterEntry>> {
        normalize_counter_entries(self.router.current().counters(champion_id, role).await?)
    }

    async fn rune_build(
        &self,
        champion_id: i64,
        role: Option<&str>,
        enemy_champion_id: Option<i64>,
    ) -> Result<RuneBuild> {
        normalize_rune_build(
            self.router
                .current()
                .rune_build(champion_id, role, enemy_champion_id)
                .await?,
        )
    }

    async fn champion_names(&self, champion_id: i64) -> Option<(String, String)> {
        self.router.current().champion_names(champion_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ProviderError;

    struct StubProvider(i64);

    #[async_trait]
    impl BuildProvider for StubProvider {
        async fn items(&self, _snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
            Ok(vec![ItemRecommendation {
                item_id: self.0,
                name: format!("Item {}", self.0),
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
            enemies: vec![],
            allies: vec![],
            players: vec![],
        }
    }

    #[tokio::test]
    async fn forwards_to_active_build_provider_without_fallback() {
        let proxy = BuildProviderProxy::new(
            ProviderKind::Deeplol,
            [
                (
                    ProviderKind::Deeplol,
                    Arc::new(StubProvider(1)) as Arc<dyn BuildProvider>,
                ),
                (
                    ProviderKind::Ugg,
                    Arc::new(StubProvider(2)) as Arc<dyn BuildProvider>,
                ),
            ],
        )
        .expect("proxy");
        assert_eq!(proxy.items(&snapshot()).await.expect("items")[0].item_id, 1);
        proxy.set_active(ProviderKind::Ugg).expect("switch");
        assert_eq!(proxy.items(&snapshot()).await.expect("items")[0].item_id, 2);
    }
}
