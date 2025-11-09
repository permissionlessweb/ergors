use axum::Json;
use ho_std::commonware::error::CommonwareNetworkError;
use ho_std::llm::HoError;
use reqwest::StatusCode;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, CwHoError>;

#[derive(Error, Debug)]
pub enum CwHoError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Storage error: {0}")]
    Storage(#[from] anyhow::Error),

    #[error("HoError error: {0}")]
    HoError(#[from] HoError),

    #[error("Storage error: {0}")]
    CommonwareNetworkError(#[from] CommonwareNetworkError),

    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("LLM provider error: {0}")]
    LlmEntity(String),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),
}

/// Helper function to create error JSON responses
pub fn error_json(message: &str, code: &str) -> serde_json::Value {
    serde_json::json!({
        "error": message,
        "code": code,
        "timestamp": chrono::Utc::now()
    })
}

/// Helper function to create API error responses
pub fn api_error(status: StatusCode, message: &str, code: &str) -> Json<serde_json::Value> {
    Json(error_json(message, code))
}

/// Helper function to create error responses
pub fn error_response(status: StatusCode, message: &str, code: &str) -> Json<serde_json::Value> {
    Json(error_json(message, code))
}
