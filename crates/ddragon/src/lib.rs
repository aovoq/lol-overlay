//! Riot **Data Dragon** client with process-lifetime caching of static maps.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;
use tokio::sync::RwLock;

const DDRAGON: &str = "https://ddragon.leagueoflegends.com";

#[derive(Debug, thiserror::Error)]
pub enum DdragonError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

pub struct ChampionMaps {
    pub name_to_id: HashMap<String, i64>,
    pub id_to_name: HashMap<i64, String>,
    pub id_to_image: HashMap<i64, String>,
    pub id_to_key: HashMap<i64, String>,
}

struct StaticData {
    version: String,
    champions: Arc<ChampionMaps>,
    items: Arc<HashMap<i64, String>>,
}

pub struct DdragonClient {
    http: reqwest::Client,
    cache: RwLock<Option<StaticData>>,
}

impl Default for DdragonClient {
    fn default() -> Self {
        Self::new()
    }
}

impl DdragonClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            cache: RwLock::new(None),
        }
    }

    pub async fn version(&self) -> Result<String, DdragonError> {
        self.ensure_loaded().await?;
        let guard = self.cache.read().await;
        Ok(guard
            .as_ref()
            .expect("cache populated by ensure_loaded")
            .version
            .clone())
    }

    pub async fn champions(&self) -> Result<Arc<ChampionMaps>, DdragonError> {
        self.ensure_loaded().await?;
        let guard = self.cache.read().await;
        Ok(Arc::clone(
            &guard
                .as_ref()
                .expect("cache populated by ensure_loaded")
                .champions,
        ))
    }

    pub async fn items(&self) -> Result<Arc<HashMap<i64, String>>, DdragonError> {
        self.ensure_loaded().await?;
        let guard = self.cache.read().await;
        Ok(Arc::clone(
            &guard
                .as_ref()
                .expect("cache populated by ensure_loaded")
                .items,
        ))
    }

    /// Populate version + static maps once. Idempotent; cheap after the first call.
    async fn ensure_loaded(&self) -> Result<(), DdragonError> {
        {
            let guard = self.cache.read().await;
            if guard.is_some() {
                return Ok(());
            }
        }

        let version = fetch_ddragon_version(&self.http).await?;
        let champions = fetch_champion_map(&self.http, &version).await?;
        let items = fetch_item_map(&self.http, &version).await?;

        let mut guard = self.cache.write().await;
        if guard.is_none() {
            *guard = Some(StaticData {
                version,
                champions: Arc::new(champions),
                items: Arc::new(items),
            });
        }
        Ok(())
    }
}

async fn fetch_ddragon_version(http: &reqwest::Client) -> Result<String, DdragonError> {
    let v: Vec<String> = http
        .get(format!("{DDRAGON}/api/versions.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    v.into_iter()
        .next()
        .ok_or_else(|| DdragonError::Other("Data Dragon returned no versions".into()))
}

/// All directions of the champion map: normalized-name → id for resolving
/// Live Client names, id → display name for OPENLOL page labels, id → Data
/// Dragon image id for synthesizing mock state, and id → numeric key string
/// for u.gg URLs.
async fn fetch_champion_map(
    http: &reqwest::Client,
    ddver: &str,
) -> Result<ChampionMaps, DdragonError> {
    let file: DDChampionFile = http
        .get(format!("{DDRAGON}/cdn/{ddver}/data/en_US/champion.json"))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let mut name_to_id = HashMap::new();
    let mut id_to_name = HashMap::new();
    let mut id_to_image = HashMap::new();
    let mut id_to_key = HashMap::new();
    for (id_key, champ) in file.data {
        if let Ok(num) = champ.key.parse::<i64>() {
            // `id_key` matches rawChampionName ("Chogath"); `name` is the
            // display form ("Cho'Gath"). Index both, normalized.
            name_to_id.insert(normalize(&id_key), num);
            name_to_id.insert(normalize(&champ.name), num);
            id_to_name.insert(num, champ.name);
            id_to_image.insert(num, id_key);
            id_to_key.insert(num, champ.key);
        }
    }
    Ok(ChampionMaps {
        name_to_id,
        id_to_name,
        id_to_image,
        id_to_key,
    })
}

async fn fetch_item_map(
    http: &reqwest::Client,
    ddver: &str,
) -> Result<HashMap<i64, String>, DdragonError> {
    let file: DDItemFile = http
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

/// Lowercase + strip non-alphanumerics so "Cho'Gath", "Chogath" and "chogath"
/// all collapse to the same key.
pub fn normalize(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

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
    use super::normalize;

    #[test]
    fn normalize_collapses_punctuation_and_case() {
        assert_eq!(normalize("Cho'Gath"), "chogath");
        assert_eq!(normalize("Chogath"), "chogath");
        assert_eq!(normalize("Kai'Sa"), "kaisa");
    }
}
