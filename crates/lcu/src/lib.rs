//! LCU (League Client) access via the [`irelia`] crate.
//!
//! `irelia` handles lockfile discovery, auth, and the self-signed cert for us,
//! so this crate is just thin helpers over its REST client plus a WebSocket
//! subscriber that pushes champ-select updates onto a channel for event-driven
//! rune import (no polling needed for the pick itself).
//!
//! Writing runes uses `/lol-perks/*`, which is on Riot's approved LCU endpoint
//! list but still requires registering the app with Riot before public release.

mod error;
mod parse;
mod rest;
mod ws;

pub use error::LcuError;
pub use overlay_types::{
    ChampSelectEvent, MatchmakingInfo, MyPick, Phase, RecentGame, RunePagePayload, SummonerInfo,
};

pub use parse::{parse_champ_select, parse_my_pick};
pub use rest::{
    accept_ready_check, apply_runes, apply_spells, decline_ready_check, fetch_matchmaking,
    fetch_phase, fetch_platform_id, fetch_recent_matches, fetch_session, fetch_summoner,
};
pub use ws::{subscribe_champ_select, subscribe_matchmaking, LcuSubscription};
