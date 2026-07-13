//! Direct JSON player-stat adapter for DeepLoL's public CDN API.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use futures::{stream, StreamExt};
use overlay_provider::{PlayerStatsProvider, ProviderCapabilities, ProviderError, Result};
use overlay_types::{
    MatchFailure, MatchPage, MatchParticipant, PlayerChampionStats, PlayerIdentity, PlayerMatch,
    PlayerProfile, PlayerRef, ProviderExtras, RankedEntry, RefreshAvailability, RefreshResult,
    SeasonRank,
};
use serde_json::{json, Value};

use super::{DeepLolProvider, DEEPLOL};

const PLAYER_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const MATCH_CONCURRENCY: usize = 5;
const MATCH_PAGE_SIZE: usize = 20;

type Timed<T> = (Instant, T);
type ChampionCacheKey = (PlayerRef, String, Option<String>, Option<String>);

#[derive(Default)]
pub(super) struct PlayerCache {
    profiles: HashMap<PlayerRef, Timed<PlayerProfile>>,
    matches: HashMap<(PlayerRef, String, Option<i64>), Timed<MatchPage>>,
    champions: HashMap<ChampionCacheKey, Timed<Vec<PlayerChampionStats>>>,
}

impl PlayerCache {
    fn fresh<T: Clone>(entry: Option<&Timed<T>>) -> Option<T> {
        entry
            .filter(|(loaded, _)| loaded.elapsed() < PLAYER_CACHE_TTL)
            .map(|(_, value)| value.clone())
    }

    fn invalidate(&mut self, player: &PlayerRef) {
        self.profiles.remove(player);
        self.matches.retain(|(cached, _, _), _| cached != player);
        self.champions
            .retain(|(cached, _, _, _), _| cached != player);
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn platform_id(raw: &str) -> Result<String> {
    let upper = raw.trim().to_ascii_uppercase();
    let mapped = match upper.as_str() {
        "KR" | "KR1" => "KR",
        "JP" | "JP1" => "JP1",
        "NA" | "NA1" => "NA1",
        "EUW" | "EUW1" => "EUW1",
        "EUNE" | "EUN1" => "EUN1",
        "OCE" | "OC1" => "OC1",
        "BR" | "BR1" => "BR1",
        "LAN" | "LA1" => "LA1",
        "LAS" | "LA2" => "LA2",
        "TR" | "TR1" => "TR1",
        "RU" => "RU",
        "PH" | "PH2" => "PH2",
        "SG" | "SG2" => "SG2",
        "TH" | "TH2" => "TH2",
        "TW" | "TW2" => "TW2",
        "VN" | "VN2" => "VN2",
        _ => {
            return Err(ProviderError::Other(format!(
                "unsupported DeepLoL platform: {raw}"
            )))
        }
    };
    Ok(mapped.into())
}

async fn response_json(request: reqwest::RequestBuilder) -> Result<Value> {
    let response = request.send().await?;
    let status = response.status();
    let retry_after = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = response.text().await?;
    if !status.is_success() {
        let detail = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .get("msg")
                    .or_else(|| value.get("detail"))
                    .map(Value::to_string)
            })
            .unwrap_or_else(|| "response body unavailable".into());
        return Err(ProviderError::Other(format!(
            "player-http:{} retry-after={}: {detail}",
            status.as_u16(),
            retry_after.as_deref().unwrap_or("none")
        )));
    }
    serde_json::from_str(&body).map_err(Into::into)
}

fn object<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| value.get(*key))
}

fn string(value: &Value, keys: &[&str]) -> Option<String> {
    object(value, keys).and_then(|value| match value {
        Value::String(text) if !text.is_empty() => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

fn integer(value: &Value, keys: &[&str]) -> Option<i64> {
    object(value, keys).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().map(|number| number as i64))
            .or_else(|| value.as_str()?.parse().ok())
    })
}

fn float(value: &Value, keys: &[&str]) -> Option<f64> {
    object(value, keys).and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
}

fn boolean(value: &Value, keys: &[&str]) -> Option<bool> {
    object(value, keys).and_then(|value| {
        value
            .as_bool()
            .or_else(|| value.as_i64().map(|number| number != 0))
    })
}

fn timestamp_millis(value: i64) -> i64 {
    if value.abs() < 10_000_000_000 {
        value * 1_000
    } else {
        value
    }
}

fn parse_rank(queue: &str, value: &Value) -> RankedEntry {
    RankedEntry {
        queue: queue.into(),
        tier: string(value, &["tier"]),
        division: string(value, &["division", "rank"]),
        lp: integer(value, &["lp", "league_points"]),
        wins: integer(value, &["win_cnt", "wins"]),
        losses: integer(value, &["loss_cnt", "losses"]),
    }
}

fn parse_profile(
    player: &PlayerRef,
    basic_response: &Value,
    realtime: Option<&Value>,
    updated: Option<&Value>,
) -> Result<PlayerProfile> {
    let basic = basic_response
        .get("summoner_basic_info_dict")
        .unwrap_or(basic_response);
    let puuid = string(basic, &["puu_id", "puuid"])
        .ok_or_else(|| ProviderError::Other("DeepLoL profile omitted puu_id".into()))?;
    let mut ranks = vec![];
    if let Some(tiers) = realtime.and_then(|value| value.get("season_tier_info_dict")) {
        for (key, queue) in [
            ("ranked_solo_5x5", "RANKED_SOLO_5x5"),
            ("ranked_flex_sr", "RANKED_FLEX_SR"),
        ] {
            if let Some(value) = tiers.get(key) {
                ranks.push(parse_rank(queue, value));
            }
        }
    }
    let previous_seasons = basic
        .get("previous_season_tier_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|value| SeasonRank {
            season: string(value, &["season"]).unwrap_or_else(|| "unknown".into()),
            queue: "RANKED_SOLO_5x5".into(),
            tier: string(value, &["tier"]),
            division: string(value, &["division"]),
            lp: integer(value, &["lp"]),
        })
        .collect();
    let remain_seconds = updated.and_then(|value| integer(value, &["remain_second"]));
    Ok(PlayerProfile {
        source: "deeplol".into(),
        identity: PlayerIdentity {
            platform_id: platform_id(&player.platform_id)?,
            game_name: string(basic, &["riot_id_name"]).unwrap_or_else(|| player.game_name.clone()),
            tag_line: string(basic, &["riot_id_tag_line"])
                .unwrap_or_else(|| player.tag_line.clone()),
            puuid: Some(puuid),
        },
        level: integer(basic, &["level"]),
        profile_icon_id: integer(basic, &["profile_id", "icon_id"]),
        ranks,
        previous_seasons,
        ladder_rank: realtime.and_then(|value| integer(value, &["rank", "ladder_rank"])),
        ladder_percentile: realtime
            .and_then(|value| float(value, &["rank_percent", "ladder_percentile"])),
        fetched_at: now_millis(),
        refresh: RefreshAvailability {
            app_refresh: true,
            // renew.deeplol.gg requires authentication; read requests never POST.
            site_refresh: false,
            cooldown_until: remain_seconds.map(|seconds| now_millis() + seconds * 1_000),
        },
        extras: ProviderExtras::Deeplol(json!({
            "summonerIdAvailable": string(basic, &["summoner_id"]).is_some(),
            "updatedTimestamp": updated.and_then(|value| integer(value, &["updated_timestamp"])),
            "autoUpdate": updated.and_then(|value| boolean(value, &["auto_update"])),
        })),
    })
}

fn participant_array(value: &Value) -> &[Value] {
    for key in [
        "summoner_info_list",
        "participants_list",
        "participant_list",
        "participants",
        "match_summoner_list",
    ] {
        if let Some(array) = value.get(key).and_then(Value::as_array) {
            return array;
        }
    }
    if let Some(object) = value.as_object() {
        for child in object.values() {
            let array = participant_array(child);
            if !array.is_empty() {
                return array;
            }
        }
    }
    &[]
}

fn integer_array(value: &Value, keys: &[&str]) -> Vec<i64> {
    object(value, keys)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.as_i64().or_else(|| entry.as_str()?.parse().ok()))
        .collect()
}

fn integer_object_values(value: &Value, keys: &[&str]) -> Vec<i64> {
    object(value, keys)
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|values| values.values())
        .filter_map(|entry| entry.as_i64().or_else(|| entry.as_str()?.parse().ok()))
        .filter(|value| *value > 0)
        .collect()
}

fn parse_participant(value: &Value) -> MatchParticipant {
    let stats = object(value, &["final_stat_dict", "stats"]).unwrap_or(value);
    let side = string(value, &["side"]);
    MatchParticipant {
        puuid: string(value, &["puu_id", "puuid"]),
        game_name: string(value, &["riot_id_name", "game_name"]),
        tag_line: string(value, &["riot_id_tag_line", "tag_line"]),
        champion_id: integer(value, &["champion_id"]).unwrap_or_default(),
        team_id: integer(value, &["team_id"]).unwrap_or_else(|| match side.as_deref() {
            Some("BLUE") => 100,
            Some("RED") => 200,
            _ => 0,
        }),
        role: string(value, &["position", "role", "lane"]),
        win: boolean(value, &["is_win", "win"]).unwrap_or_default(),
        kills: integer(stats, &["kill", "kills"]).unwrap_or_default(),
        deaths: integer(stats, &["death", "deaths"]).unwrap_or_default(),
        assists: integer(stats, &["assist", "assists"]).unwrap_or_default(),
        items: {
            let direct = integer_array(value, &["item_id_list", "items"]);
            if direct.is_empty() {
                integer_object_values(value, &["final_item_dict"])
            } else {
                direct
            }
        },
        extras: ProviderExtras::Deeplol(json!({
            "aiScore": float(value, &["ai_score"]),
        })),
    }
}

fn parse_match(value: &Value, puuid: &str, fallback_match_id: &str) -> Result<PlayerMatch> {
    let basic = value.get("match_basic_dict").unwrap_or(value);
    let raw_participants = participant_array(value);
    let participants = raw_participants
        .iter()
        .map(parse_participant)
        .collect::<Vec<_>>();
    let mine = raw_participants
        .iter()
        .find(|entry| string(entry, &["puu_id", "puuid"]).as_deref() == Some(puuid))
        .or_else(|| raw_participants.first())
        .ok_or_else(|| ProviderError::Other("DeepLoL match omitted participants".into()))?;
    let stats = object(mine, &["final_stat_dict", "stats"]).unwrap_or(mine);
    let duration_seconds =
        integer(basic, &["game_duration", "duration", "duration_seconds"]).unwrap_or_default();
    Ok(PlayerMatch {
        match_id: string(basic, &["match_id", "game_id"])
            .unwrap_or_else(|| fallback_match_id.into()),
        started_at: timestamp_millis(
            integer(
                basic,
                &[
                    "match_creation_time",
                    "creation_timestamp",
                    "game_creation",
                    "started_at",
                    "timestamp",
                ],
            )
            .unwrap_or_default(),
        ),
        duration_seconds,
        queue_id: integer(basic, &["queue_id", "queue_type"]).unwrap_or_default(),
        remake: boolean(basic, &["is_remake", "remake"]).unwrap_or(duration_seconds < 300),
        champion_id: integer(mine, &["champion_id"]).unwrap_or_default(),
        role: string(mine, &["position", "role", "lane"]),
        win: boolean(mine, &["is_win", "win"]).unwrap_or_default(),
        kills: integer(stats, &["kill", "kills"]).unwrap_or_default(),
        deaths: integer(stats, &["death", "deaths"]).unwrap_or_default(),
        assists: integer(stats, &["assist", "assists"]).unwrap_or_default(),
        cs: integer(stats, &["cs", "total_minions_killed"]),
        items: {
            let direct = integer_array(mine, &["item_id_list", "items"]);
            if direct.is_empty() {
                integer_object_values(mine, &["final_item_dict"])
            } else {
                direct
            }
        },
        spell_ids: {
            let direct = integer_array(mine, &["spell_id_list", "spells"]);
            if direct.is_empty() {
                integer_object_values(mine, &["spell_id_dict"])
            } else {
                direct
            }
        },
        perk_ids: {
            let direct = integer_array(mine, &["perk_id_list", "runes", "perks"]);
            if direct.is_empty() {
                integer_object_values(mine, &["rune_detail_dict"])
            } else {
                direct
            }
        },
        participants,
        extras: ProviderExtras::Deeplol(json!({
            "aiScore": float(mine, &["ai_score"]),
        })),
    })
}

fn match_ids(value: &Value) -> Vec<String> {
    value
        .get("match_id_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            string(entry, &["match_id", "game_id"]).or_else(|| entry.as_str().map(str::to_owned))
        })
        .collect()
}

fn find_stat_rows<'a>(
    value: &'a Value,
    queue: Option<&str>,
    role: Option<&str>,
) -> Option<&'a [Value]> {
    for key in [
        "champion_stat_list",
        "champion_stats",
        "champion_data_list",
        "total_champion_stat_list",
    ] {
        if let Some(array) = value.get(key).and_then(Value::as_array) {
            return Some(array);
        }
    }
    let queue_key = queue
        .map(str::to_ascii_lowercase)
        .filter(|value| value != "all")
        .unwrap_or_else(|| "total".into());
    let role_key = role.unwrap_or("All");
    if let Some(stats) = value.get("counter_champion_stats") {
        for candidate_queue in [queue_key.as_str(), "total"] {
            let Some(by_role) = stats
                .get(candidate_queue)
                .and_then(|entry| entry.get("enemy_champion_stats"))
            else {
                continue;
            };
            for candidate_role in [role_key, "All"] {
                if let Some(array) = by_role.get(candidate_role).and_then(Value::as_array) {
                    return Some(array);
                }
            }
        }
    }
    value.as_array().map(Vec::as_slice)
}

fn parse_champion_stats(
    value: &Value,
    queue: Option<&str>,
    role: Option<&str>,
) -> Vec<PlayerChampionStats> {
    find_stat_rows(value, queue, role)
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let champion_id = integer(row, &["champion_id"])?;
            if champion_id == 0 {
                return None;
            }
            let games = integer(row, &["games", "game_cnt", "play", "total_games"])?;
            let wins = integer(row, &["wins", "win_cnt"])
                .or_else(|| {
                    float(row, &["win_rate"]).map(|rate| (rate * games as f64).round() as i64)
                })
                .unwrap_or_default();
            let losses = integer(row, &["losses", "loss_cnt"]).unwrap_or(games - wins);
            let mut win_rate = float(row, &["win_rate"]).unwrap_or_else(|| {
                if games == 0 {
                    0.0
                } else {
                    wins as f64 / games as f64
                }
            });
            if win_rate > 1.0 {
                win_rate /= 100.0;
            }
            let kills = float(row, &["kill", "kills"]).unwrap_or_default();
            let deaths = float(row, &["death", "deaths"]).unwrap_or_default();
            let assists = float(row, &["assist", "assists"]).unwrap_or_default();
            Some(PlayerChampionStats {
                source: "deeplol".into(),
                champion_id,
                games,
                wins,
                losses,
                win_rate,
                kda: Some(float(row, &["kda"]).unwrap_or((kills + assists) / deaths.max(1.0))),
                cs_per_minute: float(row, &["cs_per_minute", "cs_per_min"]),
                role: role
                    .map(str::to_owned)
                    .or_else(|| string(row, &["position", "role"])),
                queue: queue.unwrap_or("ALL").into(),
                extras: ProviderExtras::Deeplol(json!({
                    "aiScore": float(row, &["ai_score"]),
                })),
            })
        })
        .collect()
}

impl DeepLolProvider {
    async fn resolve_player(&self, player: &PlayerRef) -> Result<Value> {
        response_json(
            self.http
                .get(format!("{DEEPLOL}/summoner/summoner"))
                .query(&[
                    ("platform_id", platform_id(&player.platform_id)?),
                    ("riot_id_name", player.game_name.clone()),
                    ("riot_id_tag_line", player.tag_line.clone()),
                ]),
        )
        .await
    }

    async fn updated_time(&self, player: &PlayerIdentity) -> Option<Value> {
        let puuid = player.puuid.as_deref()?;
        response_json(
            self.http
                .get(format!("{DEEPLOL}/summoner/updated-time"))
                .query(&[
                    ("puu_id", puuid),
                    ("platform_id", player.platform_id.as_str()),
                ]),
        )
        .await
        .ok()
    }

    async fn current_season(&self) -> Result<String> {
        let value = response_json(self.http.get(format!("{DEEPLOL}/common/season-list"))).await?;
        value
            .get("season_list")
            .and_then(Value::as_array)
            .and_then(|seasons| seasons.iter().filter_map(Value::as_i64).max())
            .map(|season| season.to_string())
            .ok_or_else(|| ProviderError::Other("DeepLoL returned no current season".into()))
    }
}

#[async_trait]
impl PlayerStatsProvider for DeepLolProvider {
    async fn profile(&self, player: &PlayerRef, force: bool) -> Result<PlayerProfile> {
        if !force {
            if let Some(cached) =
                PlayerCache::fresh(self.player_cache.read().await.profiles.get(player))
            {
                return Ok(cached);
            }
        }
        let basic_response = self.resolve_player(player).await?;
        let basic = basic_response
            .get("summoner_basic_info_dict")
            .unwrap_or(&basic_response);
        let puuid = string(basic, &["puu_id", "puuid"])
            .ok_or_else(|| ProviderError::Other("DeepLoL profile omitted puu_id".into()))?;
        let platform = platform_id(&player.platform_id)?;
        let realtime = if let Some(summoner_id) = string(basic, &["summoner_id"]) {
            response_json(
                self.http
                    .get(format!("{DEEPLOL}/summoner/summoner-realtime"))
                    .query(&[
                        ("platform_id", platform.as_str()),
                        ("summoner_id", summoner_id.as_str()),
                        ("puu_id", puuid.as_str()),
                    ]),
            )
            .await
            .ok()
        } else {
            None
        };
        let identity = PlayerIdentity {
            platform_id: platform,
            game_name: player.game_name.clone(),
            tag_line: player.tag_line.clone(),
            puuid: Some(puuid),
        };
        let updated = self.updated_time(&identity).await;
        let profile = parse_profile(player, &basic_response, realtime.as_ref(), updated.as_ref())?;
        self.player_cache
            .write()
            .await
            .profiles
            .insert(player.clone(), (Instant::now(), profile.clone()));
        Ok(profile)
    }

    async fn recent_matches(
        &self,
        player: &PlayerRef,
        cursor: Option<&str>,
        queue: Option<i64>,
        force: bool,
    ) -> Result<MatchPage> {
        let cursor = cursor.unwrap_or("0");
        let cache_key = (player.clone(), cursor.into(), queue);
        if !force {
            if let Some(cached) =
                PlayerCache::fresh(self.player_cache.read().await.matches.get(&cache_key))
            {
                return Ok(cached);
            }
        }
        let profile = self.profile(player, false).await?;
        let puuid = profile.identity.puuid.as_deref().ok_or_else(|| {
            ProviderError::Other("DeepLoL profile omitted player identity".into())
        })?;
        let offset = cursor
            .parse::<usize>()
            .map_err(|_| ProviderError::Other(format!("invalid DeepLoL cursor: {cursor}")))?;
        let platform = platform_id(&player.platform_id)?;
        let queue_type = queue.map_or_else(|| "ALL".into(), |value| value.to_string());
        let list = response_json(self.http.get(format!("{DEEPLOL}/match/matches")).query(&[
            ("puu_id", puuid.to_owned()),
            ("platform_id", platform.clone()),
            ("offset", offset.to_string()),
            ("count", MATCH_PAGE_SIZE.to_string()),
            ("queue_type", queue_type),
            ("champion_id", "0".into()),
            ("only_list", "1".into()),
            ("last_updated_at", "0".into()),
        ]))
        .await?;
        let ids = match_ids(&list);
        let ids_to_hydrate = ids.clone();
        let hydrated = stream::iter(ids_to_hydrate.into_iter().map(|match_id| {
            let platform = platform.clone();
            async move {
                let result = response_json(
                    self.http
                        .get(format!("{DEEPLOL}/match/match-cached"))
                        .query(&[
                            ("match_id", match_id.as_str()),
                            ("platform_id", platform.as_str()),
                        ]),
                )
                .await
                .and_then(|value| parse_match(&value, puuid, &match_id));
                (match_id, result)
            }
        }))
        .buffer_unordered(MATCH_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        let by_id = hydrated.into_iter().collect::<HashMap<_, _>>();
        let mut matches = vec![];
        let mut partial_failures = vec![];
        for match_id in &ids {
            match by_id.get(match_id) {
                Some(Ok(value)) => matches.push(value.clone()),
                Some(Err(error)) => partial_failures.push(MatchFailure {
                    match_id: match_id.clone(),
                    message: error.to_string(),
                    retryable: true,
                }),
                None => partial_failures.push(MatchFailure {
                    match_id: match_id.clone(),
                    message: "match hydration did not complete".into(),
                    retryable: true,
                }),
            }
        }
        let page = MatchPage {
            source: "deeplol".into(),
            matches,
            next_cursor: (ids.len() == MATCH_PAGE_SIZE).then(|| (offset + ids.len()).to_string()),
            partial_failures,
            fetched_at: now_millis(),
        };
        self.player_cache
            .write()
            .await
            .matches
            .insert(cache_key, (Instant::now(), page.clone()));
        Ok(page)
    }

    async fn champion_stats(
        &self,
        player: &PlayerRef,
        season: Option<&str>,
        queue: Option<&str>,
        role: Option<&str>,
        force: bool,
    ) -> Result<Vec<PlayerChampionStats>> {
        let season = match season {
            Some(value) => value.into(),
            None => self.current_season().await?,
        };
        let cache_key = (
            player.clone(),
            season.clone(),
            queue.map(str::to_owned),
            role.map(str::to_owned),
        );
        if !force {
            if let Some(cached) =
                PlayerCache::fresh(self.player_cache.read().await.champions.get(&cache_key))
            {
                return Ok(cached);
            }
        }
        let profile = self.profile(player, false).await?;
        let puuid = profile.identity.puuid.as_deref().ok_or_else(|| {
            ProviderError::Other("DeepLoL profile omitted player identity".into())
        })?;
        let response = response_json(
            self.http
                .get(format!("{DEEPLOL}/summoner/champion-stat"))
                .query(&[
                    ("puu_id", puuid),
                    ("platform_id", profile.identity.platform_id.as_str()),
                    ("season", season.as_str()),
                ]),
        )
        .await?;
        let stats = parse_champion_stats(&response, queue, role);
        self.player_cache
            .write()
            .await
            .champions
            .insert(cache_key, (Instant::now(), stats.clone()));
        Ok(stats)
    }

    async fn refresh(&self, player: &PlayerRef) -> Result<RefreshResult> {
        self.player_cache.write().await.invalidate(player);
        // Prove read freshness without crossing the authenticated mutation boundary.
        self.profile(player, true).await?;
        Ok(RefreshResult {
            source: "deeplol".into(),
            cache_invalidated: true,
            mutation_performed: false,
            refreshed_at: now_millis(),
        })
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            builds: true,
            player_profile: true,
            match_history: true,
            champion_stats: true,
            live_game: false,
            direct_api: true,
            site_refresh: false,
            regions: [
                "KR", "JP1", "NA1", "EUW1", "EUN1", "OC1", "BR1", "LA1", "LA2", "TR1", "RU", "PH2",
                "SG2", "TH2", "TW2", "VN2",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_mapping_never_sends_unnumbered_jp() {
        assert_eq!(platform_id("JP").unwrap(), "JP1");
        assert_eq!(platform_id("kr1").unwrap(), "KR");
        assert!(platform_id("PBE").is_err());
    }

    #[test]
    fn profile_fixture_handles_empty_summoner_id_and_previous_tiers() {
        let raw = json!({"summoner_basic_info_dict": {
            "puu_id": "p", "summoner_id": "", "level": 920, "profile_id": 6,
            "riot_id_name": "Hide on bush", "riot_id_tag_line": "KR1",
            "previous_season_tier_list": [{"season": 25, "tier": "MASTER", "division": 1, "lp": 225}]
        }});
        let profile = parse_profile(
            &PlayerRef {
                platform_id: "KR".into(),
                game_name: "Hide on bush".into(),
                tag_line: "KR1".into(),
            },
            &raw,
            None,
            Some(&json!({"remain_second": 30, "auto_update": false})),
        )
        .unwrap();
        assert_eq!(profile.identity.puuid.as_deref(), Some("p"));
        assert_eq!(profile.previous_seasons.len(), 1);
        assert!(profile.ranks.is_empty());
        assert!(!profile.refresh.site_refresh);
    }

    #[test]
    fn match_fixture_extracts_target_and_all_participants() {
        let raw = json!({
            "match_basic_dict": {"match_id": "KR_1", "game_duration": 1200, "queue_id": 420},
            "summoner_info_list": [
                {"puu_id": "mine", "champion_id": 103, "win": 1, "kill": 8, "death": 2, "assist": 7},
                {"puu_id": "other", "champion_id": 238, "win": 0}
            ]
        });
        let parsed = parse_match(&raw, "mine", "fallback").unwrap();
        assert_eq!(parsed.match_id, "KR_1");
        assert_eq!(parsed.champion_id, 103);
        assert_eq!(parsed.participants.len(), 2);
        assert!(!parsed.remake);
    }

    #[test]
    fn match_list_uses_actual_count_for_offset_cursor() {
        let value = json!({"match_id_list": [{"match_id": "a"}, {"match_id": "b"}]});
        assert_eq!(match_ids(&value), vec!["a", "b"]);
    }

    #[test]
    fn champion_fixture_preserves_ai_score_as_deeplol_extra() {
        let value = json!({"champion_stat_list": [{
            "champion_id": 103, "games": 10, "win_cnt": 6,
            "kill": 5.0, "death": 2.0, "assist": 7.0, "ai_score": 72.5
        }]});
        let stats = parse_champion_stats(&value, Some("RANKED"), Some("Middle"));
        assert_eq!(stats[0].win_rate, 0.6);
        assert_eq!(stats[0].kda, Some(6.0));
        assert!(matches!(stats[0].extras, ProviderExtras::Deeplol(_)));
    }

    #[test]
    fn live_schema_match_fixture_maps_final_stats_and_provider_extras() {
        let raw = json!({
            "match_basic_dict": {
                "match_id": "KR_8294576001", "creation_timestamp": 1783779694,
                "game_duration": 2094, "queue_id": 420, "is_remake": false
            },
            "participants_list": [{
                "puu_id": "mine", "riot_id_name": "Player", "riot_id_tag_line": "KR1",
                "champion_id": 23, "side": "BLUE", "position": "Top", "is_win": true,
                "final_stat_dict": {"kills": 7, "deaths": 3, "assists": 9, "cs": 296},
                "final_item_dict": {"item_0": 3074, "item_1": 6675, "item_6": 3340, "mythic": 0},
                "spell_id_dict": {"spell_1": 4, "spell_2": 14},
                "rune_detail_dict": {"perk_0": 9923, "perk_primary_style": 8100},
                "ai_score": 64.25
            }]
        });
        let parsed = parse_match(&raw, "mine", "fallback").unwrap();
        assert_eq!(parsed.started_at, 1_783_779_694_000);
        assert_eq!(
            (parsed.kills, parsed.deaths, parsed.assists, parsed.cs),
            (7, 3, 9, Some(296))
        );
        assert_eq!(parsed.items, vec![3074, 6675, 3340]);
        assert_eq!(parsed.spell_ids, vec![4, 14]);
        assert_eq!(parsed.participants[0].team_id, 100);
        assert!(matches!(parsed.extras, ProviderExtras::Deeplol(_)));
    }

    #[test]
    fn live_schema_champion_fixture_selects_queue_and_role_without_aggregate_row() {
        let raw = json!({"counter_champion_stats": {
            "total": {"enemy_champion_stats": {"All": [
                {"champion_id": 0, "games": 546, "wins": 298, "losses": 248},
                {"champion_id": 13, "games": 31, "wins": 18, "losses": 13,
                 "win_rate": 58.06, "kda": 2.97, "cs_per_min": 9.02, "ai_score": 54.65}
            ]}},
            "ranked_solo_5x5": {"enemy_champion_stats": {"Middle": [
                {"champion_id": 103, "games": 29, "wins": 12, "losses": 17,
                 "win_rate": 41.38, "kda": 2.36, "cs_per_min": 8.63}
            ]}}
        }});
        let stats = parse_champion_stats(&raw, Some("RANKED_SOLO_5X5"), Some("Middle"));
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].champion_id, 103);
        assert!((stats[0].win_rate - 0.4138).abs() < f64::EPSILON);
    }

    #[test]
    #[ignore]
    fn live_player_stats_acceptance() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        runtime.block_on(async {
            let provider =
                DeepLolProvider::new(std::sync::Arc::new(overlay_ddragon::DdragonClient::new()))
                    .expect("provider");
            let player = PlayerRef {
                platform_id: "KR".into(),
                game_name: "Hide on bush".into(),
                tag_line: "KR1".into(),
            };
            let profile = provider.profile(&player, true).await.expect("profile");
            assert_eq!(profile.identity.game_name, "Hide on bush");
            assert!(profile.identity.puuid.is_some());

            let first = provider
                .recent_matches(&player, None, None, true)
                .await
                .expect("first match page");
            assert_eq!(first.matches.len() + first.partial_failures.len(), 20);
            let cursor = first.next_cursor.as_deref().expect("second-page cursor");
            let second = provider
                .recent_matches(&player, Some(cursor), None, true)
                .await
                .expect("second match page");
            assert_eq!(second.matches.len() + second.partial_failures.len(), 20);

            let champions = provider
                .champion_stats(&player, None, None, None, true)
                .await
                .expect("champion stats");
            assert!(!champions.is_empty());
            assert!(champions.iter().all(|entry| entry.champion_id > 0));
            println!(
                "DEEPLOL PLAYER LIVE OK: profile={} first={} second={} champions={}",
                profile.identity.game_name,
                first.matches.len(),
                second.matches.len(),
                champions.len()
            );
        });
    }
}
