#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("not enough data")]
    NotEnoughData,
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("player-http:404")]
    PlayerNotFound,
    #[error("player-http:422 {0}")]
    InvalidPlayerRequest(String),
    #[error("player-http:429 retry-after={retry_after:?}")]
    RateLimited { retry_after: Option<u64> },
    #[error("player-timeout")]
    Timeout,
    #[error("invalid provider data: {0}")]
    InvalidData(String),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ProviderError>;
