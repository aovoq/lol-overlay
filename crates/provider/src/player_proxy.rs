//! Independent routing for player-stat providers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use overlay_types::{MatchPage, PlayerChampionStats, PlayerProfile, PlayerRef, RefreshResult};

use crate::error::Result;
use crate::player_trait::{PlayerStatsProvider, ProviderCapabilities, ProviderDescriptor};
use crate::proxy::ProviderKind;
use crate::router::ProviderRouter;

pub struct PlayerStatsProxy {
    router: ProviderRouter<ProviderKind, dyn PlayerStatsProvider>,
    cache: Mutex<PlayerCache>,
    inflight: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

type Timed<T> = (Instant, T);

#[derive(Default)]
struct PlayerCache {
    profiles: HashMap<String, Timed<PlayerProfile>>,
    matches: HashMap<String, Timed<MatchPage>>,
    champions: HashMap<String, Timed<Vec<PlayerChampionStats>>>,
}

fn fresh<T: Clone>(entry: Option<&Timed<T>>) -> Option<T> {
    entry
        .filter(|(loaded, _)| loaded.elapsed() < CACHE_TTL)
        .map(|(_, value)| value.clone())
}

fn player_key(kind: ProviderKind, player: &PlayerRef) -> String {
    format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}",
        kind.as_str(),
        player.platform_id,
        player.game_name,
        player.tag_line
    )
}

impl PlayerStatsProxy {
    pub fn new(
        initial: ProviderKind,
        providers: impl IntoIterator<Item = (ProviderKind, Arc<dyn PlayerStatsProvider>)>,
    ) -> Result<Self> {
        Ok(Self {
            router: ProviderRouter::new(initial, providers)?,
            cache: Mutex::new(PlayerCache::default()),
            inflight: tokio::sync::Mutex::new(HashMap::new()),
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

    async fn request_lock(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.inflight
            .lock()
            .await
            .entry(key.into())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    fn provider(&self, kind: ProviderKind) -> Arc<dyn PlayerStatsProvider> {
        self.router
            .get(kind)
            .expect("active provider must be registered")
    }
}

#[async_trait]
impl PlayerStatsProvider for PlayerStatsProxy {
    async fn profile(&self, player: &PlayerRef, force: bool) -> Result<PlayerProfile> {
        let kind = self.active();
        let key = player_key(kind, player);
        if !force {
            if let Some(value) = fresh(self.cache.lock().unwrap().profiles.get(&key)) {
                return Ok(value);
            }
        }
        let request_lock = self.request_lock(&format!("profile:{key}")).await;
        let _request = request_lock.lock().await;
        if !force {
            if let Some(value) = fresh(self.cache.lock().unwrap().profiles.get(&key)) {
                return Ok(value);
            }
        }
        let value = self.provider(kind).profile(player, force).await?;
        self.cache
            .lock()
            .unwrap()
            .profiles
            .insert(key, (Instant::now(), value.clone()));
        Ok(value)
    }

    async fn recent_matches(
        &self,
        player: &PlayerRef,
        cursor: Option<&str>,
        queue: Option<i64>,
        force: bool,
    ) -> Result<MatchPage> {
        let kind = self.active();
        let player_key = player_key(kind, player);
        let key = format!("{player_key}\u{1f}{}\u{1f}{queue:?}", cursor.unwrap_or("0"));
        if !force {
            if let Some(value) = fresh(self.cache.lock().unwrap().matches.get(&key)) {
                return Ok(value);
            }
        }
        let request_lock = self.request_lock(&format!("matches:{key}")).await;
        let _request = request_lock.lock().await;
        if !force {
            if let Some(value) = fresh(self.cache.lock().unwrap().matches.get(&key)) {
                return Ok(value);
            }
        }
        let value = self
            .provider(kind)
            .recent_matches(player, cursor, queue, force)
            .await?;
        self.cache
            .lock()
            .unwrap()
            .matches
            .insert(key, (Instant::now(), value.clone()));
        Ok(value)
    }

    async fn champion_stats(
        &self,
        player: &PlayerRef,
        season: Option<&str>,
        queue: Option<&str>,
        role: Option<&str>,
        force: bool,
    ) -> Result<Vec<PlayerChampionStats>> {
        let kind = self.active();
        let player_key = player_key(kind, player);
        let key = format!(
            "{player_key}\u{1f}{}\u{1f}{}\u{1f}{}",
            season.unwrap_or("current"),
            queue.unwrap_or("all"),
            role.unwrap_or("all")
        );
        if !force {
            if let Some(value) = fresh(self.cache.lock().unwrap().champions.get(&key)) {
                return Ok(value);
            }
        }
        let request_lock = self.request_lock(&format!("champions:{key}")).await;
        let _request = request_lock.lock().await;
        if !force {
            if let Some(value) = fresh(self.cache.lock().unwrap().champions.get(&key)) {
                return Ok(value);
            }
        }
        let value = self
            .provider(kind)
            .champion_stats(player, season, queue, role, force)
            .await?;
        self.cache
            .lock()
            .unwrap()
            .champions
            .insert(key, (Instant::now(), value.clone()));
        Ok(value)
    }

    async fn refresh(&self, player: &PlayerRef) -> Result<RefreshResult> {
        let kind = self.active();
        let prefix = player_key(kind, player);
        let result = self.provider(kind).refresh(player).await?;
        let mut cache = self.cache.lock().unwrap();
        cache.profiles.retain(|key, _| !key.starts_with(&prefix));
        cache.matches.retain(|key, _| !key.starts_with(&prefix));
        cache.champions.retain(|key, _| !key.starts_with(&prefix));
        Ok(result)
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.router.current().capabilities()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ProviderError;
    use overlay_types::{PlayerIdentity, ProviderExtras, RefreshAvailability};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Stub(&'static str);

    struct CountingStub {
        source: &'static str,
        profiles: AtomicUsize,
    }

    impl CountingStub {
        fn new(source: &'static str) -> Self {
            Self {
                source,
                profiles: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl PlayerStatsProvider for CountingStub {
        async fn profile(&self, player: &PlayerRef, _force: bool) -> Result<PlayerProfile> {
            self.profiles.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(PlayerProfile {
                source: self.source.into(),
                identity: PlayerIdentity {
                    platform_id: player.platform_id.clone(),
                    game_name: player.game_name.clone(),
                    tag_line: player.tag_line.clone(),
                    puuid: Some(self.source.into()),
                },
                level: None,
                profile_icon_id: None,
                ranks: vec![],
                previous_seasons: vec![],
                ladder_rank: None,
                ladder_percentile: None,
                fetched_at: 1,
                refresh: RefreshAvailability::default(),
                extras: ProviderExtras::None,
            })
        }

        async fn recent_matches(
            &self,
            _player: &PlayerRef,
            _cursor: Option<&str>,
            _queue: Option<i64>,
            _force: bool,
        ) -> Result<MatchPage> {
            unreachable!()
        }

        async fn champion_stats(
            &self,
            _player: &PlayerRef,
            _season: Option<&str>,
            _queue: Option<&str>,
            _role: Option<&str>,
            _force: bool,
        ) -> Result<Vec<PlayerChampionStats>> {
            unreachable!()
        }

        async fn refresh(&self, _player: &PlayerRef) -> Result<RefreshResult> {
            Ok(RefreshResult {
                source: self.source.into(),
                cache_invalidated: true,
                mutation_performed: false,
                refreshed_at: 1,
            })
        }

        fn capabilities(&self) -> ProviderCapabilities {
            ProviderCapabilities::default()
        }
    }

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

    #[tokio::test]
    async fn coalesces_duplicates_and_keeps_provider_caches_separate() {
        let deep = Arc::new(CountingStub::new("deep"));
        let ugg = Arc::new(CountingStub::new("ugg"));
        let proxy = PlayerStatsProxy::new(
            ProviderKind::Deeplol,
            [
                (
                    ProviderKind::Deeplol,
                    deep.clone() as Arc<dyn PlayerStatsProvider>,
                ),
                (
                    ProviderKind::Ugg,
                    ugg.clone() as Arc<dyn PlayerStatsProvider>,
                ),
            ],
        )
        .unwrap();

        let identity = player();
        let (first, second) = tokio::join!(
            proxy.profile(&identity, false),
            proxy.profile(&identity, false)
        );
        assert_eq!(first.unwrap().source, "deep");
        assert_eq!(second.unwrap().source, "deep");
        assert_eq!(deep.profiles.load(Ordering::SeqCst), 1);

        proxy.set_active(ProviderKind::Ugg).unwrap();
        assert_eq!(proxy.profile(&identity, false).await.unwrap().source, "ugg");
        assert_eq!(ugg.profiles.load(Ordering::SeqCst), 1);

        proxy.profile(&identity, true).await.unwrap();
        assert_eq!(ugg.profiles.load(Ordering::SeqCst), 2);
        proxy.refresh(&identity).await.unwrap();
        proxy.profile(&identity, false).await.unwrap();
        assert_eq!(ugg.profiles.load(Ordering::SeqCst), 3);
    }
}
