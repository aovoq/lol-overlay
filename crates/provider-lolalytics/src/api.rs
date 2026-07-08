//! HTTP client + process-lifetime cache for LoLalytics' `mega` API.
//!
//! The endpoint (`https://a1.lolalytics.com/mega/?ep=…`) is CORS-open and
//! unauthenticated but 403s / rejects requests without a browser-ish
//! `User-Agent` and a `Referer` of `https://lolalytics.com/`, so both are set
//! as default headers. Responses are cached for the process lifetime because
//! the in-game poller asks for items every couple of seconds and the
//! champ-select UI re-invokes commands on every render.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use overlay_provider::Result;
use reqwest::header::{HeaderMap, HeaderValue, REFERER};
use serde::de::DeserializeOwned;
use tokio::sync::RwLock;

use crate::types::{CounterResponse, EarlySetResponse, ItemSetResponse, TierResponse};

const MEGA_BASE: &str = "https://a1.lolalytics.com/mega/";
const CACHE_TTL: Duration = Duration::from_hours(6);
const RETRY_ATTEMPTS: usize = 2;
const RETRY_DELAY: Duration = Duration::from_millis(250);

/// `patch=30` aggregates the last 30 days rather than pinning a single game
/// patch, so it stays current without a version lookup. `queue=ranked`
/// (Ranked Solo/Duo) and `region=all` maximise sample size.
const PATCH: &str = "30";
const QUEUE: &str = "ranked";
const REGION: &str = "all";

/// Rank bracket the aggregates cover. LoLalytics' own default; other brackets
/// (`emerald_plus`, `diamond_plus`, …) are valid but thin out the low-elo tail.
const TIER_BRACKET: &str = "platinum_plus";

struct Cached<T> {
    loaded_at: Instant,
    value: Arc<T>,
}

impl<T> Cached<T> {
    fn fresh(&self) -> Option<Arc<T>> {
        (self.loaded_at.elapsed() < CACHE_TTL).then(|| self.value.clone())
    }
}

pub struct LolalyticsApi {
    http: reqwest::Client,
    /// `"{ep}:{slug}:{lane}"` → parsed champion-scoped response.
    itemsets: RwLock<HashMap<String, Cached<ItemSetResponse>>>,
    earlysets: RwLock<HashMap<String, Cached<EarlySetResponse>>>,
    counters: RwLock<HashMap<String, Cached<CounterResponse>>>,
    /// The whole-list `ep=tier` payload (one per process, refreshed on TTL).
    tier: RwLock<Option<Cached<TierResponse>>>,
}

impl LolalyticsApi {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(REFERER, HeaderValue::from_static("https://lolalytics.com/"));
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
            )
            .default_headers(headers)
            .build()?;
        Ok(Self {
            http,
            itemsets: RwLock::new(HashMap::new()),
            earlysets: RwLock::new(HashMap::new()),
            counters: RwLock::new(HashMap::new()),
            tier: RwLock::new(None),
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
        let key = format!("{slug}:{lane}");
        if let Some(hit) = self.itemsets.read().await.get(&key).and_then(Cached::fresh) {
            return Ok(hit);
        }
        let value: Arc<ItemSetResponse> =
            Arc::new(self.fetch_champion("build-itemset", slug, lane).await?);
        self.itemsets.write().await.insert(
            key,
            Cached {
                loaded_at: Instant::now(),
                value: value.clone(),
            },
        );
        Ok(value)
    }

    pub async fn get_earlyset(&self, slug: &str, lane: &str) -> Result<Arc<EarlySetResponse>> {
        let key = format!("{slug}:{lane}");
        if let Some(hit) = self
            .earlysets
            .read()
            .await
            .get(&key)
            .and_then(Cached::fresh)
        {
            return Ok(hit);
        }
        let value: Arc<EarlySetResponse> =
            Arc::new(self.fetch_champion("build-earlyset", slug, lane).await?);
        self.earlysets.write().await.insert(
            key,
            Cached {
                loaded_at: Instant::now(),
                value: value.clone(),
            },
        );
        Ok(value)
    }

    pub async fn get_counter(&self, slug: &str, lane: &str) -> Result<Arc<CounterResponse>> {
        let key = format!("{slug}:{lane}");
        if let Some(hit) = self.counters.read().await.get(&key).and_then(Cached::fresh) {
            return Ok(hit);
        }
        let value: Arc<CounterResponse> =
            Arc::new(self.fetch_champion("counter", slug, lane).await?);
        self.counters.write().await.insert(
            key,
            Cached {
                loaded_at: Instant::now(),
                value: value.clone(),
            },
        );
        Ok(value)
    }

    /// The whole-list tier payload (all lanes at once). `lane`/`c` are omitted;
    /// the endpoint returns every lane nested under each champion bucket.
    pub async fn get_tier(&self) -> Result<Arc<TierResponse>> {
        if let Some(hit) = self.tier.read().await.as_ref().and_then(Cached::fresh) {
            return Ok(hit);
        }
        let value: Arc<TierResponse> = Arc::new(
            self.http
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
                .await?,
        );
        *self.tier.write().await = Some(Cached {
            loaded_at: Instant::now(),
            value: value.clone(),
        });
        Ok(value)
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
