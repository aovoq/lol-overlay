use irelia::ws::types::{Event, EventKind};
use irelia::ws::{LcuWebSocket, Subscriber};
use serde_json::Value;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::LcuError;

const SUBSCRIBE_STARTUP_GRACE: Duration = Duration::from_millis(250);

pub struct ChampSelectSubscription {
    ws: Option<LcuWebSocket>,
}

impl ChampSelectSubscription {
    pub fn is_finished(&self) -> bool {
        match &self.ws {
            Some(ws) => ws.is_finished(),
            None => true,
        }
    }
}

impl Drop for ChampSelectSubscription {
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

impl Subscriber for SessionForwarder {
    fn on_event(&mut self, event: &Event, _continues: &mut bool) {
        // Event(RequestType, EventKind, EventData{ data, event_type, uri }).
        let _ = self.tx.send(event.2.data.clone());
    }
}

/// Subscribes to champ-select session events and forwards payloads onto `tx`.
pub async fn subscribe_champ_select(
    tx: UnboundedSender<Value>,
) -> Result<ChampSelectSubscription, LcuError> {
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

    Ok(ChampSelectSubscription { ws: Some(ws) })
}
