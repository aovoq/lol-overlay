//! Player statistics from OP.GG's official MCP and public app data surfaces.
//!
//! The transport is JSON. OP.GG intentionally returns selected data in a
//! compact constructor notation inside the MCP text content, so this module
//! parses that notation with a small data parser instead of scraping HTML.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use chrono::DateTime;
use overlay_ddragon::normalize;
use overlay_provider::{PlayerStatsProvider, ProviderCapabilities, ProviderError, Result};
use overlay_types::{
    MatchFailure, MatchPage, MatchParticipant, PlayerChampionStats, PlayerIdentity, PlayerMatch,
    PlayerProfile, PlayerRef, ProviderExtras, RankedEntry, RefreshAvailability, RefreshResult,
    SeasonRank,
};
use serde_json::{json, Value};

use super::OpggProvider;

const MCP_URL: &str = "https://mcp-api.op.gg/mcp";
const PAGE_SIZE: usize = 20;
const PLAYER_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const RETRY_ATTEMPTS: usize = 2;
const RETRY_DELAY: Duration = Duration::from_millis(250);

#[derive(Default)]
pub(super) struct OpggPlayerCache {
    profiles: tokio::sync::RwLock<HashMap<PlayerRef, (Instant, CompactValue)>>,
    request: tokio::sync::Mutex<()>,
}

async fn send_with_retry(request: reqwest::RequestBuilder) -> Result<reqwest::Response> {
    let mut attempt = 0;
    loop {
        attempt += 1;
        let Some(next) = request.try_clone() else {
            return Err(ProviderError::Other(
                "OP.GG request could not be cloned for retry".into(),
            ));
        };
        match next.send().await {
            Ok(response) if response.status().is_server_error() && attempt < RETRY_ATTEMPTS => {
                tokio::time::sleep(RETRY_DELAY).await;
            }
            Ok(response) => return Ok(response),
            Err(error)
                if (error.is_timeout() || error.is_connect()) && attempt < RETRY_ATTEMPTS =>
            {
                tokio::time::sleep(RETRY_DELAY).await;
            }
            Err(error) => {
                return Err(if error.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Http(error)
                });
            }
        }
    }
}

const PROFILE_FIELDS: &[&str] = &[
    "data.summoner.{game_name,tagline,level,profile_image_url,puuid,updated_at}",
    "data.summoner.ladder_rank.{rank,total}",
    "data.summoner.league_stats[].{game_type,win,lose,updated_at}",
    "data.summoner.league_stats[].tier_info.{division,lp,tier}",
    "data.summoner.previous_season_tiers[].season_id",
    "data.summoner.previous_season_tiers[].rank_entries[].game_type",
    "data.summoner.previous_season_tiers[].rank_entries[].rank_info.{division,lp,tier,win,lose}",
    "data.summoner.ranked_most_champions.my_champion_stats[].{champion_name,play,win,lose}",
];

#[derive(Debug, Clone, PartialEq)]
enum CompactValue {
    Call(String, Vec<Self>),
    Array(Vec<Self>),
    String(String),
    Number(f64),
    Bool(bool),
    Null,
}

struct CompactParser<'a> {
    input: &'a [u8],
    at: usize,
}

impl<'a> CompactParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input: input.as_bytes(),
            at: 0,
        }
    }

    fn parse(mut self) -> Result<CompactValue> {
        let value = self.value()?;
        self.space();
        if self.at != self.input.len() {
            return Err(ProviderError::Other(format!(
                "OP.GG compact response has trailing data at byte {}",
                self.at
            )));
        }
        Ok(value)
    }

    fn value(&mut self) -> Result<CompactValue> {
        self.space();
        match self.peek() {
            Some(b'"') => self.string().map(CompactValue::String),
            Some(b'[') => self.array(),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => self.identifier_or_call(),
            _ => Err(ProviderError::Other(format!(
                "invalid OP.GG compact value at byte {}",
                self.at
            ))),
        }
    }

    fn array(&mut self) -> Result<CompactValue> {
        self.expect(b'[')?;
        let values = self.arguments(b']')?;
        Ok(CompactValue::Array(values))
    }

    fn identifier_or_call(&mut self) -> Result<CompactValue> {
        let start = self.at;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
        ) {
            self.at += 1;
        }
        let name = std::str::from_utf8(&self.input[start..self.at])
            .map_err(|error| ProviderError::Other(error.to_string()))?;
        self.space();
        if self.peek() == Some(b'(') {
            self.at += 1;
            return Ok(CompactValue::Call(name.into(), self.arguments(b')')?));
        }
        match name {
            "null" => Ok(CompactValue::Null),
            "true" => Ok(CompactValue::Bool(true)),
            "false" => Ok(CompactValue::Bool(false)),
            _ => Err(ProviderError::Other(format!(
                "unexpected OP.GG compact identifier: {name}"
            ))),
        }
    }

    fn arguments(&mut self, end: u8) -> Result<Vec<CompactValue>> {
        let mut values = vec![];
        self.space();
        if self.peek() == Some(end) {
            self.at += 1;
            return Ok(values);
        }
        loop {
            values.push(self.value()?);
            self.space();
            match self.peek() {
                Some(b',') => self.at += 1,
                Some(value) if value == end => {
                    self.at += 1;
                    return Ok(values);
                }
                _ => {
                    return Err(ProviderError::Other(format!(
                        "unterminated OP.GG compact list at byte {}",
                        self.at
                    )))
                }
            }
        }
    }

    fn string(&mut self) -> Result<String> {
        let start = self.at;
        self.at += 1;
        let mut escaped = false;
        while let Some(byte) = self.peek() {
            self.at += 1;
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                let json = std::str::from_utf8(&self.input[start..self.at])
                    .map_err(|error| ProviderError::Other(error.to_string()))?;
                return serde_json::from_str(json).map_err(Into::into);
            }
        }
        Err(ProviderError::Other(
            "unterminated OP.GG compact string".into(),
        ))
    }

    fn number(&mut self) -> Result<CompactValue> {
        let start = self.at;
        while matches!(
            self.peek(),
            Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9')
        ) {
            self.at += 1;
        }
        let raw = std::str::from_utf8(&self.input[start..self.at])
            .map_err(|error| ProviderError::Other(error.to_string()))?;
        raw.parse::<f64>()
            .map(CompactValue::Number)
            .map_err(|error| ProviderError::Other(error.to_string()))
    }

    fn expect(&mut self, expected: u8) -> Result<()> {
        if self.peek() != Some(expected) {
            return Err(ProviderError::Other(format!(
                "expected '{}' at byte {}",
                expected as char, self.at
            )));
        }
        self.at += 1;
        Ok(())
    }

    fn space(&mut self) {
        while self.peek().is_some_and(|value| value.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.at).copied()
    }
}

fn args<'a>(value: &'a CompactValue, name: &str) -> Result<&'a [CompactValue]> {
    match value {
        CompactValue::Call(actual, values) if actual == name => Ok(values),
        _ => Err(ProviderError::Other(format!("expected OP.GG {name} value"))),
    }
}

fn array(value: &CompactValue) -> Result<&[CompactValue]> {
    match value {
        CompactValue::Array(values) => Ok(values),
        _ => Err(ProviderError::Other("expected OP.GG array".into())),
    }
}

fn text(value: Option<&CompactValue>) -> Option<String> {
    match value? {
        CompactValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn integer(value: Option<&CompactValue>) -> Option<i64> {
    match value? {
        CompactValue::Number(value) if value.is_finite() => Some(*value as i64),
        _ => None,
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn queue_id(game_type: &str) -> i64 {
    match game_type {
        "SOLORANKED" => 420,
        "FLEXRANKED" => 440,
        "NORMAL" => 400,
        "ARAM" => 450,
        "ARENA" => 1700,
        _ => 0,
    }
}

fn platform(raw: &str) -> Result<&'static str> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "KR" | "KR1" => Ok("KR"),
        "JP" | "JP1" => Ok("JP"),
        "NA" | "NA1" => Ok("NA"),
        "EUW" | "EUW1" => Ok("EUW"),
        "EUNE" | "EUN1" => Ok("EUNE"),
        "OCE" | "OC1" => Ok("OCE"),
        "BR" | "BR1" => Ok("BR"),
        "LAN" | "LA1" => Ok("LAN"),
        "LAS" | "LA2" => Ok("LAS"),
        "TR" | "TR1" => Ok("TR"),
        "RU" => Ok("RU"),
        "PH" | "PH2" => Ok("PH"),
        "SG" | "SG2" => Ok("SG"),
        "TH" | "TH2" => Ok("TH"),
        "TW" | "TW2" => Ok("TW"),
        "VN" | "VN2" => Ok("VN"),
        _ => Err(ProviderError::InvalidPlayerRequest(format!(
            "unsupported OP.GG platform: {raw}"
        ))),
    }
}

fn public_profile_url(player: &PlayerRef) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse("https://op.gg/lol/summoners/")
        .map_err(|error| ProviderError::Other(error.to_string()))?;
    url.path_segments_mut()
        .map_err(|()| ProviderError::Other("OP.GG profile URL cannot be a base".into()))?
        .pop_if_empty()
        .push(&platform(&player.platform_id)?.to_ascii_lowercase())
        .push(&format!("{}-{}", player.game_name, player.tag_line));
    Ok(url)
}

fn games_action_id(bundle: &str) -> Option<String> {
    let end = bundle.find("\"getGames\"")?;
    let prefix = &bundle[end.saturating_sub(240)..end];
    prefix
        .split(|character: char| !character.is_ascii_hexdigit())
        .rev()
        .find(|candidate| candidate.len() == 42)
        .map(str::to_owned)
}

fn action_json(body: &str) -> Result<Value> {
    let payload = body
        .lines()
        .find_map(|line| line.strip_prefix("1:"))
        .ok_or_else(|| ProviderError::InvalidData("OP.GG action omitted Flight value 1".into()))?;
    serde_json::from_str(payload).map_err(|error| {
        ProviderError::InvalidData(format!(
            "OP.GG action returned malformed Flight JSON: {error}"
        ))
    })
}

fn action_queue(queue: Option<i64>) -> &'static str {
    match queue {
        Some(420) => "SOLORANKED",
        Some(440) => "FLEXRANKED",
        Some(450) => "ARAM",
        Some(1700) => "ARENA",
        _ => "TOTAL",
    }
}

fn value_integer(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_u64().map(|raw| raw as i64))
        })
        .unwrap_or_default()
}

fn action_participant(value: &Value) -> Option<MatchParticipant> {
    let summoner = value.get("summoner")?;
    let stats = value.get("stats")?;
    let team = value.get("team_key").and_then(Value::as_str);
    Some(MatchParticipant {
        puuid: summoner
            .get("puuid")
            .and_then(Value::as_str)
            .map(str::to_owned),
        game_name: summoner
            .get("game_name")
            .and_then(Value::as_str)
            .map(str::to_owned),
        tag_line: summoner
            .get("tagline")
            .and_then(Value::as_str)
            .map(str::to_owned),
        champion_id: value_integer(value.get("champion_id")),
        team_id: match team {
            Some("BLUE") => 100,
            Some("RED") => 200,
            _ => 0,
        },
        role: value
            .get("position")
            .or_else(|| value.get("role"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        win: stats.get("result").and_then(Value::as_str) == Some("WIN"),
        kills: value_integer(stats.get("kill")),
        deaths: value_integer(stats.get("death")),
        assists: value_integer(stats.get("assist")),
        items: value
            .get("items")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
            .collect(),
        extras: ProviderExtras::Opgg(json!({"opScore": stats.get("op_score")})),
    })
}

fn collect_action_participants(value: &Value, output: &mut Vec<MatchParticipant>) {
    if let Some(participant) = action_participant(value) {
        output.push(participant);
        return;
    }
    match value {
        Value::Array(values) => {
            for value in values {
                collect_action_participants(value, output);
            }
        }
        Value::Object(values) => {
            for value in values.values() {
                collect_action_participants(value, output);
            }
        }
        _ => {}
    }
}

fn parse_action_match(value: &Value, puuid: &str) -> Result<PlayerMatch> {
    let mut participants = vec![];
    for key in ["team_blue", "team_red", "team_arena"] {
        if let Some(team) = value.get(key) {
            collect_action_participants(team, &mut participants);
        }
    }
    let mine = participants
        .iter()
        .find(|participant| participant.puuid.as_deref() == Some(puuid))
        .cloned()
        .ok_or_else(|| ProviderError::InvalidData("OP.GG game omitted searched player".into()))?;
    let stats = value.get("stats").unwrap_or(&Value::Null);
    let game_type = value
        .pointer("/game_type/game_type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let started_at = value
        .get("created_at")
        .and_then(Value::as_str)
        .and_then(|raw| DateTime::parse_from_rfc3339(raw).ok())
        .map(|date| date.timestamp_millis())
        .unwrap_or_default();
    let duration_seconds = value_integer(value.get("game_length"));
    Ok(PlayerMatch {
        match_id: value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        started_at,
        duration_seconds,
        queue_id: queue_id(game_type),
        remake: duration_seconds < 300,
        champion_id: mine.champion_id,
        role: mine.role.clone(),
        win: mine.win,
        kills: mine.kills,
        deaths: mine.deaths,
        assists: mine.assists,
        cs: stats
            .pointer("/cs/totalCs")
            .map(|value| value_integer(Some(value))),
        items: mine.items.clone(),
        spell_ids: value
            .get("spells")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|spell| spell.get("id").and_then(Value::as_i64))
            .collect(),
        perk_ids: value
            .get("runes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|rune| rune.get("id").and_then(Value::as_i64))
            .collect(),
        participants,
        extras: ProviderExtras::Opgg(json!({
            "opScore": stats.get("op_score"),
            "opBadge": value.get("opBadge"),
        })),
    })
}

impl OpggProvider {
    async fn mcp_call(&self, tool: &str, arguments: Value) -> Result<CompactValue> {
        let response = send_with_retry(
            self.api
                .http()
                .post(MCP_URL)
                .header("accept", "application/json, text/event-stream")
                .json(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": tool, "arguments": arguments}
                })),
        )
        .await?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let body = response.text().await?;
        if !status.is_success() {
            return Err(match status.as_u16() {
                404 => ProviderError::PlayerNotFound,
                422 => ProviderError::InvalidPlayerRequest("OP.GG rejected player input".into()),
                429 => ProviderError::RateLimited {
                    retry_after: retry_after.and_then(|value| value.parse().ok()),
                },
                code => ProviderError::Other(format!("player-http:{code}")),
            });
        }
        let value: Value = serde_json::from_str(&body)?;
        if let Some(error) = value.get("error") {
            return Err(ProviderError::Other(format!("OP.GG MCP error: {error}")));
        }
        let content = value
            .pointer("/result/content/0/text")
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::Other("OP.GG MCP omitted text content".into()))?;
        let compact = content
            .rsplit_once("\n\n")
            .map_or(content, |(_, value)| value);
        CompactParser::new(compact).parse()
    }

    async fn player_champion_id(&self, name: &str) -> Option<i64> {
        self.ddragon
            .champions()
            .await
            .ok()?
            .name_to_id
            .get(&normalize(name))
            .copied()
    }

    async fn profile_compact(&self, player: &PlayerRef, force: bool) -> Result<CompactValue> {
        let request_started = Instant::now();
        if !force {
            if let Some((loaded, value)) = self.player_cache.profiles.read().await.get(player) {
                if loaded.elapsed() < PLAYER_CACHE_TTL {
                    return Ok(value.clone());
                }
            }
        }

        let _request = self.player_cache.request.lock().await;
        if let Some((loaded, value)) = self.player_cache.profiles.read().await.get(player) {
            let refreshed_by_concurrent_request = *loaded >= request_started;
            if loaded.elapsed() < PLAYER_CACHE_TTL && (!force || refreshed_by_concurrent_request) {
                return Ok(value.clone());
            }
        }

        let value = self
            .mcp_call(
                "lol_get_summoner_profile",
                json!({
                    "game_name": player.game_name, "tag_line": player.tag_line,
                    "region": platform(&player.platform_id)?, "lang": "en_US",
                    "desired_output_fields": PROFILE_FIELDS
                }),
            )
            .await?;
        self.player_cache
            .profiles
            .write()
            .await
            .insert(player.clone(), (Instant::now(), value.clone()));
        Ok(value)
    }

    async fn discover_games_action(&self, player: &PlayerRef) -> Result<String> {
        if let Some(action) = self.games_action.read().await.clone() {
            return Ok(action);
        }
        let response = self
            .api
            .http()
            .get(public_profile_url(player)?)
            .send()
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Err(ProviderError::PlayerNotFound);
        }
        let html = response.error_for_status()?.text().await?;
        let mut bundles = html
            .split('"')
            .filter(|part| {
                part.starts_with("https://c-lol-web.op.gg/")
                    && std::path::Path::new(part)
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("js"))
            })
            .map(str::to_owned)
            .collect::<Vec<_>>();
        bundles.sort_by_key(|url| !url.contains("/7716-"));
        bundles.dedup();
        for url in bundles {
            let Ok(response) = self.api.http().get(url).send().await else {
                continue;
            };
            let Ok(response) = response.error_for_status() else {
                continue;
            };
            let Ok(bundle) = response.text().await else {
                continue;
            };
            if let Some(action) = games_action_id(&bundle) {
                *self.games_action.write().await = Some(action.clone());
                return Ok(action);
            }
        }
        Err(ProviderError::InvalidData(
            "OP.GG public app omitted the getGames server action".into(),
        ))
    }

    async fn action_matches(
        &self,
        player: &PlayerRef,
        puuid: &str,
        cursor: Option<&str>,
        queue: Option<i64>,
    ) -> Result<Value> {
        let action = self.discover_games_action(player).await?;
        let response = send_with_retry(
            self.api
                .http()
                .post(public_profile_url(player)?)
                .header("accept", "text/x-component")
                .header("content-type", "text/plain;charset=UTF-8")
                .header("next-action", action)
                .body(
                    json!([{
                        "locale": "en",
                        "region": platform(&player.platform_id)?.to_ascii_lowercase(),
                        "puuid": puuid,
                        "gameType": action_queue(queue),
                        "endedAt": cursor.unwrap_or_default(),
                        "champion": Value::Null,
                    }])
                    .to_string(),
                ),
        )
        .await?;
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited { retry_after });
        }
        let body = response.error_for_status()?.text().await?;
        action_json(&body)
    }
}

fn summoner_node(root: &CompactValue) -> Result<&[CompactValue]> {
    let data = args(root, "LolGetSummonerProfile")?
        .first()
        .ok_or_else(|| ProviderError::Other("OP.GG profile omitted data".into()))?;
    let summoner = args(data, "Data")?
        .first()
        .ok_or_else(|| ProviderError::Other("OP.GG profile omitted summoner".into()))?;
    args(summoner, "Summoner")
}

fn parse_profile(player: &PlayerRef, root: &CompactValue) -> Result<PlayerProfile> {
    let values = summoner_node(root)?;
    let mut ranks = vec![];
    if let Some(CompactValue::Array(entries)) = values.get(7) {
        for entry in entries {
            let fields = args(entry, "LeagueStat")?;
            let tier = fields.get(4).and_then(|value| args(value, "TierInfo").ok());
            ranks.push(RankedEntry {
                queue: text(fields.first()).unwrap_or_default(),
                tier: tier.and_then(|fields| text(fields.get(2))),
                division: tier.and_then(|fields| text(fields.first())),
                lp: tier.and_then(|fields| integer(fields.get(1))),
                wins: integer(fields.get(1)),
                losses: integer(fields.get(2)),
            });
        }
    }
    let mut previous_seasons = vec![];
    if let Some(CompactValue::Array(seasons)) = values.get(8) {
        for season in seasons {
            let fields = args(season, "PreviousSeasonTier")?;
            let season_id = integer(fields.first()).unwrap_or_default().to_string();
            let Some(CompactValue::Array(entries)) = fields.get(1) else {
                continue;
            };
            for entry in entries {
                let CompactValue::Call(_, fields) = entry else {
                    continue;
                };
                let rank = fields.get(1).and_then(|value| match value {
                    CompactValue::Call(_, fields) => Some(fields.as_slice()),
                    _ => None,
                });
                previous_seasons.push(SeasonRank {
                    season: season_id.clone(),
                    queue: text(fields.first()).unwrap_or_default(),
                    division: rank.and_then(|fields| text(fields.first())),
                    lp: rank.and_then(|fields| integer(fields.get(1))),
                    tier: rank.and_then(|fields| text(fields.get(2))),
                });
            }
        }
    }
    let ladder = values
        .get(6)
        .and_then(|value| args(value, "LadderRank").ok());
    Ok(PlayerProfile {
        source: "opgg".into(),
        identity: PlayerIdentity {
            platform_id: player.platform_id.clone(),
            game_name: text(values.first()).unwrap_or_else(|| player.game_name.clone()),
            tag_line: text(values.get(1)).unwrap_or_else(|| player.tag_line.clone()),
            puuid: text(values.get(4)),
        },
        level: integer(values.get(2)),
        profile_icon_id: text(values.get(3)).and_then(|url| {
            url.rsplit("profileIcon")
                .next()?
                .trim_end_matches(".jpg")
                .parse()
                .ok()
        }),
        ranks,
        previous_seasons,
        ladder_rank: ladder.and_then(|fields| integer(fields.first())),
        ladder_percentile: ladder.and_then(|fields| {
            let rank = integer(fields.first())? as f64;
            let total = integer(fields.get(1))? as f64;
            (total > 0.0).then_some(rank / total * 100.0)
        }),
        fetched_at: now_millis(),
        refresh: RefreshAvailability {
            app_refresh: true,
            site_refresh: false,
            cooldown_until: None,
        },
        extras: ProviderExtras::Opgg(json!({"updatedAt": text(values.get(5))})),
    })
}

#[async_trait]
impl PlayerStatsProvider for OpggProvider {
    async fn profile(&self, player: &PlayerRef, force: bool) -> Result<PlayerProfile> {
        parse_profile(player, &self.profile_compact(player, force).await?)
    }

    async fn recent_matches(
        &self,
        player: &PlayerRef,
        cursor: Option<&str>,
        queue: Option<i64>,
        force: bool,
    ) -> Result<MatchPage> {
        let profile = self.profile(player, force).await?;
        let puuid = profile
            .identity
            .puuid
            .ok_or_else(|| ProviderError::Other("OP.GG profile omitted puuid".into()))?;
        let root = self.action_matches(player, &puuid, cursor, queue).await?;
        let games = root
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::InvalidData("OP.GG action omitted games".into()))?;
        let mut matches = vec![];
        let mut partial_failures = vec![];
        for game in games {
            match parse_action_match(game, &puuid) {
                Ok(value) => matches.push(value),
                Err(error) => partial_failures.push(MatchFailure {
                    match_id: game
                        .get("id")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                        .to_owned(),
                    message: error.to_string(),
                    retryable: true,
                }),
            }
        }
        matches.sort_by_key(|entry| std::cmp::Reverse(entry.started_at));
        Ok(MatchPage {
            source: "opgg".into(),
            matches,
            next_cursor: (games.len() == PAGE_SIZE)
                .then(|| {
                    root.pointer("/meta/last_game_created_at")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                })
                .flatten(),
            partial_failures,
            fetched_at: now_millis(),
        })
    }

    async fn champion_stats(
        &self,
        player: &PlayerRef,
        _season: Option<&str>,
        queue: Option<&str>,
        role: Option<&str>,
        force: bool,
    ) -> Result<Vec<PlayerChampionStats>> {
        let root = self.profile_compact(player, force).await?;
        let values = summoner_node(&root)?;
        let entries = values
            .get(9)
            .and_then(|value| args(value, "RankedMostChampions").ok())
            .and_then(|fields| fields.first())
            .and_then(|value| array(value).ok());
        let Some(entries) = entries else {
            return Ok(vec![]);
        };
        let mut result = vec![];
        for entry in entries {
            let fields = args(entry, "MyChampionStat")?;
            let Some(name) = text(fields.first()) else {
                continue;
            };
            let Some(champion_id) = self.player_champion_id(&name).await else {
                continue;
            };
            let games = integer(fields.get(1)).ok_or_else(|| {
                ProviderError::InvalidData(format!("OP.GG champion stats omitted games for {name}"))
            })?;
            let wins = integer(fields.get(2)).ok_or_else(|| {
                ProviderError::InvalidData(format!("OP.GG champion stats omitted wins for {name}"))
            })?;
            let losses = integer(fields.get(3)).ok_or_else(|| {
                ProviderError::InvalidData(format!(
                    "OP.GG champion stats omitted losses for {name}"
                ))
            })?;
            result.push(PlayerChampionStats {
                source: "opgg".into(),
                champion_id,
                games,
                wins,
                losses,
                win_rate: if games > 0 {
                    wins as f64 / games as f64
                } else {
                    0.0
                },
                kda: None,
                cs_per_minute: None,
                role: role.map(str::to_owned),
                queue: queue.unwrap_or("RANKED").into(),
                extras: ProviderExtras::Opgg(json!({})),
            });
        }
        Ok(result)
    }

    async fn refresh(&self, player: &PlayerRef) -> Result<RefreshResult> {
        self.player_cache.profiles.write().await.remove(player);
        Ok(RefreshResult {
            source: "opgg".into(),
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
            // Profile data is direct MCP JSON-RPC, but match pagination discovers
            // and invokes a first-party Flight action from the public app.
            direct_api: false,
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

    fn contract_fixture() -> overlay_provider::PlayerProviderContractFixture {
        let player = PlayerRef {
            platform_id: "KR".into(),
            game_name: "Contract Player".into(),
            tag_line: "KR1".into(),
        };
        let compact = CompactParser::new(
            r#"LolGetSummonerProfile(Data(Summoner("Contract Player","KR1",100,"https://x/profileIcon6.jpg","contract-puuid","2026-07-14T00:00:00+09:00",LadderRank(10,1000),[LeagueStat("SOLORANKED",6,4,null,TierInfo(1,50,"GOLD"))],[],RankedMostChampions([]))))"#,
        )
        .parse()
        .expect("profile compact fixture");
        let profile = parse_profile(&player, &compact).expect("profile fixture");
        let parsed_match = |match_id: &str, created_at: &str| {
            parse_action_match(
                &json!({
                    "id": match_id,
                    "created_at": created_at,
                    "game_length": 1800,
                    "game_type": {"game_type": "SOLORANKED"},
                    "team_blue": [{
                        "champion_id": 103, "position": "MID", "team_key": "BLUE",
                        "summoner": {"puuid": "contract-puuid"},
                        "stats": {"kill": 5, "death": 2, "assist": 7, "result": "WIN", "op_score": 7.1}
                    }],
                    "team_red": []
                }),
                "contract-puuid",
            )
            .expect("match action fixture")
        };
        overlay_provider::PlayerProviderContractFixture {
            profile,
            pages: vec![
                MatchPage {
                    source: "opgg".into(),
                    matches: vec![parsed_match("OPGG_CONTRACT_2", "2026-07-14T00:00:00+09:00")],
                    next_cursor: Some("2026-07-14T00:00:00+09:00".into()),
                    partial_failures: vec![MatchFailure {
                        match_id: "OPGG_CONTRACT_PARTIAL".into(),
                        message: "fixture action parse failure".into(),
                        retryable: true,
                    }],
                    fetched_at: 2,
                },
                MatchPage {
                    source: "opgg".into(),
                    matches: vec![parsed_match("OPGG_CONTRACT_1", "2026-07-13T00:00:00+09:00")],
                    next_cursor: None,
                    partial_failures: vec![],
                    fetched_at: 3,
                },
            ],
            champions: vec![PlayerChampionStats {
                source: "opgg".into(),
                champion_id: 103,
                games: 10,
                wins: 6,
                losses: 4,
                win_rate: 0.6,
                kda: None,
                cs_per_minute: None,
                role: None,
                queue: "RANKED".into(),
                extras: ProviderExtras::Opgg(json!({})),
            }],
            refresh: RefreshResult {
                source: "opgg".into(),
                cache_invalidated: true,
                mutation_performed: false,
                refreshed_at: 4,
            },
            capabilities: ProviderCapabilities {
                player_profile: true,
                match_history: true,
                champion_stats: true,
                direct_api: false,
                regions: vec!["KR".into(), "JP1".into()],
                ..ProviderCapabilities::default()
            },
        }
    }

    overlay_provider::player_provider_contract_suite!(
        opgg_player_contract,
        "opgg",
        contract_fixture()
    );

    #[test]
    fn compact_parser_handles_nested_calls_arrays_escapes_and_nulls() {
        let parsed = CompactParser::new(
            r#"Root(Data([Row(1,"Hide on bush",true,null),Row(-2.5,"\u97e9\u56fd",false,[])]))"#,
        )
        .parse()
        .unwrap();
        let data = args(&parsed, "Root").unwrap();
        assert_eq!(args(&data[0], "Data").unwrap().len(), 1);
    }

    #[test]
    fn profile_fixture_preserves_missing_flex_values_and_previous_tiers() {
        let root = CompactParser::new(
            r#"LolGetSummonerProfile(Data(Summoner("Hide on bush","KR1",920,"https://x/profileIcon6.jpg","p","2026-07-13T00:00:00+09:00",LadderRank(399,2750000),[LeagueStat("SOLORANKED",298,249,null,TierInfo(1,1697,"GRANDMASTER")),LeagueStat("FLEXRANKED",null,null,null,TierInfo(null,null,null))],[PreviousSeasonTier(31,[RankEntrie("SOLORANKED",RankInfo(1,285,"MASTER",null,null)),RankEntrie1("FLEXRANKED")])],RankedMostChampions([MyChampionStat("Aurora",54,31,23)]))))"#,
        )
        .parse()
        .unwrap();
        let profile = parse_profile(
            &PlayerRef {
                platform_id: "KR".into(),
                game_name: "Hide on bush".into(),
                tag_line: "KR1".into(),
            },
            &root,
        )
        .unwrap();
        assert_eq!(profile.profile_icon_id, Some(6));
        assert_eq!(profile.ladder_percentile, Some(399.0 / 2_750_000.0 * 100.0));
        assert_eq!(profile.ranks.len(), 2);
        assert_eq!(profile.ranks[1].tier, None);
        assert_eq!(profile.previous_seasons.len(), 2);
        let values = summoner_node(&root).unwrap();
        let champions = args(&values[9], "RankedMostChampions").unwrap();
        assert_eq!(array(&champions[0]).unwrap().len(), 1);
    }

    #[test]
    fn action_fixture_extracts_all_participants_and_op_score_extra() {
        let root = json!({
            "id": "game", "created_at": "2026-07-11T23:55:58+09:00",
            "game_length": 2064, "game_type": {"game_type": "SOLORANKED"},
            "stats": {"op_score": 5.39, "cs": {"totalCs": 358}},
            "spells": [{"id": 12}, {"id": 4}], "runes": [{"id": 8021}, {"id": 8400}],
            "team_blue": [{
                "champion_id": 777, "position": "MID", "team_key": "BLUE",
                "items": [6673, 3153], "summoner": {"game_name": "Hide on bush", "tagline": "KR1", "puuid": "mine"},
                "stats": {"kill": 3, "death": 2, "assist": 11, "result": "WIN", "op_score": 5.39}
            }],
            "team_red": [{
                "champion_id": 2, "position": "TOP", "team_key": "RED",
                "items": [3073], "summoner": {"game_name": "Other", "tagline": "KR1", "puuid": "other"},
                "stats": {"kill": 7, "death": 6, "assist": 6, "result": "LOSE", "op_score": 4.45}
            }]
        });
        let parsed = parse_action_match(&root, "mine").unwrap();
        assert_eq!(parsed.queue_id, 420);
        assert_eq!(parsed.participants.len(), 2);
        assert_eq!(parsed.champion_id, 777);
        assert_eq!(parsed.cs, Some(358));
        assert_eq!(parsed.spell_ids, vec![12, 4]);
        assert!(parsed.win);
        assert!(matches!(parsed.extras, ProviderExtras::Opgg(_)));
    }

    #[test]
    fn server_action_and_flight_value_parsers_reject_malformed_contracts() {
        let bundle = r#"let c=(0,d.createServerReference)("409a2b9ca50d15e50a4dace93552e3a40113dc2753",d.callServer,void 0,d.findSourceMapURL,"getGames");"#;
        assert_eq!(
            games_action_id(bundle).as_deref(),
            Some("409a2b9ca50d15e50a4dace93552e3a40113dc2753")
        );
        let parsed = action_json("0:{\"a\":\"$@1\"}\n1:{\"data\":[],\"meta\":{}}")
            .expect("Flight action value");
        assert!(parsed["data"].as_array().unwrap().is_empty());
        assert!(action_json("0:{}").is_err());
    }

    #[test]
    fn public_profile_url_encodes_riot_id_and_queue_mapping_keeps_special_modes() {
        let url = public_profile_url(&PlayerRef {
            platform_id: "JP1".into(),
            game_name: "space/slash".into(),
            tag_line: "JP 1".into(),
        })
        .unwrap();
        assert_eq!(
            url.as_str(),
            "https://op.gg/lol/summoners/jp/space%2Fslash-JP%201"
        );
        assert_eq!(action_queue(Some(1700)), "ARENA");
        assert_eq!(queue_id("ARENA"), 1700);
        assert_eq!(queue_id("ARAM"), 450);
        assert_eq!(platform("OC1").unwrap(), "OCE");
        assert_eq!(platform("LA1").unwrap(), "LAN");
        assert_eq!(platform("LA2").unwrap(), "LAS");
        assert_eq!(platform("EUN1").unwrap(), "EUNE");
        assert_eq!(platform("PH2").unwrap(), "PH");
        assert!(platform("PBE1").is_err());

        let provider =
            OpggProvider::new(std::sync::Arc::new(overlay_ddragon::DdragonClient::new())).unwrap();
        assert!(!provider.capabilities().direct_api);
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
                OpggProvider::new(std::sync::Arc::new(overlay_ddragon::DdragonClient::new()))
                    .expect("provider");
            let player = PlayerRef {
                platform_id: "KR".into(),
                game_name: "Hide on bush".into(),
                tag_line: "KR1".into(),
            };
            let profile = provider.profile(&player, true).await.expect("profile");
            assert!(profile.identity.puuid.is_some());
            assert!(!profile.ranks.is_empty());
            let champions = provider
                .champion_stats(&player, None, None, None, true)
                .await
                .expect("champion stats");
            assert!(!champions.is_empty());
            let matches = provider
                .recent_matches(&player, None, None, true)
                .await
                .expect("matches");
            assert_eq!(
                matches.matches.len() + matches.partial_failures.len(),
                PAGE_SIZE
            );
            assert!(matches
                .matches
                .iter()
                .all(|entry| !entry.participants.is_empty()));
            assert!(matches
                .matches
                .iter()
                .filter(|entry| matches!(entry.queue_id, 400 | 420 | 440 | 450))
                .all(|entry| entry.participants.len() == 10));
            let cursor = matches.next_cursor.as_deref().expect("second-page cursor");
            let second = provider
                .recent_matches(&player, Some(cursor), None, true)
                .await
                .expect("second match page");
            assert_eq!(
                second.matches.len() + second.partial_failures.len(),
                PAGE_SIZE
            );
            println!(
                "OPGG PLAYER LIVE OK: profile={} ranks={} first={} second={} champions={}",
                profile.identity.game_name,
                profile.ranks.len(),
                matches.matches.len(),
                second.matches.len(),
                champions.len()
            );
        });
    }
}
