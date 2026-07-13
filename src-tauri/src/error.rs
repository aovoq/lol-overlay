//! Crate-wide error type.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// The data source has too few games to make a reliable recommendation.
    /// Crosses the command boundary as the literal string "not-enough-data",
    /// which the frontend branches on (any other string = generic failure).
    #[error("not-enough-data")]
    NotEnoughData,

    #[error("{0}")]
    Other(String),
}

impl From<overlay_lcu::LcuError> for Error {
    fn from(e: overlay_lcu::LcuError) -> Self {
        Error::Other(e.to_string())
    }
}

impl From<overlay_provider::ProviderError> for Error {
    fn from(e: overlay_provider::ProviderError) -> Self {
        match e {
            overlay_provider::ProviderError::NotEnoughData => Error::NotEnoughData,
            overlay_provider::ProviderError::Http(e) => Error::Http(e),
            overlay_provider::ProviderError::Json(e) => Error::Json(e),
            other @ (overlay_provider::ProviderError::PlayerNotFound
            | overlay_provider::ProviderError::InvalidPlayerRequest(_)
            | overlay_provider::ProviderError::RateLimited { .. }
            | overlay_provider::ProviderError::Timeout
            | overlay_provider::ProviderError::InvalidData(_)) => Error::Other(other.to_string()),
            overlay_provider::ProviderError::Other(s) => Error::Other(s),
        }
    }
}

impl From<overlay_live_client::LiveClientError> for Error {
    fn from(e: overlay_live_client::LiveClientError) -> Self {
        match e {
            overlay_live_client::LiveClientError::Http(err) => Error::Http(err),
            overlay_live_client::LiveClientError::Json(err) => Error::Json(err),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerCommandError {
    kind: &'static str,
    message: String,
    retry_after: Option<u64>,
}

impl From<overlay_provider::ProviderError> for PlayerCommandError {
    fn from(error: overlay_provider::ProviderError) -> Self {
        let (kind, retry_after) = match &error {
            overlay_provider::ProviderError::PlayerNotFound => ("notFound", None),
            overlay_provider::ProviderError::InvalidPlayerRequest(_) => ("validation", None),
            overlay_provider::ProviderError::RateLimited { retry_after } => {
                ("rateLimited", *retry_after)
            }
            overlay_provider::ProviderError::Timeout => ("timeout", None),
            overlay_provider::ProviderError::InvalidData(_) => ("invalidData", None),
            _ => ("unknown", None),
        };
        Self {
            kind,
            message: error.to_string(),
            retry_after,
        }
    }
}

impl From<Error> for PlayerCommandError {
    fn from(error: Error) -> Self {
        Self {
            kind: "unknown",
            message: error.to_string(),
            retry_after: None,
        }
    }
}

pub type PlayerResult<T> = std::result::Result<T, PlayerCommandError>;

// Errors must be serializable to cross the Tauri command boundary.
impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn player_errors_serialize_as_typed_camel_case_payloads() {
        let error = PlayerCommandError::from(overlay_provider::ProviderError::RateLimited {
            retry_after: Some(17),
        });
        assert_eq!(
            serde_json::to_value(error).unwrap(),
            serde_json::json!({
                "kind": "rateLimited",
                "message": "player-http:429 retry-after=Some(17)",
                "retryAfter": 17
            })
        );
    }
}
