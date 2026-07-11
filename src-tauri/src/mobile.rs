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
    pairing_code: String,
    pairing_code_expires_at: u64,
    expires_at: u64,
}

#[derive(Debug, Clone)]
struct RelaySession {
    generation: u64,
    relay_base: String,
    session_id: String,
    snapshot_url: String,
    producer_token: String,
    public: MobilePairingState,
    sequence: Arc<AtomicU64>,
    upload_inflight: Arc<AtomicBool>,
    upload_failed: Arc<AtomicBool>,
    pending: Arc<Mutex<Option<MobileSnapshot>>>,
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
    pub pairing_code: String,
    pub pairing_code_expires_at: u64,
    pub expires_at: u64,
    pub message: String,
}

impl MobilePairingState {
    fn disconnected() -> Self {
        Self {
            status: MobilePairingStatus::Disconnected,
            session_id: String::new(),
            viewer_url: String::new(),
            pairing_code: String::new(),
            pairing_code_expires_at: 0,
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
    /// Serializes start/stop so concurrent invokes cannot clobber each other.
    ops: tokio::sync::Mutex<()>,
    generation: AtomicU64,
}

#[derive(Clone)]
pub struct MobileRelay {
    inner: Arc<MobileRelayInner>,
}

fn is_loopback_host(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

fn validate_relay_url(relay_url: &str) -> crate::error::Result<reqwest::Url> {
    let parsed = reqwest::Url::parse(relay_url)
        .map_err(|e| crate::error::Error::Other(format!("invalid relay URL: {e}")))?;
    match parsed.scheme() {
        "https" => Ok(parsed),
        "http" if parsed.host_str().is_some_and(is_loopback_host) => Ok(parsed),
        "http" => Err(crate::error::Error::Other(
            "http relay URLs are only allowed for localhost".into(),
        )),
        _ => Err(crate::error::Error::Other(
            "relay URL must use https (or http on localhost)".into(),
        )),
    }
}

fn create_secret() -> Option<String> {
    std::env::var("MOBILE_RELAY_CREATE_SECRET")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            option_env!("MOBILE_RELAY_CREATE_SECRET")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

impl MobileRelay {
    pub fn new() -> crate::error::Result<Self> {
        Ok(Self {
            inner: Arc::new(MobileRelayInner {
                http: reqwest::Client::builder().timeout(HTTP_TIMEOUT).build()?,
                session: Mutex::new(None),
                ops: tokio::sync::Mutex::new(()),
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
        let _parsed = validate_relay_url(relay_url)?;

        let _ops = self.inner.ops.lock().await;

        // Take the previous session under the ops lock so a concurrent stop
        // cannot observe / clear a session we are about to replace.
        let previous = self.inner.session.lock().take();
        if let Some(previous) = previous.as_ref() {
            self.revoke_session(previous).await;
        }

        let mut request = self.inner.http.post(format!("{relay_url}/v1/sessions"));
        if let Some(secret) = create_secret() {
            request = request.bearer_auth(secret);
        }
        let response = request
            .send()
            .await?
            .error_for_status()?
            .json::<CreateSessionResponse>()
            .await?;

        let public = MobilePairingState {
            status: MobilePairingStatus::Paired,
            session_id: response.session_id.clone(),
            viewer_url: response.viewer_url,
            pairing_code: response.pairing_code,
            pairing_code_expires_at: response.pairing_code_expires_at,
            expires_at: response.expires_at,
            message: "iPhoneの接続を待っています".into(),
        };
        let generation = self.inner.generation.fetch_add(1, Ordering::AcqRel) + 1;
        *self.inner.session.lock() = Some(RelaySession {
            generation,
            relay_base: relay_url.to_string(),
            session_id: response.session_id.clone(),
            snapshot_url: format!("{relay_url}/v1/sessions/{}/snapshot", response.session_id),
            producer_token: response.producer_token,
            public: public.clone(),
            sequence: Arc::new(AtomicU64::new(0)),
            upload_inflight: Arc::new(AtomicBool::new(false)),
            upload_failed: Arc::new(AtomicBool::new(false)),
            pending: Arc::new(Mutex::new(None)),
        });
        let _ = app.emit("mobile-pairing", public.clone());
        Ok(public)
    }

    pub async fn stop(&self, app: &AppHandle) -> MobilePairingState {
        let _ops = self.inner.ops.lock().await;
        let previous = self.inner.session.lock().take();
        self.inner.generation.fetch_add(1, Ordering::AcqRel);
        let state = MobilePairingState::disconnected();
        let _ = app.emit("mobile-pairing", state.clone());
        if let Some(previous) = previous.as_ref() {
            self.revoke_session(previous).await;
        }
        state
    }

    async fn revoke_session(&self, session: &RelaySession) {
        let url = format!("{}/v1/sessions/{}", session.relay_base, session.session_id);
        let _ = self
            .inner
            .http
            .delete(&url)
            .bearer_auth(&session.producer_token)
            .send()
            .await
            .and_then(reqwest::Response::error_for_status);
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
        *session.pending.lock() = Some(payload);

        if session.upload_inflight.swap(true, Ordering::AcqRel) {
            return;
        }

        let relay = self.clone();
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let next = {
                    let mut pending = session.pending.lock();
                    if let Some(payload) = pending.take() {
                        Some(payload)
                    } else {
                        session.upload_inflight.store(false, Ordering::Release);
                        None
                    }
                };
                let Some(payload) = next else {
                    // Queued after the empty check but before inflight cleared, or
                    // after clear — reclaim the uploader slot if needed.
                    if session.pending.lock().is_some()
                        && !session.upload_inflight.swap(true, Ordering::AcqRel)
                    {
                        continue;
                    }
                    break;
                };

                let result = relay
                    .inner
                    .http
                    .post(&session.snapshot_url)
                    .bearer_auth(&session.producer_token)
                    .json(&payload)
                    .send()
                    .await
                    .and_then(reqwest::Response::error_for_status);

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
                "pairingCode": "",
                "pairingCodeExpiresAt": 0,
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

    #[test]
    fn validate_relay_url_allows_https_and_localhost_http() {
        assert!(validate_relay_url("https://relay.example.com").is_ok());
        assert!(validate_relay_url("http://127.0.0.1:8787").is_ok());
        assert!(validate_relay_url("http://localhost:8787").is_ok());
        assert!(validate_relay_url("http://evil.example.com").is_err());
        assert!(validate_relay_url("ws://127.0.0.1:8787").is_err());
    }
}
