//! Independent routing for player-stat providers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use overlay_types::{MatchPage, PlayerChampionStats, PlayerProfile, PlayerRef, RefreshResult};

use crate::error::ProviderError;
use crate::error::Result;
use crate::player_trait::{PlayerStatsProvider, ProviderCapabilities, ProviderDescriptor};
use crate::proxy::ProviderKind;
use crate::router::ProviderRouter;

pub struct PlayerStatsProxy {
    router: ProviderRouter<ProviderKind, dyn PlayerStatsProvider>,
    cache: Mutex<PlayerCache>,
    inflight: tokio::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    epochs: Mutex<HashMap<String, u64>>,
}

const CACHE_TTL: Duration = Duration::from_mins(5);

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

fn fresh_for_request<T: Clone>(
    entry: Option<&Timed<T>>,
    force: bool,
    request_started: Instant,
) -> Option<T> {
    entry
        .filter(|(loaded, _)| {
            loaded.elapsed() < CACHE_TTL && (!force || *loaded >= request_started)
        })
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

fn invalid(message: impl Into<String>) -> ProviderError {
    ProviderError::InvalidData(message.into())
}

fn validate_profile(kind: ProviderKind, profile: &PlayerProfile) -> Result<()> {
    if profile.source != kind.as_str() {
        return Err(invalid(format!(
            "{} profile claimed source {}",
            kind.as_str(),
            profile.source
        )));
    }
    if profile.identity.platform_id.is_empty()
        || profile.identity.game_name.is_empty()
        || profile.identity.tag_line.is_empty()
    {
        return Err(invalid("player profile contained an empty identity field"));
    }
    if profile
        .ladder_percentile
        .is_some_and(|value| !value.is_finite() || !(0.0..=100.0).contains(&value))
    {
        return Err(invalid(
            "player profile contained an invalid ladder percentile",
        ));
    }
    Ok(())
}

fn validate_match_page(kind: ProviderKind, page: &MatchPage) -> Result<()> {
    if page.source != kind.as_str() {
        return Err(invalid(format!(
            "{} match page claimed source {}",
            kind.as_str(),
            page.source
        )));
    }
    let mut ids = std::collections::HashSet::new();
    for game in &page.matches {
        if game.match_id.is_empty() || !ids.insert(game.match_id.as_str()) {
            return Err(invalid(
                "match page contained an empty or duplicate match ID",
            ));
        }
        if game.duration_seconds < 0
            || game.champion_id <= 0
            || game.kills < 0
            || game.deaths < 0
            || game.assists < 0
        {
            return Err(invalid(format!(
                "match {} contained invalid player statistics",
                game.match_id
            )));
        }
    }
    for failure in &page.partial_failures {
        if failure.match_id.is_empty() || !ids.insert(failure.match_id.as_str()) {
            return Err(invalid(
                "match page contained an empty or duplicate failure match ID",
            ));
        }
    }
    Ok(())
}

fn validate_champions(kind: ProviderKind, champions: &[PlayerChampionStats]) -> Result<()> {
    let mut keys = std::collections::HashSet::new();
    for entry in champions {
        if entry.source != kind.as_str() {
            return Err(invalid(format!(
                "{} champion row claimed source {}",
                kind.as_str(),
                entry.source
            )));
        }
        let key = (
            entry.champion_id,
            entry.role.as_deref(),
            entry.queue.as_str(),
        );
        if entry.champion_id <= 0 || !keys.insert(key) {
            return Err(invalid(
                "champion stats contained an invalid or duplicate row",
            ));
        }
        if entry.games < 0
            || entry.wins < 0
            || entry.losses < 0
            || entry.wins.saturating_add(entry.losses) > entry.games
            || !entry.win_rate.is_finite()
            || !(0.0..=1.0).contains(&entry.win_rate)
            || entry
                .kda
                .is_some_and(|value| !value.is_finite() || value < 0.0)
            || entry
                .cs_per_minute
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(invalid(format!(
                "champion {} contained invalid statistics",
                entry.champion_id
            )));
        }
    }
    Ok(())
}

impl PlayerStatsProxy {
    pub fn new(
        initial: ProviderKind,
        providers: impl IntoIterator<Item = (ProviderKind, Arc<dyn PlayerStatsProvider>)>,
    ) -> Result<Self> {
        let providers = providers.into_iter().collect::<Vec<_>>();
        if !is_player_provider_kind(initial)
            || providers
                .iter()
                .any(|(kind, _)| !is_player_provider_kind(*kind))
        {
            return Err(ProviderError::InvalidPlayerRequest(
                "Player Stats supports only DeepLoL and OP.GG".into(),
            ));
        }
        Ok(Self {
            router: ProviderRouter::new(initial, providers)?,
            cache: Mutex::new(PlayerCache::default()),
            inflight: tokio::sync::Mutex::new(HashMap::new()),
            epochs: Mutex::new(HashMap::new()),
        })
    }

    pub fn set_active(&self, kind: ProviderKind) -> Result<()> {
        if !is_player_provider_kind(kind) {
            return Err(ProviderError::InvalidPlayerRequest(format!(
                "{} is not a Player Stats provider",
                kind.as_str()
            )));
        }
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
                        ProviderKind::Opgg => "OP.GG",
                        ProviderKind::Ugg | ProviderKind::Lolalytics => {
                            unreachable!("non-player provider passed constructor validation")
                        }
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

    async fn release_request_lock(&self, key: &str, request_lock: &Arc<tokio::sync::Mutex<()>>) {
        let mut inflight = self.inflight.lock().await;
        let is_current = inflight
            .get(key)
            .is_some_and(|current| Arc::ptr_eq(current, request_lock));
        if is_current && Arc::strong_count(request_lock) == 2 {
            inflight.remove(key);
        }
    }

    fn epoch(&self, player_key: &str) -> u64 {
        self.epochs
            .lock()
            .unwrap()
            .get(player_key)
            .copied()
            .unwrap_or_default()
    }

    fn advance_epoch(&self, player_key: &str) {
        let mut epochs = self.epochs.lock().unwrap();
        let epoch = epochs.entry(player_key.into()).or_default();
        *epoch = epoch.wrapping_add(1);
    }

    fn invalidate_cache(&self, prefix: &str) {
        let mut cache = self.cache.lock().unwrap();
        cache.profiles.retain(|key, _| !key.starts_with(prefix));
        cache.matches.retain(|key, _| !key.starts_with(prefix));
        cache.champions.retain(|key, _| !key.starts_with(prefix));
    }

    fn provider(&self, kind: ProviderKind) -> Arc<dyn PlayerStatsProvider> {
        self.router
            .get(kind)
            .expect("active provider must be registered")
    }
}

fn is_player_provider_kind(kind: ProviderKind) -> bool {
    matches!(kind, ProviderKind::Deeplol | ProviderKind::Opgg)
}

#[async_trait]
impl PlayerStatsProvider for PlayerStatsProxy {
    async fn profile(&self, player: &PlayerRef, force: bool) -> Result<PlayerProfile> {
        let kind = self.active();
        let key = player_key(kind, player);
        let request_started = Instant::now();
        let epoch = self.epoch(&key);
        if !force {
            if let Some(value) = fresh(self.cache.lock().unwrap().profiles.get(&key)) {
                return Ok(value);
            }
        }
        let inflight_key = format!("profile:{key}");
        let request_lock = self.request_lock(&inflight_key).await;
        let request = request_lock.lock().await;
        let result = async {
            if let Some(value) = fresh_for_request(
                self.cache.lock().unwrap().profiles.get(&key),
                force,
                request_started,
            ) {
                return Ok(value);
            }
            let value = self.provider(kind).profile(player, force).await?;
            validate_profile(kind, &value)?;
            if self.epoch(&key) == epoch {
                self.cache
                    .lock()
                    .unwrap()
                    .profiles
                    .insert(key, (Instant::now(), value.clone()));
            }
            Ok(value)
        }
        .await;
        drop(request);
        self.release_request_lock(&inflight_key, &request_lock)
            .await;
        result
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
        let request_started = Instant::now();
        let epoch = self.epoch(&player_key);
        let key = format!("{player_key}\u{1f}{}\u{1f}{queue:?}", cursor.unwrap_or("0"));
        if !force {
            if let Some(value) = fresh(self.cache.lock().unwrap().matches.get(&key)) {
                return Ok(value);
            }
        }
        let inflight_key = format!("matches:{key}");
        let request_lock = self.request_lock(&inflight_key).await;
        let request = request_lock.lock().await;
        let result = async {
            if let Some(value) = fresh_for_request(
                self.cache.lock().unwrap().matches.get(&key),
                force,
                request_started,
            ) {
                return Ok(value);
            }
            let value = self
                .provider(kind)
                .recent_matches(player, cursor, queue, force)
                .await?;
            validate_match_page(kind, &value)?;
            if self.epoch(&player_key) == epoch {
                self.cache
                    .lock()
                    .unwrap()
                    .matches
                    .insert(key, (Instant::now(), value.clone()));
            }
            Ok(value)
        }
        .await;
        drop(request);
        self.release_request_lock(&inflight_key, &request_lock)
            .await;
        result
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
        let request_started = Instant::now();
        let epoch = self.epoch(&player_key);
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
        let inflight_key = format!("champions:{key}");
        let request_lock = self.request_lock(&inflight_key).await;
        let request = request_lock.lock().await;
        let result = async {
            if let Some(value) = fresh_for_request(
                self.cache.lock().unwrap().champions.get(&key),
                force,
                request_started,
            ) {
                return Ok(value);
            }
            let value = self
                .provider(kind)
                .champion_stats(player, season, queue, role, force)
                .await?;
            validate_champions(kind, &value)?;
            if self.epoch(&player_key) == epoch {
                self.cache
                    .lock()
                    .unwrap()
                    .champions
                    .insert(key, (Instant::now(), value.clone()));
            }
            Ok(value)
        }
        .await;
        drop(request);
        self.release_request_lock(&inflight_key, &request_lock)
            .await;
        result
    }

    async fn refresh(&self, player: &PlayerRef) -> Result<RefreshResult> {
        let kind = self.active();
        let prefix = player_key(kind, player);
        self.advance_epoch(&prefix);
        self.invalidate_cache(&prefix);
        let result = self.provider(kind).refresh(player).await?;
        if result.source != kind.as_str() || !result.cache_invalidated {
            return Err(invalid(format!(
                "{} refresh returned an invalid result",
                kind.as_str()
            )));
        }
        self.invalidate_cache(&prefix);
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
        block_first: bool,
        started: tokio::sync::Semaphore,
        release: tokio::sync::Semaphore,
    }

    impl CountingStub {
        fn new(source: &'static str) -> Self {
            Self {
                source,
                profiles: AtomicUsize::new(0),
                block_first: false,
                started: tokio::sync::Semaphore::new(0),
                release: tokio::sync::Semaphore::new(0),
            }
        }

        fn blocking(source: &'static str) -> Self {
            Self {
                source,
                profiles: AtomicUsize::new(0),
                block_first: true,
                started: tokio::sync::Semaphore::new(0),
                release: tokio::sync::Semaphore::new(0),
            }
        }
    }

    #[async_trait]
    impl PlayerStatsProvider for CountingStub {
        async fn profile(&self, player: &PlayerRef, _force: bool) -> Result<PlayerProfile> {
            let call = self.profiles.fetch_add(1, Ordering::SeqCst);
            if self.block_first && call == 0 {
                self.started.add_permits(1);
                self.release.acquire().await.unwrap().forget();
            }
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

    #[test]
    fn ttl_distinguishes_fresh_and_expired_entries_without_waiting() {
        let profile = PlayerProfile {
            source: "fixture".into(),
            identity: PlayerIdentity {
                platform_id: "JP1".into(),
                game_name: "Player".into(),
                tag_line: "JP1".into(),
                puuid: None,
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
        };
        assert!(fresh(Some(&(Instant::now(), profile.clone()))).is_some());
        let expired_at = Instant::now()
            .checked_sub(CACHE_TTL + Duration::from_millis(1))
            .expect("test instant supports five-minute subtraction");
        assert!(fresh(Some(&(expired_at, profile))).is_none());
    }

    #[test]
    fn player_contract_validation_rejects_wrong_sources_and_invalid_statistics() {
        let invalid_profile = PlayerProfile {
            source: "opgg".into(),
            identity: PlayerIdentity {
                platform_id: "KR".into(),
                game_name: "Player".into(),
                tag_line: "KR1".into(),
                puuid: None,
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
        };
        assert!(matches!(
            validate_profile(ProviderKind::Deeplol, &invalid_profile),
            Err(ProviderError::InvalidData(_))
        ));

        let invalid_champion = PlayerChampionStats {
            source: "deeplol".into(),
            champion_id: 103,
            games: 2,
            wins: 3,
            losses: 0,
            win_rate: 1.5,
            kda: Some(f64::NAN),
            cs_per_minute: None,
            role: Some("Middle".into()),
            queue: "RANKED_SOLO_5x5".into(),
            extras: ProviderExtras::None,
        };
        assert!(matches!(
            validate_champions(ProviderKind::Deeplol, &[invalid_champion]),
            Err(ProviderError::InvalidData(_))
        ));
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
                    ProviderKind::Opgg,
                    Arc::new(Stub("opgg")) as Arc<dyn PlayerStatsProvider>,
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
        proxy.set_active(ProviderKind::Opgg).expect("switch");
        assert_eq!(
            proxy
                .profile(&player(), false)
                .await
                .unwrap_err()
                .to_string(),
            "opgg"
        );
        assert_eq!(proxy.available().len(), 2);
    }

    #[tokio::test]
    async fn coalesces_duplicates_and_keeps_provider_caches_separate() {
        let deep = Arc::new(CountingStub::new("deeplol"));
        let opgg = Arc::new(CountingStub::new("opgg"));
        let proxy = PlayerStatsProxy::new(
            ProviderKind::Deeplol,
            [
                (
                    ProviderKind::Deeplol,
                    deep.clone() as Arc<dyn PlayerStatsProvider>,
                ),
                (
                    ProviderKind::Opgg,
                    opgg.clone() as Arc<dyn PlayerStatsProvider>,
                ),
            ],
        )
        .unwrap();

        let identity = player();
        let (first, second) = tokio::join!(
            proxy.profile(&identity, false),
            proxy.profile(&identity, false)
        );
        assert_eq!(first.unwrap().source, "deeplol");
        assert_eq!(second.unwrap().source, "deeplol");
        assert_eq!(deep.profiles.load(Ordering::SeqCst), 1);
        assert!(proxy.inflight.lock().await.is_empty());

        proxy.set_active(ProviderKind::Opgg).unwrap();
        assert_eq!(
            proxy.profile(&identity, false).await.unwrap().source,
            "opgg"
        );
        assert_eq!(opgg.profiles.load(Ordering::SeqCst), 1);

        let (first_force, second_force) = tokio::join!(
            proxy.profile(&identity, true),
            proxy.profile(&identity, true)
        );
        first_force.unwrap();
        second_force.unwrap();
        assert_eq!(opgg.profiles.load(Ordering::SeqCst), 2);
        assert!(proxy.inflight.lock().await.is_empty());
        proxy.refresh(&identity).await.unwrap();
        proxy.profile(&identity, false).await.unwrap();
        assert_eq!(opgg.profiles.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn refresh_prevents_an_older_request_from_repopulating_the_cache() {
        let provider = Arc::new(CountingStub::blocking("deeplol"));
        let proxy = Arc::new(
            PlayerStatsProxy::new(
                ProviderKind::Deeplol,
                [(
                    ProviderKind::Deeplol,
                    provider.clone() as Arc<dyn PlayerStatsProvider>,
                )],
            )
            .unwrap(),
        );
        let identity = player();
        let pending = {
            let proxy = proxy.clone();
            let identity = identity.clone();
            tokio::spawn(async move { proxy.profile(&identity, false).await })
        };
        provider.started.acquire().await.unwrap().forget();
        proxy.refresh(&identity).await.unwrap();
        provider.release.add_permits(1);
        pending.await.unwrap().unwrap();

        proxy.profile(&identity, false).await.unwrap();
        assert_eq!(provider.profiles.load(Ordering::SeqCst), 2);
        assert!(proxy.inflight.lock().await.is_empty());
    }

    #[test]
    fn rejects_build_only_player_registration_even_with_a_trait_stub() {
        let result = PlayerStatsProxy::new(
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
        );
        assert!(matches!(
            result,
            Err(ProviderError::InvalidPlayerRequest(_))
        ));
    }
}
