//! Player statistics from OP.GG's official, read-only MCP JSON-RPC endpoint.
//!
//! The transport is JSON. OP.GG intentionally returns selected data in a
//! compact constructor notation inside the MCP text content, so this module
//! parses that notation with a small data parser instead of scraping HTML.

use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use chrono::DateTime;
use futures::{stream, StreamExt};
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
const DETAIL_CONCURRENCY: usize = 5;

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

const MATCH_FIELDS: &[&str] = &[
    "data.game_history[].{id,created_at,game_length_second,game_type}",
    "data.game_history[].participants[].{champion_id,champion_name,position,team_key,items[],spells[]}",
    "data.game_history[].participants[].summoner.{game_name,tagline,puuid}",
    "data.game_history[].participants[].stats.{kill,death,assist,minion_kill,neutral_minion_kill,result,op_score}",
];

const DETAIL_FIELDS: &[&str] = &[
    "data.game_detail.{id,created_at,game_length_second,game_type}",
    "data.game_detail.teams[].participants[].{champion_id,champion_name,position,team_key,items[],spells[]}",
    "data.game_detail.teams[].participants[].summoner.{game_name,tagline,puuid}",
    "data.game_detail.teams[].participants[].stats.{kill,death,assist,minion_kill,neutral_minion_kill,result,op_score}",
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

fn decimal(value: Option<&CompactValue>) -> Option<f64> {
    match value? {
        CompactValue::Number(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn rfc3339_millis(value: Option<&CompactValue>) -> i64 {
    text(value)
        .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
        .map(|value| value.timestamp_millis())
        .unwrap_or_default()
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

fn platform(raw: &str) -> String {
    raw.trim_end_matches('1').to_ascii_uppercase()
}

impl OpggProvider {
    async fn mcp_call(&self, tool: &str, arguments: Value) -> Result<CompactValue> {
        let response = self
            .api
            .http()
            .post(MCP_URL)
            .header("accept", "application/json, text/event-stream")
            .json(&json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": tool, "arguments": arguments}
            }))
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    ProviderError::Timeout
                } else {
                    ProviderError::Http(error)
                }
            })?;
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

    async fn profile_compact(&self, player: &PlayerRef) -> Result<CompactValue> {
        self.mcp_call(
            "lol_get_summoner_profile",
            json!({
                "game_name": player.game_name, "tag_line": player.tag_line,
                "region": platform(&player.platform_id), "lang": "en_US",
                "desired_output_fields": PROFILE_FIELDS
            }),
        )
        .await
    }

    async fn detail_compact(
        &self,
        region: &str,
        id: &str,
        created_at: &str,
    ) -> Result<CompactValue> {
        self.mcp_call(
            "lol_get_summoner_game_detail",
            json!({
                "region": platform(region), "lang": "en_US", "game_id": id,
                "created_at": created_at, "desired_output_fields": DETAIL_FIELDS
            }),
        )
        .await
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
            (total > 0.0).then_some(rank / total)
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

fn participant(fields: &[CompactValue]) -> Result<MatchParticipant> {
    let summoner = fields.get(6).and_then(|value| args(value, "Summoner").ok());
    let stats = fields
        .get(7)
        .and_then(|value| args(value, "Stats").ok())
        .ok_or_else(|| ProviderError::Other("OP.GG participant omitted stats".into()))?;
    let team = text(fields.get(3));
    Ok(MatchParticipant {
        puuid: summoner.and_then(|fields| text(fields.get(2))),
        game_name: summoner.and_then(|fields| text(fields.first())),
        tag_line: summoner.and_then(|fields| text(fields.get(1))),
        champion_id: integer(fields.first()).unwrap_or_default(),
        team_id: match team.as_deref() {
            Some("BLUE") => 100,
            Some("RED") => 200,
            _ => 0,
        },
        role: text(fields.get(2)),
        win: text(stats.get(5)).as_deref() == Some("WIN"),
        kills: integer(stats.first()).unwrap_or_default(),
        deaths: integer(stats.get(1)).unwrap_or_default(),
        assists: integer(stats.get(2)).unwrap_or_default(),
        items: fields
            .get(4)
            .and_then(|value| array(value).ok())
            .into_iter()
            .flatten()
            .filter_map(|value| integer(Some(value)))
            .collect(),
        extras: ProviderExtras::Opgg(json!({"opScore": decimal(stats.get(6))})),
    })
}

fn game_fields(root: &CompactValue, detail: bool) -> Result<&[CompactValue]> {
    let root_name = if detail {
        "LolGetSummonerGameDetail"
    } else {
        "LolListSummonerMatches"
    };
    let data = args(root, root_name)?
        .first()
        .ok_or_else(|| ProviderError::Other("OP.GG match response omitted data".into()))?;
    let value = args(data, "Data")?
        .first()
        .ok_or_else(|| ProviderError::Other("OP.GG match response omitted game".into()))?;
    args(value, if detail { "GameDetail" } else { "GameHistory" })
}

fn parse_detail(root: &CompactValue, puuid: &str) -> Result<PlayerMatch> {
    let fields = game_fields(root, true)?;
    let mut participants = vec![];
    if let Some(CompactValue::Array(teams)) = fields.get(4) {
        for team in teams {
            let entries = args(team, "Team")?
                .first()
                .ok_or_else(|| ProviderError::Other("OP.GG team omitted participants".into()))?;
            for entry in array(entries)? {
                participants.push(participant(args(entry, "Participant")?)?);
            }
        }
    }
    let mine = participants
        .iter()
        .find(|entry| entry.puuid.as_deref() == Some(puuid))
        .or_else(|| participants.first())
        .ok_or_else(|| ProviderError::Other("OP.GG detail omitted participants".into()))?
        .clone();
    let game_type = text(fields.get(3)).unwrap_or_default();
    Ok(PlayerMatch {
        match_id: text(fields.first()).unwrap_or_default(),
        started_at: rfc3339_millis(fields.get(1)),
        duration_seconds: integer(fields.get(2)).unwrap_or_default(),
        queue_id: queue_id(&game_type),
        remake: integer(fields.get(2)).is_some_and(|value| value < 300),
        champion_id: mine.champion_id,
        role: mine.role.clone(),
        win: mine.win,
        kills: mine.kills,
        deaths: mine.deaths,
        assists: mine.assists,
        cs: None,
        items: mine.items.clone(),
        spell_ids: vec![],
        perk_ids: vec![],
        participants,
        extras: mine.extras,
    })
}

#[async_trait]
impl PlayerStatsProvider for OpggProvider {
    async fn profile(&self, player: &PlayerRef, _force: bool) -> Result<PlayerProfile> {
        parse_profile(player, &self.profile_compact(player).await?)
    }

    async fn recent_matches(
        &self,
        player: &PlayerRef,
        cursor: Option<&str>,
        queue: Option<i64>,
        _force: bool,
    ) -> Result<MatchPage> {
        if cursor.is_some() {
            return Err(ProviderError::Other(
                "OP.GG official MCP match API does not expose pagination".into(),
            ));
        }
        let profile = self.profile(player, false).await?;
        let puuid = profile
            .identity
            .puuid
            .ok_or_else(|| ProviderError::Other("OP.GG profile omitted puuid".into()))?;
        let root = self
            .mcp_call(
                "lol_list_summoner_matches",
                json!({
                    "game_name": player.game_name, "tag_line": player.tag_line,
                    "region": platform(&player.platform_id), "lang": "en_US",
                    "limit": PAGE_SIZE, "desired_output_fields": MATCH_FIELDS
                }),
            )
            .await?;
        let data = args(&root, "LolListSummonerMatches")?
            .first()
            .ok_or_else(|| ProviderError::Other("OP.GG matches omitted data".into()))?;
        let games = args(data, "Data")?
            .first()
            .and_then(|value| array(value).ok())
            .ok_or_else(|| ProviderError::Other("OP.GG matches omitted history".into()))?;
        let summaries = games
            .iter()
            .filter_map(|game| {
                let fields = args(game, "GameHistory").ok()?;
                Some((text(fields.first())?, text(fields.get(1))?))
            })
            .collect::<Vec<_>>();
        let hydrated = stream::iter(summaries.into_iter().map(|(id, created)| {
            let puuid = puuid.clone();
            async move {
                let result = self
                    .detail_compact(&player.platform_id, &id, &created)
                    .await
                    .and_then(|root| parse_detail(&root, &puuid));
                (id, result)
            }
        }))
        .buffer_unordered(DETAIL_CONCURRENCY)
        .collect::<Vec<_>>()
        .await;
        let mut matches = vec![];
        let mut partial_failures = vec![];
        for (id, result) in hydrated {
            match result {
                Ok(value) if queue.is_none_or(|queue| value.queue_id == queue) => {
                    matches.push(value);
                }
                Ok(_) => {}
                Err(error) => partial_failures.push(MatchFailure {
                    match_id: id,
                    message: error.to_string(),
                    retryable: true,
                }),
            }
        }
        matches.sort_by_key(|entry| std::cmp::Reverse(entry.started_at));
        Ok(MatchPage {
            source: "opgg".into(),
            matches,
            next_cursor: None,
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
        _force: bool,
    ) -> Result<Vec<PlayerChampionStats>> {
        let root = self.profile_compact(player).await?;
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
            let games = integer(fields.get(1)).unwrap_or_default();
            let wins = integer(fields.get(2)).unwrap_or_default();
            let losses = integer(fields.get(3)).unwrap_or(games - wins);
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

    async fn refresh(&self, _player: &PlayerRef) -> Result<RefreshResult> {
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
        assert_eq!(profile.ranks.len(), 2);
        assert_eq!(profile.ranks[1].tier, None);
        assert_eq!(profile.previous_seasons.len(), 2);
        let values = summoner_node(&root).unwrap();
        let champions = args(&values[9], "RankedMostChampions").unwrap();
        assert_eq!(array(&champions[0]).unwrap().len(), 1);
    }

    #[test]
    fn detail_fixture_extracts_all_participants_and_op_score_extra() {
        let root = CompactParser::new(
            r#"LolGetSummonerGameDetail(Data(GameDetail("game","2026-07-11T23:55:58+09:00",2064,"SOLORANKED",[Team([Participant(777,"Yone","MID","BLUE",[6673,3153],[12,4],Summoner("Hide on bush","KR1","mine"),Stats(3,2,11,323,35,"WIN",5.39))]),Team([Participant(2,"Olaf","TOP","RED",[3073],[3,4],Summoner("Other","KR1","other"),Stats(7,6,6,288,4,"LOSE",4.45))])])))"#,
        )
        .parse()
        .unwrap();
        let parsed = parse_detail(&root, "mine").unwrap();
        assert_eq!(parsed.queue_id, 420);
        assert_eq!(parsed.participants.len(), 2);
        assert_eq!(parsed.champion_id, 777);
        assert!(parsed.win);
        assert!(matches!(parsed.extras, ProviderExtras::Opgg(_)));
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
            println!(
                "OPGG PLAYER LIVE OK: profile={} ranks={} matches={} champions={}",
                profile.identity.game_name,
                profile.ranks.len(),
                matches.matches.len(),
                champions.len()
            );
        });
    }
}
