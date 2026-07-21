//! HTTP client + process-lifetime cache for op.gg's server-rendered pages.
//!
//! op.gg has no public JSON API (see [`crate::flight`] module docs): every
//! call here fetches a normal HTML page and parses the flight payload out of
//! it. The site's CDN (CloudFront + a bot-detection layer) 403s requests that
//! look like headless Chrome but is happy with a plain HTTP client carrying a
//! browser-ish `User-Agent` — same shape of guard as LoLalytics, different
//! trigger.

use std::sync::Arc;
use std::time::Duration;

use overlay_provider::{ProviderError, RequestRetryExt, Result, TtlCache, WINDOWS_USER_AGENT};

use crate::flight::{self, MetaNode};
use crate::types::{CounterRow, RunePage, SkillMastery, TierRow};

const BASE: &str = "https://op.gg";
const CACHE_TTL: Duration = Duration::from_hours(6);

/// Rank bracket the tier list covers. op.gg's own default for the champions
/// overview page; other brackets are valid query values but thin out sample
/// sizes at the low-elo tail.
const TIER_BRACKET: &str = "emerald_plus";

/// Parsed contents of a champion's `/build[/lane]` page: everything that
/// doesn't have a clean data prop and had to come out of the rendered
/// element tree, plus the one section ([`RunePage`]) that did.
#[derive(Debug, Clone, Default)]
pub struct BuildPage {
    pub starter_items: Vec<i64>,
    pub core_items: Vec<i64>,
    pub boots: Vec<i64>,
    /// `[spell1, spell2]` of the top summoner-spell combo; empty if unknown.
    pub spell_ids: Vec<i64>,
    /// Sorted by popularity; `runes[0]` is what op.gg recommends.
    pub runes: Vec<RunePage>,
}

pub struct OpggApi {
    http: reqwest::Client,
    /// `"{slug}:{lane}"` → parsed build page.
    build_pages: TtlCache<String, BuildPage>,
    /// `"{slug}:{lane}"` → matchup rows.
    counters: TtlCache<String, Vec<CounterRow>>,
    /// lane → the lane's full tier-list rows.
    tier_lists: TtlCache<String, Vec<TierRow>>,
    /// `"{slug}:{lane}"` → skill-leveling masteries.
    skills: TtlCache<String, Vec<SkillMastery>>,
}

impl OpggApi {
    pub(super) fn http(&self) -> &reqwest::Client {
        &self.http
    }

    pub fn new() -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(8))
            .user_agent(WINDOWS_USER_AGENT)
            .build()?;
        Ok(Self {
            http,
            build_pages: TtlCache::new(CACHE_TTL),
            counters: TtlCache::new(CACHE_TTL),
            tier_lists: TtlCache::new(CACHE_TTL),
            skills: TtlCache::new(CACHE_TTL),
        })
    }

    async fn fetch_html(&self, path: &str) -> Result<String> {
        self.http
            .get(format!("{BASE}{path}"))
            .send_with_retry()
            .await?
            .error_for_status()?
            .text()
            .await
            .map_err(Into::into)
    }

    /// `target_champion` (a slug, same convention as `slug`) scopes every
    /// number on the page to that specific matchup — same page, same shape,
    /// just a smaller sample. Confirmed by comparing `rune_pages[0].play`
    /// with and without it (e.g. Aatrox top overall vs Aatrox top vs Yone
    /// specifically report different sample sizes and win rates).
    pub async fn get_build_page(
        &self,
        slug: &str,
        lane: Option<&str>,
        target_champion: Option<&str>,
    ) -> Result<Arc<BuildPage>> {
        let key = format!(
            "{slug}:{}:{}",
            lane.unwrap_or("_"),
            target_champion.unwrap_or("_")
        );
        self.build_pages
            .get_or_fetch(key, || async {
                let path = match lane {
                    Some(lane) => format!("/lol/champions/{slug}/build/{lane}"),
                    None => format!("/lol/champions/{slug}/build"),
                };
                let mut request = self.http.get(format!("{BASE}{path}"));
                if let Some(target) = target_champion {
                    request = request.query(&[("target_champion", target)]);
                }
                let html = request
                    .send_with_retry()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                Ok(parse_build_page(&html))
            })
            .await
    }

    pub async fn get_counters(
        &self,
        slug: &str,
        lane: Option<&str>,
    ) -> Result<Arc<Vec<CounterRow>>> {
        let key = format!("{slug}:{}", lane.unwrap_or("_"));
        let value = self
            .counters
            .get_or_fetch(key, || async {
                let path = match lane {
                    Some(lane) => format!("/lol/champions/{slug}/counters/{lane}"),
                    None => format!("/lol/champions/{slug}/counters"),
                };
                let html = self.fetch_html(&path).await?;
                let chunks = flight::extract_flight_chunks(&html);
                let rows: Vec<CounterRow> =
                    flight::find_data_field(&chunks, "data").unwrap_or_default();
                Ok(rows)
            })
            .await?;
        if value.is_empty() {
            return Err(ProviderError::NotEnoughData);
        }
        Ok(value)
    }

    /// The full site-wide tier list for one lane. Unlike the per-champion
    /// pages, this lives at a single shared route (`/lol/champions`) filtered
    /// by a `position` query param rather than a path segment — op.gg has no
    /// combined "all lanes" list, so each lane is its own fetch.
    pub async fn get_tier_list(&self, lane: &str) -> Result<Arc<Vec<TierRow>>> {
        let value = self
            .tier_lists
            .get_or_fetch(lane.to_string(), || async {
                let html = self
                    .http
                    .get(format!("{BASE}/lol/champions"))
                    .query(&[
                        ("region", "global"),
                        ("tier", TIER_BRACKET),
                        ("type", "ranked"),
                        ("position", lane),
                    ])
                    .send_with_retry()
                    .await?
                    .error_for_status()?
                    .text()
                    .await?;
                let chunks = flight::extract_flight_chunks(&html);
                let rows: Vec<TierRow> =
                    flight::find_data_field(&chunks, "data").unwrap_or_default();
                Ok(rows)
            })
            .await?;
        if value.is_empty() {
            return Err(ProviderError::NotEnoughData);
        }
        Ok(value)
    }

    /// Skill-leveling masteries for a champion/lane, from the dedicated
    /// `/skills[/lane]` page's clean `skill_masteries` data prop — the only
    /// source with a full level-by-level order (the build page's rendered
    /// element tree only exposes the 3-letter max-priority summary).
    pub async fn get_skills(
        &self,
        slug: &str,
        lane: Option<&str>,
    ) -> Result<Arc<Vec<SkillMastery>>> {
        let key = format!("{slug}:{}", lane.unwrap_or("_"));
        let value = self
            .skills
            .get_or_fetch(key, || async {
                let path = match lane {
                    Some(lane) => format!("/lol/champions/{slug}/skills/{lane}"),
                    None => format!("/lol/champions/{slug}/skills"),
                };
                let html = self.fetch_html(&path).await?;
                let chunks = flight::extract_flight_chunks(&html);
                let masteries: Vec<SkillMastery> =
                    flight::find_data_field(&chunks, "skill_masteries").unwrap_or_default();
                Ok(masteries)
            })
            .await?;
        if value.is_empty() {
            return Err(ProviderError::NotEnoughData);
        }
        Ok(value)
    }
}

/// Reconstruct a [`BuildPage`] from the page's flight chunks: the rune table
/// ships a clean `"data"` prop, everything else is scraped out of the
/// rendered element tree via [`flight::collect_meta_nodes`].
fn parse_build_page(html: &str) -> BuildPage {
    let chunks = flight::extract_flight_chunks(html);
    let nodes = flight::collect_meta_nodes(&chunks);
    let runes = flight::find_data_field(&chunks, "rune_pages").unwrap_or_default();

    let (starter_items, core_items, boots) = parse_items(&nodes);
    BuildPage {
        starter_items,
        core_items,
        boots,
        spell_ids: parse_spell_ids(&nodes),
        runes,
    }
}

/// Item icons are tagged `metaType: "item"` and grouped under a row key
/// op.gg names itself (`"starter_items_0"`, `"core_items_0"`, `"boots_0"`,
/// each `_N` a popularity rank). We only want the top-ranked (`_0`) row of
/// each kind.
fn parse_items(nodes: &[MetaNode]) -> (Vec<i64>, Vec<i64>, Vec<i64>) {
    let mut starter = Vec::new();
    let mut core = Vec::new();
    let mut boots = Vec::new();
    for node in nodes {
        if node.meta_type != "item" {
            continue;
        }
        let Some(id) = node.meta_id.as_i64() else {
            continue;
        };
        if in_section(node, "starter_items_0") {
            starter.push(id);
        } else if in_section(node, "core_items_0") {
            core.push(id);
        } else if in_section(node, "boots_0") {
            boots.push(id);
        }
    }
    (starter, core, boots)
}

/// Summoner spells are tagged `metaType: "spell"` under row keys `"spell_0"`
/// / `"spell_1"`; the page repeats multiple combos, so only the first
/// occurrence of each slot (the top combo) is kept.
fn parse_spell_ids(nodes: &[MetaNode]) -> Vec<i64> {
    let mut slot0 = None;
    let mut slot1 = None;
    for node in nodes {
        if node.meta_type != "spell" {
            continue;
        }
        if slot0.is_none() && in_section(node, "spell_0") {
            slot0 = node.meta_id.as_i64();
        } else if slot1.is_none() && in_section(node, "spell_1") {
            slot1 = node.meta_id.as_i64();
        }
    }
    [slot0, slot1].into_iter().flatten().collect()
}

/// Whether any ancestor in `node`'s keyed path is exactly `section` (op.gg
/// keys both a row, e.g. `"core_items_0"`, and the individual icons inside it,
/// e.g. `"3161-0"`, so the row id isn't necessarily the nearest key).
fn in_section(node: &MetaNode, section: &str) -> bool {
    node.section_path.iter().any(|s| s == section)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item_node(section: &str, id: i64) -> MetaNode {
        MetaNode {
            section_path: vec![section.to_string()],
            meta_type: "item".to_string(),
            meta_id: json!(id),
        }
    }

    #[test]
    fn parse_items_keeps_only_rank_zero_rows() {
        let nodes = vec![
            item_node("starter_items_0", 1055),
            item_node("starter_items_0", 2003),
            item_node("starter_items_1", 1054),
            item_node("core_items_0", 3161),
            item_node("core_items_0", 6610),
            item_node("core_items_1", 6692),
            item_node("boots_0", 3047),
            item_node("boots_1", 3111),
        ];
        let (starter, core, boots) = parse_items(&nodes);
        assert_eq!(starter, vec![1055, 2003]);
        assert_eq!(core, vec![3161, 6610]);
        assert_eq!(boots, vec![3047]);
    }

    #[test]
    fn parse_spell_ids_keeps_first_combo_only() {
        let nodes = vec![
            MetaNode {
                section_path: vec!["spell_0".to_string()],
                meta_type: "spell".into(),
                meta_id: json!(4),
            },
            MetaNode {
                section_path: vec!["spell_1".to_string()],
                meta_type: "spell".into(),
                meta_id: json!(14),
            },
            MetaNode {
                section_path: vec!["spell_0".to_string()],
                meta_type: "spell".into(),
                meta_id: json!(4),
            },
            MetaNode {
                section_path: vec!["spell_1".to_string()],
                meta_type: "spell".into(),
                meta_id: json!(12),
            },
        ];
        assert_eq!(parse_spell_ids(&nodes), vec![4, 14]);
    }
}
