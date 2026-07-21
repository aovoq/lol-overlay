//! HTTP client + process-lifetime cache for LoLalytics' `mega` API.
//!
//! The endpoint (`https://a1.lolalytics.com/mega/?ep=…`) is CORS-open and
//! unauthenticated but 403s / rejects requests without a browser-ish
//! `User-Agent` and a `Referer` of `https://lolalytics.com/`, so both are set
//! as default headers. Responses are cached for the process lifetime because
//! the in-game poller asks for items every couple of seconds and the
//! champ-select UI re-invokes commands on every render.

use std::sync::Arc;
use std::time::Duration;

use overlay_provider::{RequestRetryExt, Result, TtlCache, MAC_USER_AGENT};
use reqwest::header::{HeaderMap, HeaderValue, REFERER};
use serde::de::DeserializeOwned;

use crate::types::{CounterResponse, EarlySetResponse, ItemSetResponse, TierResponse};

const MEGA_BASE: &str = "https://a1.lolalytics.com/mega/";
const CACHE_TTL: Duration = Duration::from_hours(6);

/// `patch=30` aggregates the last 30 days rather than pinning a single game
/// patch, so it stays current without a version lookup. `queue=ranked`
/// (Ranked Solo/Duo) and `region=all` maximise sample size.
const PATCH: &str = "30";
const QUEUE: &str = "ranked";
const REGION: &str = "all";

/// Rank bracket the aggregates cover. LoLalytics' own default; other brackets
/// (`emerald_plus`, `diamond_plus`, …) are valid but thin out the low-elo tail.
const TIER_BRACKET: &str = "platinum_plus";

pub struct LolalyticsApi {
    http: reqwest::Client,
    /// `"{ep}:{slug}:{lane}"` → parsed champion-scoped response.
    itemsets: TtlCache<String, ItemSetResponse>,
    earlysets: TtlCache<String, EarlySetResponse>,
    counters: TtlCache<String, CounterResponse>,
    /// The whole-list `ep=tier` payload (one per process, refreshed on TTL).
    tier: TtlCache<(), TierResponse>,
}

impl LolalyticsApi {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(REFERER, HeaderValue::from_static("https://lolalytics.com/"));
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(MAC_USER_AGENT)
            .default_headers(headers)
            .build()?;
        Ok(Self {
            http,
            itemsets: TtlCache::new(CACHE_TTL),
            earlysets: TtlCache::new(CACHE_TTL),
            counters: TtlCache::new(CACHE_TTL),
            tier: TtlCache::new(CACHE_TTL),
        })
    }

    /// GET `mega/` for a champion-scoped endpoint and deserialize the body.
    /// The bad-champion / bad-lane responses are still HTTP 200 with a `valid`
    /// PHP array, which fails to deserialize into `T` and surfaces as a parse
    /// error — callers treat that as "no data".
    async fn fetch_champion<T: DeserializeOwned>(
        &self,
        ep: &str,
        slug: &str,
        lane: &str,
    ) -> Result<T> {
        self.http
            .get(MEGA_BASE)
            .query(&[
                ("ep", ep),
                ("v", "1"),
                ("patch", PATCH),
                ("c", slug),
                ("lane", lane),
                ("tier", TIER_BRACKET),
                ("queue", QUEUE),
                ("region", REGION),
            ])
            .send_with_retry()
            .await?
            .error_for_status()?
            .json()
            .await
            .map_err(Into::into)
    }

    pub async fn get_itemset(&self, slug: &str, lane: &str) -> Result<Arc<ItemSetResponse>> {
        self.itemsets
            .get_or_fetch(format!("{slug}:{lane}"), || async {
                self.fetch_champion("build-itemset", slug, lane).await
            })
            .await
    }

    pub async fn get_earlyset(&self, slug: &str, lane: &str) -> Result<Arc<EarlySetResponse>> {
        self.earlysets
            .get_or_fetch(format!("{slug}:{lane}"), || async {
                self.fetch_champion("build-earlyset", slug, lane).await
            })
            .await
    }

    pub async fn get_counter(&self, slug: &str, lane: &str) -> Result<Arc<CounterResponse>> {
        self.counters
            .get_or_fetch(format!("{slug}:{lane}"), || async {
                self.fetch_champion("counter", slug, lane).await
            })
            .await
    }

    /// The whole-list tier payload (all lanes at once). `lane`/`c` are omitted;
    /// the endpoint returns every lane nested under each champion bucket.
    pub async fn get_tier(&self) -> Result<Arc<TierResponse>> {
        self.tier
            .get_or_fetch((), || async {
                Ok(self
                    .http
                    .get(MEGA_BASE)
                    .query(&[
                        ("ep", "tier"),
                        ("v", "1"),
                        ("patch", PATCH),
                        ("tier", TIER_BRACKET),
                        ("queue", QUEUE),
                        ("region", REGION),
                        ("lane", "all"),
                    ])
                    .send_with_retry()
                    .await?
                    .error_for_status()?
                    .json()
                    .await?)
            })
            .await
    }
}
