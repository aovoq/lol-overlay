//! Real data layer backed by **DeepLoL**'s public CDN API
//! (`b2c-api-cdn.deeplol.gg`, CORS-open, no auth) plus Riot's **Data Dragon**.
//!
//! * Item / rune recommendations come from `/champion/build`, which returns the
//!   most-picked build per lane at a given patch & rank tier.
//! * The OPENLOL champ-select panel additionally uses `/champion/rank` (per-role
//!   tier list), the build's `match_up` block (counters), and `/matchup/*`
//!   (matchup stats + raw one-trick games). DeepLoL has **no aggregated
//!   matchup-rune endpoint**, so the "VS enemy" rune page is aggregated here
//!   from individual recent games.
//! * Champion-name→id and item-id→name come from Data Dragon — the same CDN the
//!   frontend already uses, so the icon versions line up.
//!
//! Everything is cached for the process lifetime: the static maps once, and
//! each champion's build / lane tier list / matchup page on first use. The
//! in-game poller calls [`items`](DeepLolProvider::items) every couple of
//! seconds and the champ-select UI re-invokes commands on every render, so
//! hitting the network each time is not an option.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use overlay_ddragon::{normalize, ChampionMaps, DdragonClient};
use overlay_provider::{
    counter_entries_from_subject_losses, item_recommendations, rune_recommendation, BuildProvider,
    CounterEntry, ItemRecommendation, ProviderError, Result, RuneBuild, RuneRecommendation,
    SkillOrder, TierEntry, MIN_MATCHUP_GAMES,
};
use overlay_types::GameSnapshot;
use tokio::sync::RwLock;

mod player;
mod runes;
mod types;

use runes::{aggregate_otp, normalize_stat_shards};
use types::{
    BuildEntry, BuildResponse, LaneBuild, MatchupStatsResponse, OtpEntry, OtpResponse,
    RankResponse, VersionResponse,
};

const DEEPLOL: &str = "https://b2c-api-cdn.deeplol.gg";

/// …and at least this many usable rune samples from `/matchup/OTP_match`.
const MIN_RUNE_SAMPLES: usize = 3;
const CACHE_TTL: Duration = Duration::from_hours(6);
const RETRY_ATTEMPTS: usize = 2;
const RETRY_DELAY: Duration = Duration::from_millis(250);

pub struct DeepLolProvider {
    http: reqwest::Client,
    ddragon: Arc<DdragonClient>,
    /// Region for the stat queries. DeepLoL wants a *numbered* platform id
    /// (`JP1`, `NA1`, `EUW1`, …); `KR` is the one exception. The build numbers
    /// barely move between regions, so a high-population default is fine.
    /// NOTE: `/champion/rank` is the odd one out — it only answers for `KR`
    /// (anything else → HTTP 500), so the tier list always queries KR.
    /// Behind a lock because the real region arrives from the LCU after
    /// startup (`set_platform_id`).
    platform_id: std::sync::RwLock<String>,
    /// Rank bracket the builds are aggregated over. Must be one of
    /// `Emerald+`/`Diamond+`/`Master+` — other values silently return an
    /// Aram-only zeroed dataset from `/champion/rank`.
    tier: String,
    cache: RwLock<Cache>,
    player_cache: RwLock<player::PlayerCache>,
}

#[derive(Default)]
struct Cache {
    loaded_at: Option<Instant>,
    /// DeepLoL build patch, e.g. `"16.11"` — the latest patch that actually has
    /// build data (the newest *released* patch can lag a day or two).
    patch: Option<String>,
    /// The patch before that (`game_version_list[1]`), used only to compute the
    /// tier list's win-rate delta arrows.
    prev_patch: Option<String>,
    champions: Option<Arc<ChampionMaps>>,
    items: Option<Arc<HashMap<i64, String>>>,
    /// champion_id → its `/champion/build` response (cached for the session).
    builds: HashMap<i64, Arc<BuildResponse>>,
    /// game_version → `/champion/rank` response (current + previous patch).
    ranks: HashMap<String, Arc<RankResponse>>,
    /// DeepLoL lane → computed tier list. The UI may re-invoke per render, so
    /// the rank fetches + games calibration happen once per lane.
    tier_lists: HashMap<String, Vec<TierEntry>>,
    /// (champion, enemy, lane) → matchup rune page. `None` records a definitive
    /// NotEnoughData verdict so thin matchups aren't re-fetched on every
    /// render; transient network failures are deliberately NOT cached.
    matchup_builds: HashMap<(i64, i64, String), Option<RuneBuild>>,
}

impl DeepLolProvider {
    pub fn new(ddragon: Arc<DdragonClient>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            // DeepLoL's CDN 403s requests with no User-Agent (reqwest sends none
            // by default), so present a browser-ish one. curl works only because
            // it always sends `curl/x`.
            .user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
            )
            .build()?;
        Ok(Self {
            http,
            ddragon,
            platform_id: std::sync::RwLock::new("KR".into()),
            tier: "Emerald+".into(),
            cache: RwLock::new(Cache::default()),
            player_cache: RwLock::new(player::PlayerCache::default()),
        })
    }

    /// Populate the patch + name maps once. Idempotent; cheap after the first
    /// call. A concurrent double-load is harmless (the work is the same).
    async fn ensure_static(&self) -> Result<()> {
        {
            let c = self.cache.read().await;
            if c.patch.is_some()
                && c.champions.is_some()
                && c.loaded_at
                    .is_some_and(|loaded_at| loaded_at.elapsed() < CACHE_TTL)
            {
                return Ok(());
            }
        }
        let versions = self.fetch_versions().await?;
        let patch = versions
            .first()
            .cloned()
            .ok_or_else(|| ProviderError::Other("DeepLoL returned no game versions".into()))?;
        let champions = self
            .ddragon
            .champions()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        let items = self
            .ddragon
            .items()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;

        let mut c = self.cache.write().await;
        c.loaded_at = Some(Instant::now());
        c.patch = Some(patch);
        c.prev_patch = versions.get(1).cloned();
        c.champions = Some(champions);
        c.items = Some(items);
        c.builds.clear();
        c.ranks.clear();
        c.tier_lists.clear();
        c.matchup_builds.clear();
        Ok(())
    }

    /// Resolve an English champion name (e.g. `"Talon"`) to its numeric id.
    async fn champion_id(&self, raw_name: &str) -> Result<Option<i64>> {
        if raw_name.is_empty() {
            return Ok(None);
        }
        self.ensure_static().await?;
        let c = self.cache.read().await;
        Ok(c.champions
            .as_ref()
            .and_then(|champions| champions.name_to_id.get(&normalize(raw_name)).copied()))
    }

    /// Display name for a champion id (`"Cho'Gath"`), for rune-page names.
    /// Falls back to the bare id so a missing map never blocks an import.
    async fn champion_name(&self, champion_id: i64) -> String {
        if self.ensure_static().await.is_err() {
            return champion_id.to_string();
        }
        self.cache
            .read()
            .await
            .champions
            .as_ref()
            .and_then(|c| c.id_to_name.get(&champion_id))
            .cloned()
            .unwrap_or_else(|| champion_id.to_string())
    }

    /// Fetch (and cache) a champion's build for the session.
    async fn get_build(&self, champion_id: i64) -> Result<Arc<BuildResponse>> {
        {
            let c = self.cache.read().await;
            if let Some(b) = c.builds.get(&champion_id) {
                return Ok(b.clone());
            }
        }
        self.ensure_static().await?;
        let patch = self
            .cache
            .read()
            .await
            .patch
            .clone()
            .ok_or_else(|| ProviderError::Other("no build patch available".into()))?;

        let resp = Arc::new(self.fetch_resolved_build(champion_id, &patch).await?);
        self.cache
            .write()
            .await
            .builds
            .insert(champion_id, resp.clone());
        Ok(resp)
    }

    async fn fetch_resolved_build(&self, champion_id: i64, patch: &str) -> Result<BuildResponse> {
        let platform_id = self.platform_id.read().unwrap().clone();
        let build = self
            .fetch_build_for_platform(champion_id, patch, &platform_id)
            .await?;
        if has_build_data(&build) || platform_id == "KR" {
            return Ok(build);
        }

        // `/champion/rank` is already pinned to KR. Some low-volume regions
        // return `{ "build_by_lane": {} }` for champions that are present in
        // that KR tier list, so fall back to KR build data instead of showing
        // an empty/error panel.
        self.fetch_build_for_platform(champion_id, patch, "KR")
            .await
    }

    async fn fetch_build_for_platform(
        &self,
        champion_id: i64,
        patch: &str,
        platform_id: &str,
    ) -> Result<BuildResponse> {
        // NOTE: `language` is deliberately omitted — DeepLoL returns an empty
        // body for `/champion/build` when it is present.
        self.http
            .get(format!("{DEEPLOL}/champion/build"))
            .query(&[
                ("champion_id", champion_id.to_string()),
                ("game_version", patch.to_string()),
                ("platform_id", platform_id.to_string()),
                ("tier", self.tier.clone()),
            ])
            .send_with_retry()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(Into::into)
    }

    /// Fetch (and cache) the `/champion/rank` payload for one game version.
    /// Pinned to `platform_id=KR` regardless of [`Self::platform_id`] — every
    /// other platform id makes the endpoint return HTTP 500.
    async fn get_rank(&self, game_version: &str) -> Result<Arc<RankResponse>> {
        {
            let c = self.cache.read().await;
            if let Some(r) = c.ranks.get(game_version) {
                return Ok(r.clone());
            }
        }
        let resp: RankResponse = self
            .http
            .get(format!("{DEEPLOL}/champion/rank"))
            .query(&[
                ("platform_id", "KR"),
                ("tier", self.tier.as_str()),
                ("game_version", game_version),
            ])
            .send_with_retry()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let arc = Arc::new(resp);
        self.cache
            .write()
            .await
            .ranks
            .insert(game_version.to_string(), arc.clone());
        Ok(arc)
    }

    /// `/champion/rank` reports `games: 0` for every champion, so absolute
    /// game counts have to be estimated. One `/champion/build` fetch (cached)
    /// for the lane's most-picked champion gives
    /// `lane.games / lane.pick_rate ≈ total games played in the lane`; each
    /// row's games is then `pick_rate × total`. `None` = calibration failed
    /// (rows keep `games: 0` and the UI falls back to showing pick rate).
    async fn calibrate_lane_games(&self, lane: &str, rows: &[TierEntry]) -> Option<f64> {
        let anchor = rows.iter().max_by(|a, b| {
            a.pick_rate
                .partial_cmp(&b.pick_rate)
                .unwrap_or(Ordering::Equal)
        })?;
        let Ok(build) = self.get_build(anchor.champion_id).await else {
            return None;
        };
        let lb = build.build_by_lane.get(lane)?;
        if lb.games <= 0 || lb.pick_rate <= 0.0 {
            return None;
        }
        Some(lb.games as f64 / lb.pick_rate)
    }

    /// The single-champion (no enemy) rune page, straight from the most-picked
    /// build entry of the chosen lane.
    async fn solo_rune_build(&self, champion_id: i64, role: Option<&str>) -> Result<RuneBuild> {
        let build = self.get_build(champion_id).await?;
        if let Some((lane, entry)) = pick(&build, role) {
            return self.rune_build_from_entry(champion_id, lane, entry).await;
        }

        let prev_patch = self.cache.read().await.prev_patch.clone();
        if let Some(prev_patch) = prev_patch {
            let previous = self.fetch_resolved_build(champion_id, &prev_patch).await?;
            if let Some((lane, entry)) = pick(&previous, role) {
                return self.rune_build_from_entry(champion_id, lane, entry).await;
            }
        }

        Err(ProviderError::Other("no rune data for champion".into()))
    }

    async fn rune_build_from_entry(
        &self,
        champion_id: i64,
        lane: &str,
        entry: &BuildEntry,
    ) -> Result<RuneBuild> {
        let r = &entry.rune;

        // DeepLoL lays runes out as [style, keystone, perks…] for the primary
        // and [style, perks…] for the secondary; stat shards are their own list.
        let primary_style = r.main_build.first().copied().unwrap_or(0);
        let sub_style = r.sub_build.first().copied().unwrap_or(0);
        let primary_perks: Vec<i64> = r.main_build.iter().skip(1).copied().collect();
        let sub_perks: Vec<i64> = r.sub_build.iter().skip(1).copied().collect();
        let shards = normalize_stat_shards(&r.stat_build);

        if primary_style == 0
            || sub_style == 0
            || primary_perks.len() != 4
            || sub_perks.len() != 2
            || shards.len() != 3
        {
            return Err(ProviderError::Other("incomplete rune data".into()));
        }
        let name = self.champion_name(champion_id).await;
        Ok(RuneBuild {
            page_name: format!("OPENLOL {name} {lane}"),
            lane: lane.to_string(),
            win_rate: entry.win_rate,
            games: entry.games,
            primary_style_id: primary_style,
            sub_style_id: sub_style,
            primary_perk_ids: primary_perks,
            sub_perk_ids: sub_perks,
            shard_ids: shards,
            spell_ids: entry.spell.build.clone(),
            matchup: false,
        })
    }

    /// A rune page tuned against a specific enemy. DeepLoL has no aggregated
    /// endpoint for this, so we pull the most recent one-trick games of the
    /// matchup (`/matchup/OTP_match`, 10 per page × 2 pages) and build a
    /// consensus page out of them; `/matchup/matchup_stats` provides the
    /// headline win rate / games and acts as the data-volume gate.
    async fn matchup_rune_build(
        &self,
        champion_id: i64,
        role: Option<&str>,
        enemy_champion_id: i64,
    ) -> Result<RuneBuild> {
        // Lane resolution mirrors the solo path: my own build data decides
        // where I actually play when `role` is unknown or off-meta.
        let lane = {
            let my_build = self.get_build(champion_id).await?;
            pick_lane(&my_build, role)
                .map(|(l, _)| l.to_string())
                .ok_or_else(|| ProviderError::Other("no lane data for champion".into()))?
        };

        let key = (champion_id, enemy_champion_id, lane.clone());
        {
            let c = self.cache.read().await;
            if let Some(cached) = c.matchup_builds.get(&key) {
                return match cached {
                    Some(b) => Ok(b.clone()),
                    None => Err(ProviderError::NotEnoughData),
                };
            }
        }

        // Gate on the matchup's sample size. NOTE: an invalid pair returns 200
        // with `"stats_by_position": null` (handled by `null_default` → empty
        // map), and win rates here are PERCENT 0–100, unlike everywhere else.
        let stats: MatchupStatsResponse = self
            .http
            .get(format!("{DEEPLOL}/matchup/matchup_stats"))
            .query(&[
                ("champion_id", champion_id.to_string()),
                ("enemy_champion_id", enemy_champion_id.to_string()),
            ])
            .send_with_retry()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let Some(pos) = stats
            .stats_by_position
            .get(&lane)
            .filter(|p| p.games >= MIN_MATCHUP_GAMES)
            .cloned()
        else {
            self.cache.write().await.matchup_builds.insert(key, None);
            return Err(ProviderError::NotEnoughData);
        };

        let mut samples: Vec<OtpEntry> = Vec::new();
        for page in 1..=2 {
            let resp: OtpResponse = self
                .http
                .get(format!("{DEEPLOL}/matchup/OTP_match"))
                .query(&[
                    ("champion_id", champion_id.to_string()),
                    ("enemy_champion_id", enemy_champion_id.to_string()),
                    ("page", page.to_string()),
                    ("player_type", "all".to_string()),
                ])
                .send_with_retry()
                .await?
                .error_for_status()?
                .json()
                .await?;
            samples.extend(resp.match_up_list);
        }
        let filtered: Vec<&OtpEntry> = samples
            .iter()
            .filter(|m| m.position == lane && m.rune.is_complete())
            .collect();
        if filtered.len() < MIN_RUNE_SAMPLES {
            self.cache.write().await.matchup_builds.insert(key, None);
            return Err(ProviderError::NotEnoughData);
        }
        let page = aggregate_otp(&filtered).ok_or(ProviderError::NotEnoughData)?;

        let me = self.champion_name(champion_id).await;
        let foe = self.champion_name(enemy_champion_id).await;
        let build = RuneBuild {
            page_name: format!("OPENLOL {me} vs {foe}"),
            lane,
            win_rate: pos.my_win_rate / 100.0, // percent → fraction
            games: pos.games,
            primary_style_id: page.primary_style,
            sub_style_id: page.sub_style,
            primary_perk_ids: page.primary_perks,
            sub_perk_ids: page.sub_perks,
            shard_ids: page.shards,
            spell_ids: page.spells,
            matchup: true,
        };
        self.cache
            .write()
            .await
            .matchup_builds
            .insert(key, Some(build.clone()));
        Ok(build)
    }

    async fn fetch_versions(&self) -> Result<Vec<String>> {
        let platform_id = self.platform_id.read().unwrap().clone();
        let v: VersionResponse = self
            .http
            .get(format!("{DEEPLOL}/champion/version"))
            .query(&[("cnt", "3"), ("platform_id", platform_id.as_str())])
            .send_with_retry()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(v.game_version_list)
    }
}

#[async_trait]
impl BuildProvider for DeepLolProvider {
    fn set_platform_id(&self, platform_id: &str) {
        let mut current = self.platform_id.write().unwrap();
        if *current == platform_id {
            return;
        }
        *current = platform_id.to_string();
        drop(current);
        // Builds already cached were fetched for the old region; drop them so
        // the next lookup re-queries. (Blocking try_write is fine here — a
        // miss just means a racing reader keeps the old data one more round.)
        if let Ok(mut cache) = self.cache.try_write() {
            cache.builds.clear();
            cache.matchup_builds.clear();
        }
    }

    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
        let id = self
            .champion_id(&snapshot.self_raw_name)
            .await?
            .ok_or_else(|| {
                ProviderError::Other(format!("unknown champion: {:?}", snapshot.self_champion))
            })?;
        let build = self.get_build(id).await?;
        let (_, entry) = pick(&build, Some(&snapshot.self_position))
            .ok_or_else(|| ProviderError::Other("no build data for champion".into()))?;

        let wr = entry.win_rate * 100.0;
        let guard = self.cache.read().await; // no awaits past here
        let recs = item_recommendations(
            entry.item.build.iter().copied(),
            |item_id| {
                guard
                    .items
                    .as_ref()
                    .and_then(|items| items.get(&item_id).cloned())
                    .unwrap_or_else(|| format!("Item {item_id}"))
            },
            wr,
            entry.games,
        );
        if recs.is_empty() {
            return Err(ProviderError::Other("build had no items".into()));
        }
        Ok(recs)
    }

    async fn skill_order(&self, snapshot: &GameSnapshot) -> Result<SkillOrder> {
        let id = self
            .champion_id(&snapshot.self_raw_name)
            .await?
            .ok_or_else(|| {
                ProviderError::Other(format!("unknown champion: {:?}", snapshot.self_champion))
            })?;
        let build = self.get_build(id).await?;
        let (_, entry) = pick(&build, Some(&snapshot.self_position))
            .ok_or_else(|| ProviderError::Other("no skill data for champion".into()))?;

        let max_order: Vec<i64> = entry
            .skill
            .build
            .iter()
            .copied()
            .filter(|id| matches!(id, 1..=4))
            .collect();
        let level_order: Vec<i64> = entry
            .skill
            .detail
            .iter()
            .copied()
            .filter(|id| matches!(id, 1..=4))
            .collect();

        if max_order.is_empty() && level_order.is_empty() {
            return Err(ProviderError::Other("build had no skill order".into()));
        }

        Ok(SkillOrder {
            max_order,
            level_order,
            win_rate: entry.skill.win_rate,
            games: entry.skill.games,
        })
    }

    /// The auto-import path's flat page. Thin shim over [`Self::rune_build`]
    /// so the two never disagree: the LCU page wants one flat list
    /// [keystone, primary perks…, secondary perks…, stat shards].
    async fn runes(&self, champion_id: i64, role: Option<&str>) -> Result<RuneRecommendation> {
        Ok(rune_recommendation(
            "DeepLoL",
            self.rune_build(champion_id, role, None).await?,
        ))
    }

    async fn tier_list(&self, role: &str) -> Result<Vec<TierEntry>> {
        let lane = deeplol_lane(Some(role))
            .ok_or_else(|| ProviderError::Other(format!("unknown role: {role:?}")))?;
        {
            let c = self.cache.read().await;
            if let Some(rows) = c.tier_lists.get(lane) {
                return Ok(rows.clone());
            }
        }
        self.ensure_static().await?;
        let (patch, prev_patch) = {
            let c = self.cache.read().await;
            (
                c.patch
                    .clone()
                    .ok_or_else(|| ProviderError::Other("no build patch available".into()))?,
                c.prev_patch.clone(),
            )
        };
        let now = self.get_rank(&patch).await?;
        // The previous patch only feeds the delta arrows — losing it must not
        // take down the whole tier list (deltas just read 0.0 = unknown).
        let mut prev = None;
        if let Some(pp) = prev_patch {
            if let Ok(r) = self.get_rank(&pp).await {
                prev = Some(r);
            }
        }
        let mut rows = tier_rows(&now, prev.as_deref(), lane);
        if let Some(total) = self.calibrate_lane_games(lane, &rows).await {
            for r in &mut rows {
                r.games = (r.pick_rate * total).round() as i64;
            }
        }
        self.cache
            .write()
            .await
            .tier_lists
            .insert(lane.to_string(), rows.clone());
        Ok(rows)
    }

    async fn counters(&self, champion_id: i64, role: &str) -> Result<Vec<CounterEntry>> {
        let build = self.get_build(champion_id).await?;
        let (_, lb) = pick_lane(&build, Some(role))
            .ok_or_else(|| ProviderError::Other("no lane data for champion".into()))?;
        let counters = counter_entries(lb);
        if !counters.is_empty() {
            return Ok(counters);
        }

        // New DeepLoL patches can appear before matchup counts have enough
        // volume. Keep the rest of the build on the current patch, but fall
        // back one patch for counters instead of showing an empty strip.
        let prev_patch = self.cache.read().await.prev_patch.clone();
        let Some(prev_patch) = prev_patch else {
            return Ok(counters);
        };
        let previous = self.fetch_resolved_build(champion_id, &prev_patch).await?;
        let Some((_, lb)) = pick_lane(&previous, Some(role)) else {
            return Ok(counters);
        };
        Ok(counter_entries(lb))
    }

    async fn rune_build(
        &self,
        champion_id: i64,
        role: Option<&str>,
        enemy_champion_id: Option<i64>,
    ) -> Result<RuneBuild> {
        match enemy_champion_id {
            None => self.solo_rune_build(champion_id, role).await,
            Some(enemy) => self.matchup_rune_build(champion_id, role, enemy).await,
        }
    }

    async fn champion_names(&self, champion_id: i64) -> Option<(String, String)> {
        self.ensure_static().await.ok()?;
        let c = self.cache.read().await;
        let champs = c.champions.as_ref()?;
        Some((
            champs.id_to_name.get(&champion_id)?.clone(),
            champs.id_to_image.get(&champion_id)?.clone(),
        ))
    }
}

trait RequestBuilderRetryExt {
    async fn send_with_retry(self) -> std::result::Result<reqwest::Response, reqwest::Error>;
}

impl RequestBuilderRetryExt for reqwest::RequestBuilder {
    async fn send_with_retry(self) -> std::result::Result<reqwest::Response, reqwest::Error> {
        let request = self;
        let mut attempt = 0;
        loop {
            let Some(next) = request.try_clone() else {
                return request.send().await;
            };
            match next.send().await {
                Ok(response) if response.status().is_server_error() && attempt < RETRY_ATTEMPTS => {
                    attempt += 1;
                    tokio::time::sleep(RETRY_DELAY * attempt as u32).await;
                }
                Err(err)
                    if (err.is_connect() || err.is_timeout() || err.is_request())
                        && attempt < RETRY_ATTEMPTS =>
                {
                    attempt += 1;
                    tokio::time::sleep(RETRY_DELAY * attempt as u32).await;
                }
                result => return result,
            }
        }
    }
}

/// LCU/Live-Client position string → DeepLoL lane key.
fn deeplol_lane(role: Option<&str>) -> Option<&'static str> {
    Some(match role?.to_ascii_lowercase().as_str() {
        "top" => "Top",
        "jungle" => "Jungle",
        "middle" | "mid" => "Middle",
        "bottom" | "bot" | "adc" => "Bot",
        "utility" | "support" | "supporter" => "Supporter",
        _ => return None,
    })
}

/// Pick the lane data for a role: the requested lane if it has builds,
/// otherwise the champion's most-played lane (Aram last).
fn pick_lane<'a>(build: &'a BuildResponse, role: Option<&str>) -> Option<(&'a str, &'a LaneBuild)> {
    if let Some(lane) = deeplol_lane(role) {
        if let Some(lb) = build.build_by_lane.get(lane) {
            if !lb.build_lst.is_empty() {
                return Some((lane, lb));
            }
        }
    }
    let mut lanes: Vec<(&String, &LaneBuild)> = build.build_by_lane.iter().collect();
    // Most games first; Aram demoted so it's only a last resort.
    lanes.sort_by_key(|(k, v)| (k.as_str() == "Aram", -v.games));
    lanes
        .into_iter()
        .find(|(_, v)| !v.build_lst.is_empty())
        .map(|(k, v)| (k.as_str(), v))
}

/// [`pick_lane`] narrowed to the lane's top build entry.
fn pick<'a>(build: &'a BuildResponse, role: Option<&str>) -> Option<(&'a str, &'a BuildEntry)> {
    pick_lane(build, role).and_then(|(lane, lb)| lb.build_lst.first().map(|e| (lane, e)))
}

fn has_build_data(build: &BuildResponse) -> bool {
    build
        .build_by_lane
        .values()
        .any(|lane| !lane.build_lst.is_empty())
}

/// Shape one lane's tier list from the rank payload(s). A champion belongs to
/// a lane iff its `win_rate > 0` there; fringe picks (< 0.5% pick rate) are
/// dropped; the delta is win-rate movement vs the previous patch in percentage
/// points (0.0 = champion missing from the previous patch); rows are sorted by
/// win rate desc. `games` stays 0 here — it's calibrated separately.
fn tier_rows(now: &RankResponse, prev: Option<&RankResponse>, lane: &str) -> Vec<TierEntry> {
    let prev_wr: HashMap<i64, f64> = prev
        .map(|r| {
            r.champion_data_list
                .iter()
                .filter_map(|c| {
                    c.performance_dict
                        .get(lane)
                        .filter(|p| p.win_rate > 0.0)
                        .map(|p| (c.champion_id, p.win_rate))
                })
                .collect()
        })
        .unwrap_or_default();
    let mut rows: Vec<TierEntry> = now
        .champion_data_list
        .iter()
        .filter_map(|c| {
            let p = c.performance_dict.get(lane)?;
            if p.win_rate <= 0.0 || p.pick_rate < 0.005 {
                return None;
            }
            Some(TierEntry {
                champion_id: c.champion_id,
                win_rate: p.win_rate,
                win_rate_delta: prev_wr
                    .get(&c.champion_id)
                    .map_or(0.0, |w| (p.win_rate - w) * 100.0),
                games: 0,
                pick_rate: p.pick_rate,
                ban_rate: p.ban_rate,
            })
        })
        .collect();
    rows.sort_by(|a, b| {
        b.win_rate
            .partial_cmp(&a.win_rate)
            .unwrap_or(Ordering::Equal)
    });
    rows
}

/// Counters from a lane's `weak_against` list: those are the champions the
/// subject LOSES to (sorted worst-for-the-subject first = best counters
/// first), and `win_rate` is from the *subject's* perspective — invert it so
/// each entry carries the counter champion's own win rate.
fn counter_entries(lb: &LaneBuild) -> Vec<CounterEntry> {
    counter_entries_from_subject_losses(
        lb.match_up
            .weak_against
            .iter()
            .map(|m| (m.enemy_champion_id, m.win_rate, m.games)),
    )
}

#[cfg(test)]
mod tests;
