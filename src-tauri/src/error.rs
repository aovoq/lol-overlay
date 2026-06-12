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
