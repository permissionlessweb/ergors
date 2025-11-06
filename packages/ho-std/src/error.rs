//! Error handling for CW-HO system

use thiserror::Error;

pub type HoResult<T> = std::result::Result<T, HoError>;

#[derive(Error, Debug)]
pub enum HoError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Orchestration error: {0}")]
    Orchestration(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("DeSerialization error: {0}")]
    DeSerialization(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),
}

impl From<String> for HoError {
    fn from(s: String) -> Self {
        HoError::Other(s)
    }
}

impl From<&str> for HoError {
    fn from(s: &str) -> Self {
        HoError::Other(s.to_string())
    }
}
