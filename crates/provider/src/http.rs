//! Shared HTTP plumbing for provider API clients.
//!
//! Every stats provider ships the same transport shape: a browser-ish
//! reqwest client, bounded retries on transient failures, and a TTL cache in
//! front of endpoints the UI polls every few seconds. This module
//! centralizes that plumbing so a provider only implements endpoint-specific
//! URL building and response mapping.
//!
//! `overlay-live-client` keeps its own copy of [`RequestRetryExt`] on
//! purpose: it sits below the provider layer and must not depend on it.

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::header::{CONTENT_TYPE, RETRY_AFTER};
use serde::de::DeserializeOwned;
use tokio::sync::{Mutex, RwLock};

use crate::error::{ProviderError, Result};

const RETRY_ATTEMPTS: usize = 2;
const RETRY_DELAY: Duration = Duration::from_millis(250);

/// Windows-Chrome user agent used by providers whose bot guard accepts a
/// plain browser-ish client (LOL.PS, OP.GG).
pub const WINDOWS_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// macOS-Chrome user agent used by providers scraped from a desktop-Safari-ish
/// vantage point historically (LoLalytics, u.gg).
pub const MAC_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

/// Retry a reqwest request on transient failures: 5xx responses and
/// connect/timeout/request-build errors, with a short linear backoff.
/// Non-retryable responses and exhausted attempts surface as-is.
pub trait RequestRetryExt {
    fn send_with_retry(
        self,
    ) -> impl std::future::Future<Output = std::result::Result<reqwest::Response, reqwest::Error>> + Send;
}

impl RequestRetryExt for reqwest::RequestBuilder {
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

/// GET `url` as JSON with provider-standard error mapping: 429 →
/// [`ProviderError::RateLimited`] (honoring `Retry-After`), other non-2xx →
/// [`ProviderError::Other`] with a truncated body excerpt, a non-JSON content
/// type or a parse failure → [`ProviderError::InvalidData`]. `source` labels
/// error messages (e.g. `"LOL.PS"`).
pub async fn fetch_json<T: DeserializeOwned>(
    http: &reqwest::Client,
    url: &str,
    source: &str,
) -> Result<T> {
    let response = http.get(url).send_with_retry().await?;
    if response.status().as_u16() == 429 {
        let retry_after = response
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        return Err(ProviderError::RateLimited { retry_after });
    }
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        let diagnostic: String = body.chars().take(512).collect();
        return Err(ProviderError::Other(format!(
            "{source} HTTP {status}: {diagnostic}"
        )));
    }
    let is_json = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    if !is_json {
        return Err(ProviderError::InvalidData(format!(
            "{source} returned a non-JSON response"
        )));
    }
    let body = response.bytes().await?;
    serde_json::from_slice(&body)
        .map_err(|error| ProviderError::InvalidData(format!("{source} JSON: {error}")))
}

struct Entry<T> {
    loaded_at: Instant,
    value: Arc<T>,
}

/// Keyed TTL cache with per-key single-flight: concurrent callers for the
/// same key share one upstream fetch instead of stampeding the endpoint.
/// Stale entries are replaced on the next fetch; [`TtlCache::clear`] drops
/// everything (e.g. on patch change).
pub struct TtlCache<K, T> {
    ttl: Duration,
    entries: RwLock<HashMap<K, Entry<T>>>,
    inflight: Mutex<HashMap<K, Arc<Mutex<()>>>>,
}

impl<K, T> TtlCache<K, T>
where
    K: Eq + Hash + Clone,
{
    pub fn new(ttl: Duration) -> Self {
        Self {
            ttl,
            entries: RwLock::new(HashMap::new()),
            inflight: Mutex::new(HashMap::new()),
        }
    }

    /// Fresh cached value for `key`, if any.
    pub async fn get(&self, key: &K) -> Option<Arc<T>> {
        self.entries
            .read()
            .await
            .get(key)
            .and_then(|entry| (entry.loaded_at.elapsed() < self.ttl).then(|| entry.value.clone()))
    }

    /// Return the fresh cached value, or run `fetch` once per key (other
    /// concurrent callers wait and reuse the result) and cache it. Failed
    /// fetches are not cached.
    pub async fn get_or_fetch<F, Fut>(&self, key: K, fetch: F) -> Result<Arc<T>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        if let Some(hit) = self.get(&key).await {
            return Ok(hit);
        }
        let key_lock = {
            let mut inflight = self.inflight.lock().await;
            inflight
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _permit = key_lock.lock().await;
        let result = self.load(key.clone(), fetch).await;
        // Drop the inflight entry once no other task can still hold a clone
        // of the key lock, so the map stays bounded by in-flight work only.
        // The `inflight` lock serializes this check against new clones.
        let mut inflight = self.inflight.lock().await;
        if Arc::strong_count(&key_lock) == 2 {
            inflight.remove(&key);
        }
        drop(inflight);
        result
    }

    /// Second freshness check after winning the per-key lock: a task that
    /// queued behind a completed fetch must reuse its result.
    async fn load<F, Fut>(&self, key: K, fetch: F) -> Result<Arc<T>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        if let Some(hit) = self.get(&key).await {
            return Ok(hit);
        }
        let value = Arc::new(fetch().await?);
        self.entries.write().await.insert(
            key,
            Entry {
                loaded_at: Instant::now(),
                value: value.clone(),
            },
        );
        Ok(value)
    }

    pub async fn clear(&self) {
        self.entries.write().await.clear();
    }
}

impl<K, T> Default for TtlCache<K, T>
where
    K: Eq + Hash + Clone,
{
    fn default() -> Self {
        Self::new(Duration::from_hours(6))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use crate::error::ProviderError;
    use crate::http::TtlCache;

    #[tokio::test]
    async fn fresh_entries_are_reused_and_expired_entries_refetched() {
        let fetches = AtomicUsize::new(0);
        let cache = TtlCache::<&str, usize>::new(Duration::from_mins(30));
        for expected in [1, 1, 1] {
            let value = cache
                .get_or_fetch("k", || async {
                    Ok(fetches.fetch_add(1, Ordering::SeqCst) + 1)
                })
                .await
                .unwrap();
            assert_eq!(*value, expected);
        }
        let expired_fetches = AtomicUsize::new(0);
        let expired = TtlCache::<&str, usize>::new(Duration::ZERO);
        for expected in [1, 2, 3] {
            let value = expired
                .get_or_fetch("k", || async {
                    Ok(expired_fetches.fetch_add(1, Ordering::SeqCst) + 1)
                })
                .await
                .unwrap();
            assert_eq!(*value, expected);
        }
    }

    #[tokio::test]
    async fn failed_fetches_are_not_cached() {
        let fetches = AtomicUsize::new(0);
        let cache = TtlCache::<&str, usize>::new(Duration::from_mins(30));
        for _ in 0..2 {
            let result = cache
                .get_or_fetch("k", || async {
                    fetches.fetch_add(1, Ordering::SeqCst);
                    Err(ProviderError::Other("down".into()))
                })
                .await;
            assert!(matches!(result, Err(ProviderError::Other(_))));
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn concurrent_callers_share_one_fetch() {
        let fetches = Arc::new(AtomicUsize::new(0));
        let cache = Arc::new(TtlCache::<&str, usize>::new(Duration::from_mins(30)));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let cache = cache.clone();
            let fetches = fetches.clone();
            tasks.push(tokio::spawn(async move {
                cache
                    .get_or_fetch("k", || async move {
                        fetches.fetch_add(1, Ordering::SeqCst);
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        Ok(7)
                    })
                    .await
            }));
        }
        for task in tasks {
            assert_eq!(*task.await.unwrap().unwrap(), 7);
        }
        assert_eq!(fetches.load(Ordering::SeqCst), 1);
        // The inflight entry is dropped once the stampede drains.
        assert!(cache.inflight.lock().await.is_empty());
    }

    #[tokio::test]
    async fn clear_forces_a_refetch() {
        let fetches = AtomicUsize::new(0);
        let cache = TtlCache::<&str, usize>::new(Duration::from_mins(30));
        cache
            .get_or_fetch("k", || async { Ok(fetches.fetch_add(1, Ordering::SeqCst)) })
            .await
            .unwrap();
        cache.clear().await;
        cache
            .get_or_fetch("k", || async { Ok(fetches.fetch_add(1, Ordering::SeqCst)) })
            .await
            .unwrap();
        assert_eq!(fetches.load(Ordering::SeqCst), 2);
    }
}
