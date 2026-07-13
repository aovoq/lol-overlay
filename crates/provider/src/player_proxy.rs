//! Independent routing for player-stat providers.

use std::sync::Arc;

use async_trait::async_trait;
use overlay_types::{MatchPage, PlayerChampionStats, PlayerProfile, PlayerRef, RefreshResult};

use crate::error::Result;
use crate::player_trait::{PlayerStatsProvider, ProviderCapabilities, ProviderDescriptor};
use crate::proxy::ProviderKind;
use crate::router::ProviderRouter;

pub struct PlayerStatsProxy {
    router: ProviderRouter<ProviderKind, dyn PlayerStatsProvider>,
}

impl PlayerStatsProxy {
    pub fn new(
        initial: ProviderKind,
        providers: impl IntoIterator<Item = (ProviderKind, Arc<dyn PlayerStatsProvider>)>,
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

    pub fn available(&self) -> Vec<ProviderDescriptor> {
        self.router
            .available_by(|a, b| a.as_str().cmp(b.as_str()))
            .into_iter()
            .map(|kind| {
                let capabilities = self
                    .router
                    .get(kind)
                    .expect("available provider must be registered")
                    .capabilities();
                ProviderDescriptor {
                    id: kind.as_str().into(),
                    label: match kind {
                        ProviderKind::Deeplol => "DeepLoL",
                        ProviderKind::Ugg => "U.GG",
                        ProviderKind::Opgg => "OP.GG",
                        ProviderKind::Lolalytics => "LoLalytics",
                    }
                    .into(),
                    capabilities,
                }
            })
            .collect()
    }
}

#[async_trait]
impl PlayerStatsProvider for PlayerStatsProxy {
    async fn profile(&self, player: &PlayerRef, force: bool) -> Result<PlayerProfile> {
        self.router.current().profile(player, force).await
    }

    async fn recent_matches(
        &self,
        player: &PlayerRef,
        cursor: Option<&str>,
        queue: Option<i64>,
        force: bool,
    ) -> Result<MatchPage> {
        self.router
            .current()
            .recent_matches(player, cursor, queue, force)
            .await
    }

    async fn champion_stats(
        &self,
        player: &PlayerRef,
        season: Option<&str>,
        queue: Option<&str>,
        role: Option<&str>,
        force: bool,
    ) -> Result<Vec<PlayerChampionStats>> {
        self.router
            .current()
            .champion_stats(player, season, queue, role, force)
            .await
    }

    async fn refresh(&self, player: &PlayerRef) -> Result<RefreshResult> {
        self.router.current().refresh(player).await
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.router.current().capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ProviderError;

    struct Stub(&'static str);

    #[async_trait]
    impl PlayerStatsProvider for Stub {
        async fn profile(&self, _player: &PlayerRef, _force: bool) -> Result<PlayerProfile> {
            Err(ProviderError::Other(self.0.into()))
        }

        async fn recent_matches(
            &self,
            _player: &PlayerRef,
            _cursor: Option<&str>,
            _queue: Option<i64>,
            _force: bool,
        ) -> Result<MatchPage> {
            Err(ProviderError::Other(self.0.into()))
        }

        async fn champion_stats(
            &self,
            _player: &PlayerRef,
            _season: Option<&str>,
            _queue: Option<&str>,
            _role: Option<&str>,
            _force: bool,
        ) -> Result<Vec<PlayerChampionStats>> {
            Err(ProviderError::Other(self.0.into()))
        }

        async fn refresh(&self, _player: &PlayerRef) -> Result<RefreshResult> {
            Err(ProviderError::Other(self.0.into()))
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities {
                player_profile: true,
                direct_api: true,
                ..ProviderCapabilities::default()
            }
        }
    }

    fn player() -> PlayerRef {
        PlayerRef {
            platform_id: "JP1".into(),
            game_name: "Player".into(),
            tag_line: "JP1".into(),
        }
    }

    #[tokio::test]
    async fn switching_is_independent_and_errors_do_not_fallback() {
        let proxy = PlayerStatsProxy::new(
            ProviderKind::Deeplol,
            [
                (
                    ProviderKind::Deeplol,
                    Arc::new(Stub("deep")) as Arc<dyn PlayerStatsProvider>,
                ),
                (
                    ProviderKind::Ugg,
                    Arc::new(Stub("ugg")) as Arc<dyn PlayerStatsProvider>,
                ),
            ],
        )
        .expect("proxy");
        assert_eq!(
            proxy
                .profile(&player(), false)
                .await
                .unwrap_err()
                .to_string(),
            "deep"
        );
        proxy.set_active(ProviderKind::Ugg).expect("switch");
        assert_eq!(
            proxy
                .profile(&player(), false)
                .await
                .unwrap_err()
                .to_string(),
            "ugg"
        );
        assert_eq!(proxy.available().len(), 2);
    }
}
