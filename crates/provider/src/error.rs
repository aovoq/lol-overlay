#[derive(thiserror::Error, Debug)]
pub enum ProviderError {
    #[error("not enough data")]
    NotEnoughData,
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("parse error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ProviderError>;
