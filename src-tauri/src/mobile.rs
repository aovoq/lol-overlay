//! Ephemeral mobile relay sessions and non-blocking snapshot publishing.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::events::{log, RecommendationsEvent};
use overlay_types::GameSnapshot;

const PROTOCOL_VERSION: u8 = 1;
const HTTP_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateSessionResponse {
    session_id: String,
    producer_token: String,
    viewer_url: String,
    expires_at: u64,
}

#[derive(Debug, Clone)]
struct RelaySession {
    generation: u64,
    snapshot_url: String,
    producer_token: String,
    public: MobilePairingState,
    sequence: Arc<AtomicU64>,
    upload_inflight: Arc<AtomicBool>,
    upload_failed: Arc<AtomicBool>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MobilePairingStatus {
    Disconnected,
    Paired,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MobilePairingState {
    pub status: MobilePairingStatus,
    pub session_id: String,
    pub viewer_url: String,
    pub expires_at: u64,
    pub message: String,
}

impl MobilePairingState {
    fn disconnected() -> Self {
        Self {
            status: MobilePairingStatus::Disconnected,
            session_id: String::new(),
            viewer_url: String::new(),
            expires_at: 0,
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MobileSnapshot {
    protocol_version: u8,
    sequence: u64,
    captured_at: u64,
    phase: String,
    client_up: bool,
    game: Option<MobileGame>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MobileGame {
    game_mode: String,
    game_time: f64,
    self_champion: String,
    self_raw_name: String,
    self_position: String,
    allies: Vec<String>,
    enemies: Vec<overlay_types::EnemyChampion>,
    threats: overlay_types::ThreatProfile,
    skill_order: Option<overlay_types::SkillOrder>,
    items: Vec<overlay_types::ItemRecommendation>,
}

struct MobileRelayInner {
    http: reqwest::Client,
    session: Mutex<Option<RelaySession>>,
    generation: AtomicU64,
}

#[derive(Clone)]
pub struct MobileRelay {
    inner: Arc<MobileRelayInner>,
}

impl MobileRelay {
    pub fn new() -> crate::error::Result<Self> {
        Ok(Self {
            inner: Arc::new(MobileRelayInner {
                http: reqwest::Client::builder().timeout(HTTP_TIMEOUT).build()?,
                session: Mutex::new(None),
                generation: AtomicU64::new(0),
            }),
        })
    }

    pub fn state(&self) -> MobilePairingState {
        self.inner
            .session
            .lock()
            .as_ref()
            .map_or_else(MobilePairingState::disconnected, |session| {
                session.public.clone()
            })
    }

    pub async fn start(
        &self,
        app: &AppHandle,
        relay_url: &str,
    ) -> crate::error::Result<MobilePairingState> {
        let relay_url = relay_url.trim().trim_end_matches('/');
        let parsed = reqwest::Url::parse(relay_url)
            .map_err(|e| crate::error::Error::Other(format!("invalid relay URL: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(crate::error::Error::Other(
                "relay URL must use http or https".into(),
            ));
        }

        let response = self
            .inner
            .http
            .post(format!("{relay_url}/v1/sessions"))
            .send()
            .await?
            .error_for_status()?
            .json::<CreateSessionResponse>()
            .await?;

        let public = MobilePairingState {
            status: MobilePairingStatus::Paired,
            session_id: response.session_id.clone(),
            viewer_url: response.viewer_url,
            expires_at: response.expires_at,
            message: "iPhoneの接続を待っています".into(),
        };
        let generation = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        *self.inner.session.lock() = Some(RelaySession {
            generation,
            snapshot_url: format!("{relay_url}/v1/sessions/{}/snapshot", response.session_id),
            producer_token: response.producer_token,
            public: public.clone(),
            sequence: Arc::new(AtomicU64::new(0)),
            upload_inflight: Arc::new(AtomicBool::new(false)),
            upload_failed: Arc::new(AtomicBool::new(false)),
        });
        let _ = app.emit("mobile-pairing", public.clone());
        Ok(public)
    }

    pub fn stop(&self, app: &AppHandle) -> MobilePairingState {
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        *self.inner.session.lock() = None;
        let state = MobilePairingState::disconnected();
        let _ = app.emit("mobile-pairing", state.clone());
        state
    }

    pub fn publish_idle(&self, app: &AppHandle, phase: &str, client_up: bool) {
        self.publish(app, phase, client_up, None);
    }

    pub fn publish_game(
        &self,
        app: &AppHandle,
        phase: &str,
        client_up: bool,
        snapshot: &GameSnapshot,
        recommendations: &RecommendationsEvent,
    ) {
        self.publish(
            app,
            phase,
            client_up,
            Some(MobileGame {
                game_mode: snapshot.game_mode.clone(),
                game_time: snapshot.game_time,
                self_champion: snapshot.self_champion.clone(),
                self_raw_name: snapshot.self_raw_name.clone(),
                self_position: snapshot.self_position.clone(),
                allies: snapshot.allies.clone(),
                enemies: snapshot.enemies.clone(),
                threats: recommendations.threats,
                skill_order: recommendations.skill_order.clone(),
                items: recommendations.items.clone(),
            }),
        );
    }

    fn publish(&self, app: &AppHandle, phase: &str, client_up: bool, game: Option<MobileGame>) {
        let Some(session) = self.inner.session.lock().clone() else {
            return;
        };
        if session.upload_inflight.swap(true, Ordering::AcqRel) {
            return;
        }

        let payload = MobileSnapshot {
            protocol_version: PROTOCOL_VERSION,
            sequence: session.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            captured_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            phase: phase.to_string(),
            client_up,
            game,
        };
        let relay = self.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let result = relay
                .inner
                .http
                .post(&session.snapshot_url)
                .bearer_auth(&session.producer_token)
                .json(&payload)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status);

            session.upload_inflight.store(false, Ordering::Release);
            match result {
                Ok(_) => {
                    if session.upload_failed.swap(false, Ordering::Relaxed) {
                        relay.emit_current(
                            &app,
                            session.generation,
                            MobilePairingStatus::Paired,
                            "Relayへ再接続しました",
                        );
                    }
                }
                Err(error) => {
                    if !session.upload_failed.swap(true, Ordering::Relaxed) {
                        log(&app, "warn", format!("mobile relay upload failed: {error}"));
                        relay.emit_current(
                            &app,
                            session.generation,
                            MobilePairingStatus::Error,
                            "Relayへ送信できません",
                        );
                    }
                }
            }
        });
    }

    fn emit_current(
        &self,
        app: &AppHandle,
        generation: u64,
        status: MobilePairingStatus,
        message: &str,
    ) {
        let mut active = self.inner.session.lock();
        let Some(session) = active
            .as_mut()
            .filter(|session| session.generation == generation)
        else {
            return;
        };
        session.public.status = status;
        session.public.message = message.into();
        let _ = app.emit("mobile-pairing", session.public.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairing_status_serializes_to_the_frontend_contract() {
        assert_eq!(
            serde_json::to_value(MobilePairingState::disconnected()).unwrap(),
            serde_json::json!({
                "status": "disconnected",
                "sessionId": "",
                "viewerUrl": "",
                "expiresAt": 0,
                "message": "",
            })
        );
    }

    #[test]
    fn idle_snapshot_serializes_to_the_protocol_contract() {
        let snapshot = MobileSnapshot {
            protocol_version: PROTOCOL_VERSION,
            sequence: 1,
            captured_at: 2,
            phase: "None".into(),
            client_up: false,
            game: None,
        };
        assert_eq!(
            serde_json::to_value(snapshot).unwrap(),
            serde_json::json!({
                "protocolVersion": 1,
                "sequence": 1,
                "capturedAt": 2,
                "phase": "None",
                "clientUp": false,
                "game": null,
            })
        );
    }
}
