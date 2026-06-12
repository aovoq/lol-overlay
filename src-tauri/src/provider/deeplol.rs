//! Real data layer backed by **DeepLoL**'s public CDN API
//! (`b2c-api-cdn.deeplol.gg`, CORS-open, no auth) plus Riot's **Data Dragon**.
//!
//! * Item / rune recommendations come from `/champion/build`, which returns the
//!   most-picked build per lane at a given patch & rank tier.
//! * The HEXGATE champ-select panel additionally uses `/champion/rank` (per-role
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

use std::cmp::{Ordering, Reverse};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::error::{Error, Result};
use crate::live_client::GameSnapshot;
use crate::provider::{
    BuildProvider, CounterEntry, ItemRecommendation, RuneBuild, RuneRecommendation, SkillOrder,
    TierEntry,
};

const DEEPLOL: &str = "https://b2c-api-cdn.deeplol.gg";
const DDRAGON: &str = "https://ddragon.leagueoflegends.com";

/// A matchup page needs this many games behind it before we trust it; the same
/// floor is applied to individual counter matchups.
const MIN_MATCHUP_GAMES: i64 = 30;
/// …and at least this many usable rune samples from `/matchup/OTP_match`.
const MIN_RUNE_SAMPLES: usize = 3;

pub struct DeepLolProvider {
    http: reqwest::Client,
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
}

#[derive(Default)]
struct Cache {
    /// DeepLoL build patch, e.g. `"16.11"` — the latest patch that actually has
    /// build data (the newest *released* patch can lag a day or two).
    patch: Option<String>,
    /// The patch before that (`game_version_list[1]`), used only to compute the
    /// tier list's win-rate delta arrows.
    prev_patch: Option<String>,
    /// Normalized English champion name → numeric id (`"chogath" → 31`).
    name_to_id: HashMap<String, i64>,
    /// Champion id → display name (`31 → "Cho'Gath"`), for HEXGATE page names.
    id_to_name: HashMap<i64, String>,
    /// Champion id → Data Dragon image id (`31 → "Chogath"`), for the mock
    /// scenarios that synthesize Live-Client-shaped state.
    id_to_image: HashMap<i64, String>,
    /// Item id → display name, for the recommendation labels.
    item_names: HashMap<i64, String>,
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
    pub fn new() -> Result<Self> {
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
            platform_id: std::sync::RwLock::new("KR".into()),
            tier: "Emerald+".into(),
            cache: RwLock::new(Cache::default()),
        })
    }

    /// Populate the patch + name maps once. Idempotent; cheap after the first
    /// call. A concurrent double-load is harmless (the work is the same).
    async fn ensure_static(&self) -> Result<()> {
        {
            let c = self.cache.read().await;
            if c.patch.is_some() && !c.name_to_id.is_empty() {
                return Ok(());
            }
        }
        let versions = self.fetch_versions().await?;
        let patch = versions
            .first()
            .cloned()
            .ok_or_else(|| Error::Other("DeepLoL returned no game versions".into()))?;
        let ddver = self.fetch_ddragon_version().await?;
        let (name_to_id, id_to_name, id_to_image) = self.fetch_champion_map(&ddver).await?;
        let item_names = self.fetch_item_map(&ddver).await?;

        let mut c = self.cache.write().await;
        c.patch = Some(patch);
        c.prev_patch = versions.get(1).cloned();
        c.name_to_id = name_to_id;
        c.id_to_name = id_to_name;
        c.id_to_image = id_to_image;
        c.item_names = item_names;
        Ok(())
    }

    /// Resolve an English champion name (e.g. `"Talon"`) to its numeric id.
    async fn champion_id(&self, raw_name: &str) -> Option<i64> {
        if raw_name.is_empty() {
            return None;
        }
        if let Err(e) = self.ensure_static().await {
            eprintln!("deeplol: static load failed: {e}");
            return None;
        }
        let c = self.cache.read().await;
        c.name_to_id.get(&normalize(raw_name)).copied()
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
            .id_to_name
            .get(&champion_id)
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
            .ok_or_else(|| Error::Other("no build patch available".into()))?;

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
        self.fetch_build_for_platform(champion_id, patch, "KR").await
    }

    async fn fetch_build_for_platform(
        &self,
        champion_id: i64,
        patch: &str,
        platform_id: &str,
    ) -> Result<BuildResponse> {
        // NOTE: `language` is deliberately omitted — DeepLoL returns an empty
        // body for `/champion/build` when it is present.
        self
            .http
            .get(format!("{DEEPLOL}/champion/build"))
            .query(&[
                ("champion_id", champion_id.to_string()),
                ("game_version", patch.to_string()),
                ("platform_id", platform_id.to_string()),
                ("tier", self.tier.clone()),
            ])
            .send()
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
            .send()
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
        let build = match self.get_build(anchor.champion_id).await {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "deeplol: games calibration failed for champion {}: {e}",
                    anchor.champion_id
                );
                return None;
            }
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

        Err(Error::Other("no rune data for champion".into()))
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
        let shards = r.stat_build.clone();

        if primary_style == 0
            || sub_style == 0
            || primary_perks.len() + sub_perks.len() + shards.len() < 6
        {
            return Err(Error::Other("incomplete rune data".into()));
        }
        let name = self.champion_name(champion_id).await;
        Ok(RuneBuild {
            page_name: format!("HEXGATE {name} {lane}"),
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
                .ok_or_else(|| Error::Other("no lane data for champion".into()))?
        };

        let key = (champion_id, enemy_champion_id, lane.clone());
        {
            let c = self.cache.read().await;
            if let Some(cached) = c.matchup_builds.get(&key) {
                return match cached {
                    Some(b) => Ok(b.clone()),
                    None => Err(Error::NotEnoughData),
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
            .send()
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
            return Err(Error::NotEnoughData);
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
                .send()
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
            return Err(Error::NotEnoughData);
        }
        let page = aggregate_otp(&filtered).ok_or(Error::NotEnoughData)?;

        let me = self.champion_name(champion_id).await;
        let foe = self.champion_name(enemy_champion_id).await;
        let build = RuneBuild {
            page_name: format!("HEXGATE {me} vs {foe}"),
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
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(v.game_version_list)
    }

    async fn fetch_ddragon_version(&self) -> Result<String> {
        let v: Vec<String> = self
            .http
            .get(format!("{DDRAGON}/api/versions.json"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        v.into_iter()
            .next()
            .ok_or_else(|| Error::Other("Data Dragon returned no versions".into()))
    }

    /// All directions of the champion map: normalized-name → id for resolving
    /// Live Client names, id → display name for HEXGATE page labels, and
    /// id → Data Dragon image id for synthesizing mock state.
    #[allow(clippy::type_complexity)]
    async fn fetch_champion_map(
        &self,
        ddver: &str,
    ) -> Result<(
        HashMap<String, i64>,
        HashMap<i64, String>,
        HashMap<i64, String>,
    )> {
        let file: DDChampionFile = self
            .http
            .get(format!("{DDRAGON}/cdn/{ddver}/data/en_US/champion.json"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let mut by_name = HashMap::new();
        let mut by_id = HashMap::new();
        let mut by_image = HashMap::new();
        for (id_key, champ) in file.data {
            if let Ok(num) = champ.key.parse::<i64>() {
                // `id_key` matches rawChampionName ("Chogath"); `name` is the
                // display form ("Cho'Gath"). Index both, normalized.
                by_name.insert(normalize(&id_key), num);
                by_name.insert(normalize(&champ.name), num);
                by_id.insert(num, champ.name);
                by_image.insert(num, id_key);
            }
        }
        Ok((by_name, by_id, by_image))
    }

    async fn fetch_item_map(&self, ddver: &str) -> Result<HashMap<i64, String>> {
        let file: DDItemFile = self
            .http
            .get(format!("{DDRAGON}/cdn/{ddver}/data/en_US/item.json"))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let mut map = HashMap::new();
        for (id, item) in file.data {
            if let Ok(num) = id.parse::<i64>() {
                map.insert(num, item.name);
            }
        }
        Ok(map)
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
            .await
            .ok_or_else(|| Error::Other(format!("unknown champion: {:?}", snapshot.self_champion)))?;
        let build = self.get_build(id).await?;
        let (_, entry) = pick(&build, Some(&snapshot.self_position))
            .ok_or_else(|| Error::Other("no build data for champion".into()))?;

        let wr = entry.win_rate * 100.0;
        let guard = self.cache.read().await; // no awaits past here
        let mut recs = Vec::new();
        let mut seen = HashSet::new();
        for (i, &item_id) in entry.item.build.iter().enumerate() {
            if item_id == 0 || !seen.insert(item_id) {
                continue;
            }
            let name = guard
                .item_names
                .get(&item_id)
                .cloned()
                .unwrap_or_else(|| format!("Item {item_id}"));
            let reason = if i == 0 {
                format!("Core build · {wr:.0}% WR · {} games", entry.games)
            } else {
                "Core build".to_string()
            };
            recs.push(ItemRecommendation {
                item_id,
                name,
                score: (1.0 - i as f32 * 0.08).max(0.2),
                reason,
            });
        }
        if recs.is_empty() {
            return Err(Error::Other("build had no items".into()));
        }
        Ok(recs)
    }

    async fn skill_order(&self, snapshot: &GameSnapshot) -> Result<SkillOrder> {
        let id = self
            .champion_id(&snapshot.self_raw_name)
            .await
            .ok_or_else(|| Error::Other(format!("unknown champion: {:?}", snapshot.self_champion)))?;
        let build = self.get_build(id).await?;
        let (_, entry) = pick(&build, Some(&snapshot.self_position))
            .ok_or_else(|| Error::Other("no skill data for champion".into()))?;

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
            return Err(Error::Other("build had no skill order".into()));
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
        let b = self.rune_build(champion_id, role, None).await?;
        let mut perks = b.primary_perk_ids;
        perks.extend(b.sub_perk_ids);
        perks.extend(b.shard_ids);
        Ok(RuneRecommendation {
            name: format!("DeepLoL {} ({:.0}% WR)", b.lane, b.win_rate * 100.0),
            primary_style_id: b.primary_style_id,
            sub_style_id: b.sub_style_id,
            selected_perk_ids: perks,
        })
    }

    async fn tier_list(&self, role: &str) -> Result<Vec<TierEntry>> {
        let lane = deeplol_lane(Some(role))
            .ok_or_else(|| Error::Other(format!("unknown role: {role:?}")))?;
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
                    .ok_or_else(|| Error::Other("no build patch available".into()))?,
                c.prev_patch.clone(),
            )
        };
        let now = self.get_rank(&patch).await?;
        // The previous patch only feeds the delta arrows — losing it must not
        // take down the whole tier list (deltas just read 0.0 = unknown).
        let mut prev = None;
        if let Some(pp) = prev_patch {
            match self.get_rank(&pp).await {
                Ok(r) => prev = Some(r),
                Err(e) => eprintln!("deeplol: previous-patch rank failed (no wr deltas): {e}"),
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
            .ok_or_else(|| Error::Other("no lane data for champion".into()))?;
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
        Some((
            c.id_to_name.get(&champion_id)?.clone(),
            c.id_to_image.get(&champion_id)?.clone(),
        ))
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
    rows.sort_by(|a, b| b.win_rate.partial_cmp(&a.win_rate).unwrap_or(Ordering::Equal));
    rows
}

/// Counters from a lane's `weak_against` list: those are the champions the
/// subject LOSES to (sorted worst-for-the-subject first = best counters
/// first), and `win_rate` is from the *subject's* perspective — invert it so
/// each entry carries the counter champion's own win rate.
fn counter_entries(lb: &LaneBuild) -> Vec<CounterEntry> {
    lb.match_up
        .weak_against
        .iter()
        .filter(|m| m.games >= MIN_MATCHUP_GAMES)
        .take(8)
        .map(|m| CounterEntry {
            champion_id: m.enemy_champion_id,
            win_rate: 1.0 - m.win_rate,
            games: m.games,
        })
        .collect()
}

/// One consensus rune page distilled from individual OTP matchup games.
struct AggregatedPage {
    primary_style: i64,
    sub_style: i64,
    /// [keystone, p1, p2, p3]
    primary_perks: Vec<i64>,
    /// [s1, s2]
    sub_perks: Vec<i64>,
    /// [offense, flex, defense]
    shards: Vec<i64>,
    /// [spell1, spell2]; empty when no game had a usable spell pair.
    spells: Vec<i64>,
}

/// Build the consensus page: group games by (primary style, keystone) so
/// different archetypes don't blend into an invalid hybrid, then take the
/// per-slot mode inside the largest group. OTP slot layout: `perk_0` =
/// keystone, `perk_1..3` = primary minors, `perk_4..5` = secondary minors.
fn aggregate_otp(samples: &[&OtpEntry]) -> Option<AggregatedPage> {
    let mut groups: HashMap<(i64, i64), Vec<&OtpEntry>> = HashMap::new();
    for s in samples {
        groups
            .entry((s.rune.perk_primary_style, s.rune.perk_0))
            .or_default()
            .push(s);
    }
    // Largest archetype wins; ties break on the smaller key for determinism.
    let ((primary_style, keystone), group) = groups
        .into_iter()
        .max_by_key(|(k, v)| (v.len(), Reverse(*k)))?;

    // Secondary tree: mode the style first, then mode the minors only among
    // games that used that style — mixing trees per slot could otherwise
    // produce a page no client would accept (perk_4 from one tree, perk_5
    // from another).
    let sub_style = mode(group.iter().map(|s| s.rune.perk_sub_style));
    let sub_group: Vec<&&OtpEntry> = group
        .iter()
        .filter(|s| s.rune.perk_sub_style == sub_style)
        .collect();

    Some(AggregatedPage {
        primary_style,
        sub_style,
        primary_perks: vec![
            keystone,
            mode(group.iter().map(|s| s.rune.perk_1)),
            mode(group.iter().map(|s| s.rune.perk_2)),
            mode(group.iter().map(|s| s.rune.perk_3)),
        ],
        sub_perks: vec![
            mode(sub_group.iter().map(|s| s.rune.perk_4)),
            mode(sub_group.iter().map(|s| s.rune.perk_5)),
        ],
        shards: vec![
            mode(group.iter().map(|s| s.rune.stat_perk_0)),
            mode(group.iter().map(|s| s.rune.stat_perk_1)),
            mode(group.iter().map(|s| s.rune.stat_perk_2)),
        ],
        // Spells are independent of the rune archetype, so count them across
        // all of the lane's games, not just the winning rune group.
        spells: most_common_spell_pair(samples),
    })
}

/// Most common summoner-spell pair across games. Flash sits in either slot
/// depending on the player's keybind, so `[4,11]` and `[11,4]` count as the
/// same pair; the output keeps whichever orientation occurred more often.
fn most_common_spell_pair(samples: &[&OtpEntry]) -> Vec<i64> {
    // normalized (min,max) pair → (count as-is, count swapped)
    let mut counts: HashMap<(i64, i64), (usize, usize)> = HashMap::new();
    for s in samples {
        let (a, b) = (s.spell.spell_1, s.spell.spell_2);
        if a <= 0 || b <= 0 {
            continue;
        }
        let key = (a.min(b), a.max(b));
        let slot = counts.entry(key).or_default();
        if (a, b) == key {
            slot.0 += 1;
        } else {
            slot.1 += 1;
        }
    }
    let Some((key, (as_is, swapped))) = counts
        .into_iter()
        .max_by_key(|&(k, (x, y))| (x + y, Reverse(k)))
    else {
        return Vec::new();
    };
    if swapped > as_is {
        vec![key.1, key.0]
    } else {
        vec![key.0, key.1]
    }
}

/// Most frequent positive value; ties break on the smaller value for
/// determinism. 0 only when the input has no positive values.
fn mode<I: Iterator<Item = i64>>(values: I) -> i64 {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for v in values {
        if v > 0 {
            *counts.entry(v).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by_key(|&(v, c)| (c, Reverse(v)))
        .map(|(v, _)| v)
        .unwrap_or(0)
}

/// Lowercase + strip non-alphanumerics so "Cho'Gath", "Chogath" and "chogath"
/// all collapse to the same key.
fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

// ---- DeepLoL response shapes (only the fields we use) ----
//
// DeepLoL sends explicit `null` for some fields (e.g. an Aram lane's `games`,
// or the whole `stats_by_position` for an unplayed matchup). `#[serde(default)]`
// alone does NOT cover that — it only fills *absent* fields, and a present
// `null` in an `i64`/`f64` aborts the whole parse. `null_default` turns
// present-null into the type's default, so one ropey lane can't sink the
// entire response. Applied to every field for robustness against a loose API.
//
// A few parsed fields aren't consumed yet — they document the payload shape
// for the UI to pick up later; `allow(dead_code)` marks those structs.

/// Deserialize, mapping an explicit JSON `null` to `T::default()`.
fn null_default<'de, D, T>(d: D) -> std::result::Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
struct VersionResponse {
    #[serde(default, deserialize_with = "null_default")]
    game_version_list: Vec<String>,
}

// -- /champion/build --

#[derive(Debug, Deserialize)]
struct BuildResponse {
    #[serde(default, deserialize_with = "null_default")]
    build_by_lane: HashMap<String, LaneBuild>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct LaneBuild {
    #[serde(default, deserialize_with = "null_default")]
    build_lst: Vec<BuildEntry>,
    /// Real per-lane champion games — also the games-calibration numerator.
    #[serde(default, deserialize_with = "null_default")]
    games: i64,
    #[serde(default, deserialize_with = "null_default")]
    pick_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    ban_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    match_up: MatchUp,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct MatchUp {
    #[serde(default, deserialize_with = "null_default")]
    strong_against: Vec<MatchUpEntry>,
    /// Champions the subject loses to, sorted ascending by the subject's
    /// `win_rate` (worst matchup first).
    #[serde(default, deserialize_with = "null_default")]
    weak_against: Vec<MatchUpEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct MatchUpEntry {
    #[serde(default, deserialize_with = "null_default")]
    games: i64,
    /// The subject champion's win rate vs this enemy (fraction 0..1).
    #[serde(default, deserialize_with = "null_default")]
    win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    match_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    enemy_champion_id: i64,
}

#[derive(Debug, Default, Deserialize)]
struct BuildEntry {
    #[serde(default, deserialize_with = "null_default")]
    rune: RuneBlock,
    #[serde(default, deserialize_with = "null_default")]
    item: ItemBlock,
    #[serde(default, deserialize_with = "null_default")]
    spell: SpellBlock,
    #[serde(default, deserialize_with = "null_default")]
    skill: SkillBlock,
    #[serde(default, deserialize_with = "null_default")]
    win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    games: i64,
}

#[derive(Debug, Default, Deserialize)]
struct RuneBlock {
    #[serde(default, deserialize_with = "null_default")]
    main_build: Vec<i64>,
    #[serde(default, deserialize_with = "null_default")]
    sub_build: Vec<i64>,
    #[serde(default, deserialize_with = "null_default")]
    stat_build: Vec<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct ItemBlock {
    #[serde(default, deserialize_with = "null_default")]
    build: Vec<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct SpellBlock {
    /// Summoner spell ids, e.g. `[14, 4]` (Ignite + Flash).
    #[serde(default, deserialize_with = "null_default")]
    build: Vec<i64>,
}

#[derive(Debug, Default, Deserialize)]
struct SkillBlock {
    /// Basic-skill max order, e.g. `[3, 1, 2]` = E > Q > W.
    #[serde(default, deserialize_with = "null_default")]
    build: Vec<i64>,
    /// Level-by-level skill order. Riot skill ids: 1 = Q, 2 = W, 3 = E, 4 = R.
    #[serde(default, deserialize_with = "null_default")]
    detail: Vec<i64>,
    #[serde(default, deserialize_with = "null_default")]
    win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    games: i64,
}

// -- /champion/rank --

#[derive(Debug, Deserialize)]
struct RankResponse {
    #[serde(default, deserialize_with = "null_default")]
    champion_data_list: Vec<RankChampion>,
}

#[derive(Debug, Default, Deserialize)]
struct RankChampion {
    #[serde(default, deserialize_with = "null_default")]
    champion_id: i64,
    /// Keys: `Top|Jungle|Middle|Bot|Supporter|Aram|Total`.
    #[serde(default, deserialize_with = "null_default")]
    performance_dict: HashMap<String, RankPerformance>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[allow(dead_code)]
struct RankPerformance {
    /// Fraction 0..1; 0 = the champion isn't played in this lane.
    #[serde(default, deserialize_with = "null_default")]
    win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    pick_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    ban_rate: f64,
    /// 1–5, 0 = not played.
    #[serde(default, deserialize_with = "null_default")]
    tier: i64,
    #[serde(default, deserialize_with = "null_default")]
    rank: i64,
    /// Rank-position movement, NOT a win-rate delta.
    #[serde(default, deserialize_with = "null_default")]
    rank_delta: i64,
    /// Always 0 in practice — unusable; see games calibration.
    #[serde(default, deserialize_with = "null_default")]
    games: i64,
}

// -- /matchup/matchup_stats --

#[derive(Debug, Deserialize)]
struct MatchupStatsResponse {
    /// Whole node is JSON `null` for an invalid/unplayed pair.
    #[serde(default, deserialize_with = "null_default")]
    stats_by_position: HashMap<String, PositionStats>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[allow(dead_code)]
struct PositionStats {
    #[serde(default, deserialize_with = "null_default")]
    games: i64,
    /// PERCENT 0–100 — the one DeepLoL payload that isn't a fraction.
    #[serde(default, deserialize_with = "null_default")]
    my_win_rate: f64,
    #[serde(default, deserialize_with = "null_default")]
    enemy_win_rate: f64,
}

// -- /matchup/OTP_match --

#[derive(Debug, Deserialize)]
struct OtpResponse {
    #[serde(default, deserialize_with = "null_default")]
    match_up_list: Vec<OtpEntry>,
}

#[derive(Debug, Default, Deserialize)]
#[allow(dead_code)]
struct OtpEntry {
    #[serde(default, deserialize_with = "null_default")]
    position: String,
    /// 0 | 1.
    #[serde(default, deserialize_with = "null_default")]
    win: i64,
    #[serde(default, deserialize_with = "null_default")]
    rune: OtpRune,
    #[serde(default, deserialize_with = "null_default")]
    spell: OtpSpell,
}

/// One game's full rune page. Slot layout: `perk_0` = keystone, `perk_1..3` =
/// primary minors, `perk_4..5` = secondary minors.
#[derive(Debug, Default, Deserialize)]
struct OtpRune {
    #[serde(default, deserialize_with = "null_default")]
    perk_0: i64,
    #[serde(default, deserialize_with = "null_default")]
    perk_1: i64,
    #[serde(default, deserialize_with = "null_default")]
    perk_2: i64,
    #[serde(default, deserialize_with = "null_default")]
    perk_3: i64,
    #[serde(default, deserialize_with = "null_default")]
    perk_4: i64,
    #[serde(default, deserialize_with = "null_default")]
    perk_5: i64,
    #[serde(default, deserialize_with = "null_default")]
    perk_primary_style: i64,
    #[serde(default, deserialize_with = "null_default")]
    perk_sub_style: i64,
    #[serde(default, deserialize_with = "null_default")]
    stat_perk_0: i64,
    #[serde(default, deserialize_with = "null_default")]
    stat_perk_1: i64,
    #[serde(default, deserialize_with = "null_default")]
    stat_perk_2: i64,
}

impl OtpRune {
    /// True when every slot is filled — a partial block (missing perks in old
    /// games, API holes mapped to 0 by `null_default`) would poison the
    /// per-slot mode, so such games are excluded from aggregation.
    fn is_complete(&self) -> bool {
        self.perk_primary_style > 0
            && self.perk_sub_style > 0
            && [
                self.perk_0,
                self.perk_1,
                self.perk_2,
                self.perk_3,
                self.perk_4,
                self.perk_5,
                self.stat_perk_0,
                self.stat_perk_1,
                self.stat_perk_2,
            ]
            .iter()
            .all(|&p| p > 0)
    }
}

#[derive(Debug, Default, Deserialize)]
struct OtpSpell {
    #[serde(default, deserialize_with = "null_default")]
    spell_1: i64,
    #[serde(default, deserialize_with = "null_default")]
    spell_2: i64,
}

// ---- Data Dragon name maps ----

#[derive(Debug, Deserialize)]
struct DDChampionFile {
    data: HashMap<String, DDChampion>,
}

#[derive(Debug, Deserialize)]
struct DDChampion {
    /// Numeric id, encoded as a string in Data Dragon.
    key: String,
    name: String,
}

#[derive(Debug, Deserialize)]
struct DDItemFile {
    data: HashMap<String, DDItem>,
}

#[derive(Debug, Deserialize)]
struct DDItem {
    name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_games_lane_survives_parse() {
        // Regression: an Aram lane with `"games": null` used to abort the whole
        // `/champion/build` parse, leaving the UI with no items at all.
        let json = r#"{
          "build_by_lane": {
            "Middle": {"games": 100, "build_lst": [
              {"win_rate": 0.53, "games": 80,
               "rune": {"main_build": [8000,8010,9111,9105,8299],
                        "sub_build": [8400,8473,8453],
                        "stat_build": [5001,5008,5008]},
               "item": {"build": [6692,3047,0,3814]}}
            ]},
            "Aram": {"games": null, "build_lst": []}
          }
        }"#;
        let b: BuildResponse = serde_json::from_str(json).expect("null games must not abort parse");
        let (lane, e) = pick(&b, Some("MIDDLE")).expect("Middle build should be picked");
        assert_eq!(lane, "Middle");
        assert_eq!(e.item.build, vec![6692, 3047, 0, 3814]);
        // Rune flatten is keystone+3 / +2 / +3 shards = 9 perks (LCU page size).
        let perks = (e.rune.main_build.len() - 1) + (e.rune.sub_build.len() - 1)
            + e.rune.stat_build.len();
        assert_eq!(perks, 9);
    }

    #[test]
    fn normalize_collapses_punctuation_and_case() {
        assert_eq!(normalize("Cho'Gath"), "chogath");
        assert_eq!(normalize("Chogath"), "chogath");
        assert_eq!(normalize("Kai'Sa"), "kaisa");
    }

    #[test]
    fn rank_response_parses_and_shapes_tier_rows() {
        // `rank_delta: null` exercises the null_default path on /champion/rank.
        let now_json = r#"{
          "champion_data_list": [
            {"champion_id": 64, "performance_dict": {
              "Jungle": {"win_rate": 0.52, "pick_rate": 0.12, "ban_rate": 0.08,
                         "tier": 1, "rank": 3, "rank_delta": null, "games": 0},
              "Total": {}
            }},
            {"champion_id": 35, "performance_dict": {
              "Jungle": {"win_rate": 0.545, "pick_rate": 0.04, "ban_rate": 0.10,
                         "tier": 1, "rank": 1, "rank_delta": 2, "games": 0}
            }},
            {"champion_id": 1, "performance_dict": {
              "Jungle": {"win_rate": 0.61, "pick_rate": 0.001, "ban_rate": 0,
                         "tier": 0, "rank": 99, "rank_delta": 0, "games": 0},
              "Middle": {"win_rate": 0.51, "pick_rate": 0.06, "ban_rate": 0.02,
                         "tier": 2, "rank": 10, "rank_delta": 0, "games": 0}
            }},
            {"champion_id": 99, "performance_dict": {
              "Jungle": {"win_rate": 0, "pick_rate": 0.02, "ban_rate": 0,
                         "tier": 0, "rank": 0, "rank_delta": 0, "games": 0}
            }}
          ]
        }"#;
        let prev_json = r#"{"champion_data_list": [
          {"champion_id": 64, "performance_dict": {
            "Jungle": {"win_rate": 0.50, "pick_rate": 0.11, "ban_rate": 0.07,
                       "tier": 2, "rank": 4, "rank_delta": 0, "games": 0}
          }}
        ]}"#;
        let now: RankResponse = serde_json::from_str(now_json).expect("rank must parse");
        let prev: RankResponse = serde_json::from_str(prev_json).expect("prev rank must parse");

        let rows = tier_rows(&now, Some(&prev), "Jungle");
        // 1 is dropped (0.1% pick rate), 99 is dropped (win_rate 0 = not a
        // jungler); the rest sort by win rate desc.
        assert_eq!(
            rows.iter().map(|r| r.champion_id).collect::<Vec<_>>(),
            vec![35, 64]
        );
        // 35 is missing from the previous patch → delta unknown (0.0).
        assert_eq!(rows[0].win_rate_delta, 0.0);
        // 64: 0.52 vs 0.50 → +2.0 percentage points.
        assert!((rows[1].win_rate_delta - 2.0).abs() < 1e-9);
        assert!((rows[1].pick_rate - 0.12).abs() < 1e-9);
        assert!((rows[1].ban_rate - 0.08).abs() < 1e-9);
        // Games are calibrated separately; the pure shaping leaves them 0.
        assert!(rows.iter().all(|r| r.games == 0));
    }

    #[test]
    fn build_spell_and_matchup_parse_and_counters_invert() {
        let json = r#"{
          "build_by_lane": {
            "Jungle": {
              "games": 5000, "pick_rate": 0.05, "win_rate": 0.51, "ban_rate": null,
              "build_lst": [
                {"win_rate": 0.53, "games": 800,
                 "rune": {"main_build": [8000,8010,9111,9105,8299],
                          "sub_build": [8400,8473,8453],
                          "stat_build": [5005,5008,5001]},
                 "item": {"build": [6692]},
                 "spell": {"build": [11, 4]},
                 "skill": {"build": [3, 1, 2],
                           "detail": [3, 1, 2, 3, 3, 4],
                           "win_rate": 0.54,
                           "games": 777}}
              ],
              "match_up": {
                "strong_against": [
                  {"games": 120, "win_rate": 0.6, "match_rate": 0.01, "enemy_champion_id": 5}
                ],
                "weak_against": [
                  {"games": 200, "win_rate": 0.42, "match_rate": 0.02, "enemy_champion_id": 64},
                  {"games": 10, "win_rate": 0.43, "match_rate": 0.001, "enemy_champion_id": 76},
                  {"games": 150, "win_rate": 0.46, "match_rate": 0.015, "enemy_champion_id": 121}
                ]
              }
            }
          }
        }"#;
        let b: BuildResponse = serde_json::from_str(json).expect("build must parse");
        let (lane, lb) = pick_lane(&b, Some("jungle")).expect("Jungle lane should be picked");
        assert_eq!(lane, "Jungle");
        assert_eq!(lb.build_lst[0].spell.build, vec![11, 4]);
        assert_eq!(lb.build_lst[0].skill.build, vec![3, 1, 2]);
        assert_eq!(lb.build_lst[0].skill.detail, vec![3, 1, 2, 3, 3, 4]);
        assert_eq!(lb.build_lst[0].skill.games, 777);

        let counters = counter_entries(lb);
        // 76 is dropped (< 30 games); order (worst-for-subject first) kept.
        assert_eq!(counters.len(), 2);
        assert_eq!(counters[0].champion_id, 64);
        // win_rate is inverted to the counter champion's perspective.
        assert!((counters[0].win_rate - 0.58).abs() < 1e-9);
        assert_eq!(counters[0].games, 200);
        assert_eq!(counters[1].champion_id, 121);
        assert!((counters[1].win_rate - 0.54).abs() < 1e-9);
    }

    #[test]
    fn matchup_stats_null_positions_survive_parse() {
        // Invalid pairs come back as 200 + `"stats_by_position": null`.
        let r: MatchupStatsResponse =
            serde_json::from_str(r#"{"stats_by_position": null}"#).expect("null must parse");
        assert!(r.stats_by_position.is_empty());

        let r: MatchupStatsResponse = serde_json::from_str(
            r#"{"stats_by_position": {"Middle": {"games": 1468, "my_win_rate": 51.57,
                "enemy_win_rate": 48.43}}}"#,
        )
        .expect("stats must parse");
        let mid = &r.stats_by_position["Middle"];
        assert_eq!(mid.games, 1468);
        // Percent 0–100 here, not a fraction.
        assert!((mid.my_win_rate - 51.57).abs() < 1e-9);
    }

    #[test]
    fn otp_match_parses_and_flags_incomplete_runes() {
        let json = r#"{"match_up_list": [
          {"position": "Jungle", "win": 1, "tier": "MASTER",
           "rune": {"perk_0": 8010, "perk_1": 9111, "perk_2": 9104, "perk_3": 8299,
                    "perk_4": 8473, "perk_5": 8451, "perk_primary_style": 8000,
                    "perk_sub_style": 8400, "stat_perk_0": 5005, "stat_perk_1": 5008,
                    "stat_perk_2": 5001},
           "spell": {"spell_1": 11, "spell_2": 4}},
          {"position": "Jungle", "win": 0, "rune": null, "spell": null}
        ]}"#;
        let r: OtpResponse = serde_json::from_str(json).expect("OTP must parse");
        assert_eq!(r.match_up_list.len(), 2);
        assert!(r.match_up_list[0].rune.is_complete());
        assert_eq!(r.match_up_list[0].spell.spell_1, 11);
        // A null rune block zeroes out → incomplete → excluded from aggregation.
        assert!(!r.match_up_list[1].rune.is_complete());
    }

    /// Convenience builder for aggregation tests.
    fn otp(
        primary: i64,
        keystone: i64,
        p1: i64,
        sub_style: i64,
        p4: i64,
        p5: i64,
        spells: (i64, i64),
    ) -> OtpEntry {
        OtpEntry {
            position: "Jungle".into(),
            win: 1,
            rune: OtpRune {
                perk_0: keystone,
                perk_1: p1,
                perk_2: 9104,
                perk_3: 8299,
                perk_4: p4,
                perk_5: p5,
                perk_primary_style: primary,
                perk_sub_style: sub_style,
                stat_perk_0: 5005,
                stat_perk_1: 5008,
                stat_perk_2: 5001,
            },
            spell: OtpSpell {
                spell_1: spells.0,
                spell_2: spells.1,
            },
        }
    }

    #[test]
    fn matchup_aggregation_groups_modes_and_spell_pairs() {
        let entries = vec![
            // Group A: Precision + keystone 8010 (3 games — wins).
            otp(8000, 8010, 9111, 8400, 8473, 8451, (11, 4)),
            otp(8000, 8010, 9111, 8400, 8473, 8451, (4, 11)),
            // Same group, but secondary tree differs — its minors must not
            // leak into the secondary-slot modes.
            otp(8000, 8010, 9104, 8100, 8139, 8135, (11, 4)),
            // Group B: Domination + keystone 8112 (2 games — loses).
            otp(8100, 8112, 9111, 8000, 9111, 8009, (11, 12)),
            otp(8100, 8112, 9111, 8000, 9111, 8009, (11, 12)),
        ];
        let refs: Vec<&OtpEntry> = entries.iter().collect();
        let page = aggregate_otp(&refs).expect("aggregation must produce a page");

        assert_eq!(page.primary_style, 8000); // 3-game group beats 2-game group
        assert_eq!(page.primary_perks[0], 8010); // the group's keystone
        assert_eq!(page.primary_perks[1], 9111); // per-slot mode (2 vs 1)
        assert_eq!(page.sub_style, 8400); // modal secondary tree
        assert_eq!(page.sub_perks, vec![8473, 8451]); // 8100-tree game excluded
        assert_eq!(page.shards, vec![5005, 5008, 5001]);
        // Spells: Smite/Flash appears 3× ([11,4] twice + [4,11] once) vs
        // [11,12] twice — the pair counting must merge orientations and the
        // output must keep the more common one.
        assert_eq!(page.spells, vec![11, 4]);
    }

    #[test]
    fn counter_inversion_caps_at_eight() {
        let weak: Vec<MatchUpEntry> = (0..12)
            .map(|i| MatchUpEntry {
                games: 100,
                win_rate: 0.40 + i as f64 * 0.01,
                match_rate: 0.01,
                enemy_champion_id: 1000 + i,
            })
            .collect();
        let lb = LaneBuild {
            match_up: MatchUp {
                strong_against: vec![],
                weak_against: weak,
            },
            ..Default::default()
        };
        let counters = counter_entries(&lb);
        assert_eq!(counters.len(), 8);
        // Best counter (subject's worst matchup) stays first.
        assert_eq!(counters[0].champion_id, 1000);
        assert!((counters[0].win_rate - 0.60).abs() < 1e-9);
    }

    // ---- live tests (network) ----
    //
    // Decisive end-to-end checks against the real DeepLoL + Data Dragon APIs.
    // Ignored by default; run with:
    //   cargo test --lib provider::deeplol -- --ignored --nocapture

    /// Shared harness for the live tests: one provider, one runtime.
    fn live<F, Fut>(f: F)
    where
        F: FnOnce(DeepLolProvider) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(f(DeepLolProvider::new().unwrap()));
    }

    #[test]
    #[ignore]
    fn live_champion_names() {
        // The mock scenarios lean on both name directions; Wukong is the case
        // where display name and Data Dragon image id actually differ.
        live(|p| async move {
            let (name, image) = p.champion_names(62).await.expect("Wukong names");
            assert_eq!(name, "Wukong");
            assert_eq!(image, "MonkeyKing");
            let (name, image) = p.champion_names(31).await.expect("Cho'Gath names");
            assert_eq!(name, "Cho'Gath");
            assert_eq!(image, "Chogath");
            assert!(p.champion_names(-1).await.is_none());
            println!("CHAMPION NAMES OK");
        });
    }

    #[test]
    #[ignore]
    fn live_zed_items() {
        live(|p| async move {
            let snap = GameSnapshot {
                game_mode: "CLASSIC".into(),
                game_time: 600.0,
                self_champion: "Zed".into(),
                self_raw_name: "Zed".into(),
                self_position: "MIDDLE".into(),
                enemies: vec![],
                allies: vec![],
            };
            match p.items(&snap).await {
                Ok(items) => {
                    println!("ITEMS OK ({}):", items.len());
                    for it in &items {
                        println!("  {} [{}] {}", it.item_id, it.name, it.reason);
                    }
                    assert!(!items.is_empty());
                }
                Err(e) => panic!("items() failed: {e}"),
            }
            match p.runes(238, Some("middle")).await {
                Ok(r) => println!(
                    "RUNES OK: {} primary={} sub={} perks={:?}",
                    r.name, r.primary_style_id, r.sub_style_id, r.selected_perk_ids
                ),
                Err(e) => panic!("runes() failed: {e}"),
            }
        });
    }

    #[test]
    #[ignore]
    fn live_hexgate_tier_list() {
        live(|p| async move {
            let rows = p.tier_list("jungle").await.expect("tier_list failed");
            println!("TIER LIST OK ({} rows):", rows.len());
            for r in rows.iter().take(8) {
                println!(
                    "  {:>4} wr={:.3} d={:+.1} pr={:.3} br={:.3} games={}",
                    r.champion_id, r.win_rate, r.win_rate_delta, r.pick_rate, r.ban_rate, r.games
                );
            }
            assert!(!rows.is_empty());
            for r in &rows {
                assert!(r.champion_id > 0);
                assert!(r.win_rate > 0.0 && r.win_rate < 1.0, "wr {}", r.win_rate);
                assert!(r.pick_rate >= 0.005 && r.pick_rate <= 1.0);
                assert!(r.ban_rate >= 0.0 && r.ban_rate <= 1.0);
                assert!(r.games >= 0);
            }
            assert!(
                rows.windows(2).all(|w| w[0].win_rate >= w[1].win_rate),
                "tier list must be sorted by win rate desc"
            );
            assert!(
                rows.iter().any(|r| r.games > 0),
                "games calibration produced no estimates"
            );
            // Second invoke must come from the cache (and stay identical).
            let again = p.tier_list("jungle").await.expect("cached tier_list failed");
            assert_eq!(again.len(), rows.len());
        });
    }

    #[test]
    #[ignore]
    fn live_hexgate_counters() {
        live(|p| async move {
            // Who counters Shaco (35) in the jungle?
            let counters = p.counters(35, "jungle").await.expect("counters failed");
            println!("COUNTERS OK ({}):", counters.len());
            for c in &counters {
                println!("  {:>4} wr={:.3} games={}", c.champion_id, c.win_rate, c.games);
            }
            assert!(!counters.is_empty());
            assert!(counters.len() <= 8);
            for c in &counters {
                assert!(c.champion_id > 0);
                assert!(c.win_rate > 0.0 && c.win_rate < 1.0);
                assert!(c.games >= MIN_MATCHUP_GAMES);
            }
        });
    }

    #[test]
    #[ignore]
    fn live_hexgate_rune_build() {
        live(|p| async move {
            // Viego (234) jungle, no enemy → the plain best-build page.
            let b = p
                .rune_build(234, Some("jungle"), None)
                .await
                .expect("rune_build failed");
            println!("RUNE BUILD OK: {b:?}");
            assert!(b.page_name.starts_with("HEXGATE Viego"), "{}", b.page_name);
            assert_eq!(b.lane, "Jungle");
            assert!(b.primary_style_id > 0 && b.sub_style_id > 0);
            assert_eq!(b.primary_perk_ids.len(), 4);
            assert_eq!(b.sub_perk_ids.len(), 2);
            assert_eq!(b.shard_ids.len(), 3);
            assert_eq!(b.spell_ids.len(), 2, "expected a spell pair");
            assert!(b.win_rate > 0.0 && b.win_rate < 1.0);
            assert!(b.games > 0);
            assert!(!b.matchup);
            // runes() is a shim over rune_build(): same data, flat LCU shape.
            let r = p.runes(234, Some("jungle")).await.expect("runes failed");
            assert_eq!(r.primary_style_id, b.primary_style_id);
            assert_eq!(r.selected_perk_ids.len(), 9);
        });
    }

    #[test]
    #[ignore]
    fn live_current_mock_pick_rune_build() {
        live(|p| async move {
            let rows = p.tier_list("jungle").await.expect("tier_list failed");
            let champion_id = rows.first().expect("jungle tier list empty").champion_id;
            let b = p
                .rune_build(champion_id, Some("jungle"), None)
                .await
                .expect("current mock pick rune_build failed");
            println!("MOCK PICK RUNE BUILD OK: champion={champion_id} {b:?}");
            assert!(b.primary_style_id > 0 && b.sub_style_id > 0);
            assert_eq!(b.primary_perk_ids.len(), 4);
            assert_eq!(b.sub_perk_ids.len(), 2);
            assert_eq!(b.shard_ids.len(), 3);
            assert!(b.win_rate > 0.0 && b.win_rate < 1.0);
            assert!(b.games > 0);
        });
    }

    #[test]
    #[ignore]
    fn live_current_jungle_tier_rune_builds() {
        live(|p| async move {
            let rows = p.tier_list("jungle").await.expect("tier_list failed");
            for row in rows.iter().take(12) {
                let b = p
                    .rune_build(row.champion_id, Some("jungle"), None)
                    .await
                    .unwrap_or_else(|e| {
                        panic!(
                            "rune_build failed for current jungle tier champion {}: {e}",
                            row.champion_id
                        )
                    });
                println!(
                    "JUNGLE RUNE OK: champion={} lane={} games={}",
                    row.champion_id, b.lane, b.games
                );
                assert_eq!(b.primary_perk_ids.len(), 4);
                assert_eq!(b.sub_perk_ids.len(), 2);
                assert_eq!(b.shard_ids.len(), 3);
            }
        });
    }

    #[test]
    #[ignore]
    fn live_jp1_region_falls_back_to_kr_builds() {
        live(|p| async move {
            p.set_platform_id("JP1");

            let zyra = p
                .rune_build(143, Some("jungle"), None)
                .await
                .expect("JP1 Zyra should fall back to KR build data");
            println!("JP1 FALLBACK RUNE OK: {zyra:?}");
            assert_eq!(zyra.lane, "Jungle");
            assert_eq!(zyra.primary_perk_ids.len(), 4);
            assert_eq!(zyra.sub_perk_ids.len(), 2);
            assert_eq!(zyra.shard_ids.len(), 3);

            let counters = p
                .counters(200, "jungle")
                .await
                .expect("JP1 Aurora counters should fall back to KR build data");
            println!("JP1 FALLBACK COUNTERS OK: {}", counters.len());
            assert!(!counters.is_empty());
        });
    }

    #[test]
    #[ignore]
    fn live_hexgate_matchup_build() {
        live(|p| async move {
            // Viego (234) vs Shaco (35) in the jungle. The matchup may
            // legitimately be too thin at the current patch, so the only
            // acceptable failure is exactly NotEnoughData.
            match p.rune_build(234, Some("jungle"), Some(35)).await {
                Ok(b) => {
                    println!("MATCHUP BUILD OK: {b:?}");
                    assert!(b.matchup);
                    assert!(b.page_name.contains(" vs "), "{}", b.page_name);
                    assert!(b.primary_style_id > 0 && b.sub_style_id > 0);
                    assert_eq!(b.primary_perk_ids.len(), 4);
                    assert_eq!(b.sub_perk_ids.len(), 2);
                    assert_eq!(b.shard_ids.len(), 3);
                    assert!(b.win_rate > 0.0 && b.win_rate < 1.0);
                    assert!(b.games >= MIN_MATCHUP_GAMES);
                }
                Err(Error::NotEnoughData) => {
                    println!("MATCHUP: not enough data (acceptable outcome)");
                }
                Err(e) => panic!("matchup rune_build failed unexpectedly: {e}"),
            }
        });
    }
}
