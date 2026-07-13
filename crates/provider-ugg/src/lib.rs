//! u.gg stats provider implementing [`BuildProvider`].
//!
//! Data comes from u.gg's stats2 JSON API, including site-wide tier lists via
//! the `champion_ranking` endpoint used by <https://u.gg/lol/tier-list>.

mod api;
mod tier_list;
mod types;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use overlay_ddragon::{normalize, DdragonClient};
use overlay_provider::{
    counter_entries_from_subject_losses, item_recommendations, rune_recommendation,
    split_primary_secondary_runes, BuildProvider, CounterEntry, ItemRecommendation, ProviderError,
    Result, RuneBuild, RuneRecommendation, SkillOrder,
};
use overlay_types::{GameSnapshot, TierEntry};
use tokio::sync::RwLock;

use crate::api::{select_matchups, select_overview, UggApi};
use crate::tier_list::{region_slug, tier_entries_from_ranking, TIER_LIST_RANK};
use crate::types::default_overview::{LateItem, OverviewData};
use crate::types::mappings::{Mode, Region, Role};
use crate::types::matchups::Matchup;

pub struct UggProvider {
    ddragon: Arc<DdragonClient>,
    api: UggApi,
    platform_id: std::sync::RwLock<String>,
    tier_lists: RwLock<HashMap<String, Vec<TierEntry>>>,
}

impl UggProvider {
    pub fn new(ddragon: Arc<DdragonClient>) -> Result<Self> {
        Ok(Self {
            ddragon,
            api: UggApi::new()?,
            platform_id: std::sync::RwLock::new("KR".into()),
            tier_lists: RwLock::new(HashMap::new()),
        })
    }

    fn set_platform_id_internal(&self, platform_id: &str) {
        *self.platform_id.write().unwrap() = platform_id.to_string();
        if let Ok(mut cache) = self.tier_lists.try_write() {
            cache.clear();
        }
    }

    fn current_region(&self) -> Region {
        platform_to_region(&self.platform_id.read().unwrap())
    }

    async fn ensure_static(&self) -> Result<()> {
        let version = self
            .ddragon
            .version()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        self.api.ensure_static(&version).await
    }

    async fn champion_id(&self, raw_name: &str) -> Option<i64> {
        let champs = self.ddragon.champions().await.ok()?;
        champs.name_to_id.get(&normalize(raw_name)).copied()
    }

    async fn champion_key(&self, champion_id: i64) -> Result<String> {
        self.ensure_static().await?;
        let champs = self
            .ddragon
            .champions()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        champs
            .id_to_key
            .get(&champion_id)
            .cloned()
            .ok_or_else(|| ProviderError::Other(format!("unknown champion id: {champion_id}")))
    }

    async fn champion_name(&self, champion_id: i64) -> String {
        self.ensure_static().await.ok();
        self.champion_names(champion_id)
            .await
            .map_or_else(|| format!("Champion {champion_id}"), |(n, _)| n)
    }

    async fn fetch_overview_for_snapshot(
        &self,
        snapshot: &GameSnapshot,
    ) -> Result<(OverviewData, Role)> {
        if is_arena_mode(&snapshot.game_mode) {
            return Err(ProviderError::NotEnoughData);
        }
        let id = self
            .champion_id(&snapshot.self_raw_name)
            .await
            .ok_or_else(|| {
                ProviderError::Other(format!("unknown champion: {:?}", snapshot.self_champion))
            })?;
        self.fetch_overview(id, &snapshot.self_position, snapshot_mode(snapshot))
            .await
    }

    async fn fetch_overview(
        &self,
        champion_id: i64,
        position: &str,
        mode: Mode,
    ) -> Result<(OverviewData, Role)> {
        self.ensure_static().await?;
        let champ_key = self.champion_key(champion_id).await?;
        let role = ugg_role(position, mode);
        let region = self.current_region();
        let overview = self
            .api
            .get_overview(&champ_key, mode, crate::types::mappings::Build::Recommended)
            .await?;
        select_overview(&overview, region, role)
    }

    async fn fetch_matchup_overview(
        &self,
        champion_id: i64,
        enemy_champion_id: i64,
        position: &str,
    ) -> Result<(OverviewData, Role)> {
        self.ensure_static().await?;
        let champ_key = self.champion_key(champion_id).await?;
        let enemy_champ_key = self.champion_key(enemy_champion_id).await?;
        let role = ugg_role(position, Mode::Normal);
        let region = self.current_region();
        let overview = self
            .api
            .get_matchup_overview(
                &champ_key,
                &enemy_champ_key,
                Mode::Normal,
                crate::types::mappings::Build::Recommended,
            )
            .await?;
        select_overview(&overview, region, role)
    }

    #[allow(clippy::cast_precision_loss)]
    fn overview_to_items(
        data: &OverviewData,
        item_names: &HashMap<i64, String>,
    ) -> Vec<ItemRecommendation> {
        let wr = if data.core_items.matches > 0 {
            data.core_items.wins as f64 / data.core_items.matches as f64 * 100.0
        } else if data.matches > 0 {
            data.wins as f64 / data.matches as f64 * 100.0
        } else {
            0.0
        };
        let games = if data.core_items.matches > 0 {
            data.core_items.matches
        } else {
            data.matches
        };

        let mut build_ids = Vec::new();
        build_ids.extend(data.starting_items.item_ids.iter().copied());
        build_ids.extend(data.core_items.item_ids.iter().copied());
        for options in [
            &data.item_4_options,
            &data.item_5_options,
            &data.item_6_options,
        ] {
            if let Some(best) = best_late_item(options) {
                build_ids.push(best.id);
            }
        }

        item_recommendations(
            build_ids,
            |item_id| {
                item_names
                    .get(&item_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Item {item_id}"))
            },
            wr,
            games,
        )
    }

    #[allow(clippy::cast_precision_loss)]
    fn overview_to_skill_order(data: &OverviewData) -> SkillOrder {
        let max_order = parse_max_order(&data.abilities.ability_max_order);
        let level_order = parse_level_order(&data.abilities.ability_order);
        let win_rate = if data.abilities.matches > 0 {
            data.abilities.wins as f64 / data.abilities.matches as f64
        } else {
            0.0
        };
        SkillOrder {
            max_order,
            level_order,
            win_rate,
            games: data.abilities.matches,
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn overview_to_rune_build(
        data: &OverviewData,
        resolved_role: Role,
        champ_name: &str,
        matchup: bool,
    ) -> Result<RuneBuild> {
        let lane = role_lane_name(resolved_role);
        let (primary_perks, sub_perks) = split_primary_secondary_runes(&data.runes.rune_ids);
        let win_rate = if data.runes.matches > 0 {
            data.runes.wins as f64 / data.runes.matches as f64
        } else if data.matches > 0 {
            data.wins as f64 / data.matches as f64
        } else {
            0.0
        };
        let games = if data.runes.matches > 0 {
            data.runes.matches
        } else {
            data.matches
        };

        if data.runes.primary_style_id == 0
            || data.runes.secondary_style_id == 0
            || primary_perks.len() != 4
            || sub_perks.len() != 2
            || data.shards.shard_ids.len() != 3
        {
            return Err(ProviderError::Other("incomplete rune data".into()));
        }

        Ok(RuneBuild {
            page_name: format!("OPENLOL {champ_name} {lane}"),
            lane: lane.to_string(),
            win_rate,
            games,
            primary_style_id: data.runes.primary_style_id,
            sub_style_id: data.runes.secondary_style_id,
            primary_perk_ids: primary_perks,
            sub_perk_ids: sub_perks,
            shard_ids: data.shards.shard_ids.clone(),
            spell_ids: data.summoner_spells.spell_ids.clone(),
            matchup,
        })
    }
}

#[async_trait]
impl BuildProvider for UggProvider {
    fn set_platform_id(&self, platform_id: &str) {
        self.set_platform_id_internal(platform_id);
    }

    async fn items(&self, snapshot: &GameSnapshot) -> Result<Vec<ItemRecommendation>> {
        let (data, _) = self.fetch_overview_for_snapshot(snapshot).await?;
        self.ensure_static().await?;
        let items = self
            .ddragon
            .items()
            .await
            .map_err(|e| ProviderError::Other(e.to_string()))?;
        let recs = Self::overview_to_items(&data, &items);
        if recs.is_empty() {
            return Err(ProviderError::Other("build had no items".into()));
        }
        Ok(recs)
    }

    async fn skill_order(&self, snapshot: &GameSnapshot) -> Result<SkillOrder> {
        let (data, _) = self.fetch_overview_for_snapshot(snapshot).await?;
        let order = Self::overview_to_skill_order(&data);
        if order.max_order.is_empty() && order.level_order.is_empty() {
            return Err(ProviderError::Other("build had no skill order".into()));
        }
        Ok(order)
    }

    async fn runes(&self, champion_id: i64, role: Option<&str>) -> Result<RuneRecommendation> {
        Ok(rune_recommendation(
            "u.gg",
            self.rune_build(champion_id, role, None).await?,
        ))
    }

    async fn counters(&self, champion_id: i64, role: &str) -> Result<Vec<CounterEntry>> {
        self.ensure_static().await?;
        let champ_key = self.champion_key(champion_id).await?;
        let ugg_role = ugg_role(role, Mode::Normal);
        let region = self.current_region();
        let matchups = self.api.get_matchups(&champ_key, Mode::Normal).await?;
        let (data, _) = select_matchups(&matchups, region, ugg_role)?;
        let counters = counter_entries_from_worst(&data.worst_matchups);
        if !counters.is_empty() {
            return Ok(counters);
        }

        let Some(prev_patch) = self.api.previous_patch().await? else {
            return Ok(counters);
        };
        let Ok(previous) = self
            .api
            .get_matchups_for_patch(&champ_key, Mode::Normal, &prev_patch)
            .await
        else {
            return Ok(counters);
        };
        let Ok((data, _)) = select_matchups(&previous, region, ugg_role) else {
            return Ok(counters);
        };
        let previous_counters = counter_entries_from_worst(&data.worst_matchups);
        if previous_counters.is_empty() {
            Ok(counters)
        } else {
            Ok(previous_counters)
        }
    }

    async fn tier_list(&self, role: &str) -> Result<Vec<TierEntry>> {
        self.ensure_static().await?;
        let region = region_slug(&self.platform_id.read().unwrap());
        let cache_key = format!("{region}:{}", role.to_ascii_lowercase());

        {
            let cache = self.tier_lists.read().await;
            if let Some(rows) = cache.get(&cache_key) {
                return Ok(rows.clone());
            }
        }

        let ranking = self
            .api
            .get_champion_ranking(region, Mode::Normal, TIER_LIST_RANK)
            .await?;
        let previous = match self.api.previous_patch().await? {
            Some(prev_patch) => self
                .api
                .get_champion_ranking_for_patch(region, Mode::Normal, TIER_LIST_RANK, &prev_patch)
                .await
                .ok(),
            None => None,
        };
        let rows = tier_entries_from_ranking(&ranking, previous.as_ref(), role)?;
        self.tier_lists
            .write()
            .await
            .insert(cache_key, rows.clone());
        Ok(rows)
    }

    async fn rune_build(
        &self,
        champion_id: i64,
        role: Option<&str>,
        enemy_champion_id: Option<i64>,
    ) -> Result<RuneBuild> {
        let position = role.unwrap_or("");
        let (data, resolved_role, matchup) = if let Some(enemy) = enemy_champion_id {
            let (data, resolved_role) = self
                .fetch_matchup_overview(champion_id, enemy, position)
                .await?;
            (data, resolved_role, true)
        } else {
            let (data, resolved_role) = self
                .fetch_overview(champion_id, position, Mode::Normal)
                .await?;
            (data, resolved_role, false)
        };
        let name = self.champion_name(champion_id).await;
        Self::overview_to_rune_build(&data, resolved_role, &name, matchup)
    }

    async fn champion_names(&self, champion_id: i64) -> Option<(String, String)> {
        self.ensure_static().await.ok()?;
        let champs = self.ddragon.champions().await.ok()?;
        Some((
            champs.id_to_name.get(&champion_id)?.clone(),
            champs.id_to_image.get(&champion_id)?.clone(),
        ))
    }
}

fn snapshot_mode(snapshot: &GameSnapshot) -> Mode {
    if snapshot.game_mode.eq_ignore_ascii_case("ARAM") {
        Mode::ARAM
    } else {
        Mode::Normal
    }
}

fn is_arena_mode(game_mode: &str) -> bool {
    game_mode.eq_ignore_ascii_case("arena") || game_mode.eq_ignore_ascii_case("cherry")
}

#[must_use]
pub fn platform_to_region(platform_id: &str) -> Region {
    match platform_id.to_ascii_uppercase().as_str() {
        "NA1" => Region::NA1,
        "EUW1" => Region::EUW1,
        "KR" => Region::KR,
        "EUN1" => Region::EUN1,
        "BR1" => Region::BR1,
        "LA1" => Region::LA1,
        "LA2" => Region::LA2,
        "OC1" => Region::OC1,
        "RU" => Region::RU,
        "TR1" => Region::TR1,
        "JP1" => Region::JP1,
        "PH2" => Region::PH2,
        "SG2" => Region::SG2,
        "TH2" => Region::TH2,
        "TW2" => Region::TW2,
        "VN2" => Region::VN2,
        "ME1" => Region::ME1,
        _ => Region::World,
    }
}

#[must_use]
fn ugg_role(position: &str, mode: Mode) -> Role {
    if mode == Mode::ARAM {
        return Role::None;
    }
    match position.to_ascii_lowercase().as_str() {
        "top" => Role::Top,
        "jungle" => Role::Jungle,
        "middle" | "mid" => Role::Mid,
        "bottom" | "bot" | "adc" => Role::ADCarry,
        "utility" | "support" | "supporter" => Role::Support,
        _ => Role::Automatic,
    }
}

fn role_lane_name(role: Role) -> &'static str {
    match role {
        Role::Jungle => "Jungle",
        Role::Top => "Top",
        Role::Mid => "Middle",
        Role::ADCarry => "Bot",
        Role::Support => "Supporter",
        Role::None => "ARAM",
        _ => "Unknown",
    }
}

#[must_use]
fn ability_char_to_id(c: char) -> Option<i64> {
    match c.to_ascii_uppercase() {
        'Q' => Some(1),
        'W' => Some(2),
        'E' => Some(3),
        'R' => Some(4),
        _ => None,
    }
}

#[must_use]
pub fn parse_max_order(s: &str) -> Vec<i64> {
    let from_chars: Vec<i64> = s.chars().filter_map(ability_char_to_id).collect();
    if !from_chars.is_empty() {
        return from_chars;
    }
    s.split(|c: char| c == ',' || c == '>' || c.is_whitespace())
        .filter_map(|part| part.parse::<i64>().ok())
        .filter(|id| (1..=4).contains(id))
        .collect()
}

#[must_use]
pub fn parse_level_order(chars: &[char]) -> Vec<i64> {
    chars
        .iter()
        .filter_map(|c| ability_char_to_id(*c))
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn best_late_item(options: &[LateItem]) -> Option<&LateItem> {
    options.iter().max_by(|a, b| {
        let wr_a = if a.matches > 0 {
            a.wins as f64 / a.matches as f64
        } else {
            0.0
        };
        let wr_b = if b.matches > 0 {
            b.wins as f64 / b.matches as f64
        } else {
            0.0
        };
        wr_a.partial_cmp(&wr_b).unwrap_or(std::cmp::Ordering::Equal)
    })
}

/// Counters from u.gg `worst_matchups`: invert win rate to the opponent's
/// perspective (same contract as `DeepLoL`'s `counter_entries`).
#[must_use]
pub fn counter_entries_from_worst(worst: &[Matchup]) -> Vec<CounterEntry> {
    counter_entries_from_subject_losses(
        worst
            .iter()
            .map(|m| (m.champion_id, m.winrate, i64::from(m.matches))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ability_chars_map_to_skill_ids() {
        assert_eq!(
            parse_level_order(&['Q', 'W', 'E', 'Q', 'W', 'E']),
            vec![1, 2, 3, 1, 2, 3]
        );
        assert_eq!(parse_max_order("QWE"), vec![1, 2, 3]);
        assert_eq!(parse_max_order("3>1>2"), vec![3, 1, 2]);
    }

    #[test]
    fn platform_id_maps_to_region() {
        assert_eq!(platform_to_region("JP1"), Region::JP1);
        assert_eq!(platform_to_region("kr"), Region::KR);
        assert_eq!(platform_to_region("unknown"), Region::World);
    }

    #[test]
    fn provider_defaults_to_kr_until_platform_id_is_set() {
        use std::sync::Arc;

        let provider = UggProvider::new(Arc::new(DdragonClient::new())).expect("provider");
        assert_eq!(provider.current_region(), Region::KR);
        provider.set_platform_id("JP1");
        assert_eq!(provider.current_region(), Region::JP1);
    }

    #[test]
    fn counters_invert_win_rate_and_filter_by_games() {
        let worst = vec![
            Matchup {
                champion_id: 1,
                wins: 20,
                matches: 50,
                winrate: 0.4,
            },
            Matchup {
                champion_id: 2,
                wins: 25,
                matches: 20,
                winrate: 0.5,
            },
            Matchup {
                champion_id: 3,
                wins: 5,
                matches: 40,
                winrate: 0.125,
            },
        ];
        let counters = counter_entries_from_worst(&worst);
        assert_eq!(counters.len(), 2);
        assert!(counters
            .iter()
            .all(|c| c.games >= overlay_provider::MIN_MATCHUP_GAMES));
        assert_eq!(counters[0].champion_id, 3);
        assert!((counters[0].win_rate - 0.875).abs() < f64::EPSILON);
        assert_eq!(counters[1].champion_id, 1);
        assert!((counters[1].win_rate - 0.6).abs() < f64::EPSILON);
    }

    #[tokio::test]
    #[ignore = "network: live u.gg overview for Ahri"]
    async fn fetch_ahri_overview_from_live_api() {
        use std::sync::Arc;

        use overlay_types::GameSnapshot;

        let ddragon = Arc::new(DdragonClient::new());
        let provider = UggProvider::new(ddragon).expect("provider");

        let snapshot = GameSnapshot {
            game_mode: "CLASSIC".into(),
            game_time: 600.0,
            self_champion: "Ahri".into(),
            self_raw_name: "Ahri".into(),
            self_position: "middle".into(),
            enemies: vec![],
            allies: vec![],
            players: vec![],
        };

        let items = provider.items(&snapshot).await.expect("items");
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| i.item_id > 0));
        println!("items: {items:?}");

        let skills = provider.skill_order(&snapshot).await.expect("skill_order");
        assert!(!skills.level_order.is_empty());
        assert!(skills.win_rate >= 0.0 && skills.win_rate <= 1.0);
        println!("skills: {skills:?}");

        let build = provider
            .rune_build(103, Some("middle"), None)
            .await
            .expect("rune_build");
        assert!(build.primary_style_id >= 8000);
        assert!(!build.primary_perk_ids.is_empty());
        assert!(build.win_rate >= 0.0 && build.win_rate <= 1.0);
        println!("rune_build: {build:?}");
    }

    #[tokio::test]
    #[ignore = "network: live u.gg matchup overview for Vladimir vs Ahri"]
    async fn fetch_vladimir_ahri_matchup_rune_build_from_live_api() {
        use std::sync::Arc;

        let ddragon = Arc::new(DdragonClient::new());
        let provider = UggProvider::new(ddragon).expect("provider");
        let build = provider
            .rune_build(8, Some("middle"), Some(103))
            .await
            .expect("matchup rune_build");

        assert!(build.matchup);
        assert_eq!(build.lane, "Middle");
        assert!(build.primary_style_id >= 8000);
        assert!(!build.primary_perk_ids.is_empty());
        assert!(build.win_rate >= 0.0 && build.win_rate <= 1.0);
        assert!(build.games > 0);
        println!("matchup rune_build: {build:?}");
    }
}
