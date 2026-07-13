use irelia::ws::types::{Event, EventKind};
use irelia::ws::{LcuWebSocket, Subscriber};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::LcuError;

const SUBSCRIBE_STARTUP_GRACE: Duration = Duration::from_millis(250);

pub struct LcuSubscription {
    ws: Option<LcuWebSocket>,
}

impl LcuSubscription {
    pub fn is_finished(&self) -> bool {
        match &self.ws {
            Some(ws) => ws.is_finished(),
            None => true,
        }
    }
}

impl Drop for LcuSubscription {
    fn drop(&mut self) {
        if let Some(ws) = self.ws.take() {
            let _ = ws.abort();
        }
    }
}

/// WebSocket subscriber that forwards champ-select session payloads onto a channel.
struct SessionForwarder {
    tx: UnboundedSender<Value>,
}

/// WebSocket subscriber that signals when matchmaking or ready-check state changes.
struct MatchmakingForwarder {
    tx: UnboundedSender<()>,
}

impl Subscriber for MatchmakingForwarder {
    fn on_event(&mut self, _event: &Event, _continues: &mut bool) {
        let _ = self.tx.send(());
    }
}

impl Subscriber for SessionForwarder {
    fn on_event(&mut self, event: &Event, _continues: &mut bool) {
        // Event(RequestType, EventKind, EventData{ data, event_type, uri }).
        let _ = self.tx.send(event.2.data.clone());
    }
}

/// Subscribes to champ-select session events and forwards payloads onto `tx`.
pub async fn subscribe_champ_select(
    tx: UnboundedSender<Value>,
) -> Result<LcuSubscription, LcuError> {
    let mut ws = LcuWebSocket::new();
    ws.subscribe(
        EventKind::json_api_event_callback("/lol-champ-select/v1/session"),
        SessionForwarder { tx },
    )
    .ok_or_else(|| LcuError::Unavailable("champ-select websocket subscription closed".into()))?;

    // irelia connects on a background thread. If the LCU process is absent,
    // the channel send can win the race just before that thread exits.
    tokio::time::sleep(SUBSCRIBE_STARTUP_GRACE).await;
    if ws.is_finished() {
        return Err(LcuError::Unavailable(
            "champ-select websocket thread exited before subscribing".into(),
        ));
    }

    Ok(LcuSubscription { ws: Some(ws) })
}

/// Subscribes to queue and ready-check changes. The receiver should fetch the
/// combined REST representation after each signal because the two endpoints
/// make up one [`MatchmakingInfo`](overlay_types::MatchmakingInfo).
pub async fn subscribe_matchmaking(tx: UnboundedSender<()>) -> Result<LcuSubscription, LcuError> {
    let mut ws = LcuWebSocket::new();
    for endpoint in [
        "/lol-matchmaking/v1/search",
        "/lol-matchmaking/v1/ready-check",
    ] {
        ws.subscribe(
            EventKind::json_api_event_callback(endpoint),
            MatchmakingForwarder { tx: tx.clone() },
        )
        .ok_or_else(|| LcuError::Unavailable("matchmaking websocket subscription closed".into()))?;
    }

    tokio::time::sleep(SUBSCRIBE_STARTUP_GRACE).await;
    if ws.is_finished() {
        return Err(LcuError::Unavailable(
            "matchmaking websocket thread exited before subscribing".into(),
        ));
    }

    Ok(LcuSubscription { ws: Some(ws) })
}
