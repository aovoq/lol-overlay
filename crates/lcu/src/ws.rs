use irelia::ws::types::{Event, EventKind};
use irelia::ws::{LcuWebSocket, Subscriber};
use serde_json::Value;
use tokio::sync::mpsc::UnboundedSender;

use crate::error::LcuError;

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
/// The underlying socket handle is intentionally leaked so the subscription
/// lives for the lifetime of the process (mirrors previous mem::forget in lib.rs).
pub fn subscribe_champ_select(tx: UnboundedSender<Value>) -> Result<(), LcuError> {
    let mut ws = LcuWebSocket::new();
    let _ = ws.subscribe(
        EventKind::json_api_event_callback("/lol-champ-select/v1/session"),
        SessionForwarder { tx },
    );
    std::mem::forget(ws);
    Ok(())
}
