#[derive(thiserror::Error, Debug)]
pub enum LcuError {
    #[error("LCU unavailable: {0}")]
    Unavailable(String),
    #[error("LCU error: {0}")]
    Other(String),
}
