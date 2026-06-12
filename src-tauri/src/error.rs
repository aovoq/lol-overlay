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

// Errors must be serializable to cross the Tauri command boundary.
impl Serialize for Error {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
