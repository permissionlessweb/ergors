//! Error handling for ERGORS system

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use thiserror::Error;

pub type HoResult<T> = std::result::Result<T, HoError>;

#[derive(Error, Debug)]
pub enum HoError {
    #[error("P2P error: {0}")]
    P2P(String), // We'll convert from commonware-p2p errors

    #[error("Broadcast error: {0}")]
    Broadcast(String), // We'll convert from commonware-broadcast errors
    #[error("Collector timeout")]
    CollectorTimeout,

    #[error("Cfg: {0}")]
    Cfg(String),

    #[error("No peers with role: {0}")]
    NoPeersForRole(String),

    #[error("CommonwareLookupError: {0}")]
    CommonwareLookupError(#[from] commonware_p2p::authenticated::lookup::Error),

    #[error("Message serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("Invalid node type: {0}")]
    InvalidNodeType(String),

    #[error("Network not initialized")]
    NotInitialized,

    #[error("NodePrivKeyNotFound")]
    NodePrivKeyNotFound,

    #[error("Peer not found: {0}")]
    PeerNotFound(String),

    #[error("Channel error: {0}")]
    ChannelError(String),

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),

    #[error("Auth error: {0}")]
    Auth(#[from] Auth),
    #[error("LLM error: {0}")]
    Llm(String),
    #[error("Orchestration error: {0}")]
    Orchestration(String),
    #[error("Storage error: {0}")]
    Storage(String),
    #[error("WASM error: {0}")]
    Wasm(String),
    #[error("Crypto error: {0}")]
    Crypto(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
    #[error("DeSerialization error: {0}")]
    DeSerialization(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TomlDeErr error: {0}")]
    TomlDeErr(#[from] toml::de::Error),
    #[error("TomlSerErr error: {0}")]
    TomlSerErr(#[from] toml::ser::Error),
    #[error("HTTP client error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("EncodeError: {0}")]
    EncodeError(prost::EncodeError),
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

/// Authentication error types
#[derive(Debug, thiserror::Error)]
pub enum Auth {
    #[error("Missing signature header")]
    MissingSignature,
    #[error("Missing timestamp header")]
    MissingTimestamp,
    #[error("Invalid signature format")]
    InvalidSignature,
    #[error("Signature verification failed")]
    VerificationFailed,
    #[error("Request expired")]
    RequestExpired,
}

impl IntoResponse for Auth {
    fn into_response(self) -> Response {
        use axum::http::StatusCode;
        use serde::Serialize;

        #[derive(Serialize)]
        struct ErrorResponse {
            error: String,
        }

        let (status, message) = match self {
            Auth::MissingSignature | Auth::MissingTimestamp => {
                (StatusCode::UNAUTHORIZED, self.to_string())
            }
            Auth::InvalidSignature | Auth::VerificationFailed => {
                (StatusCode::FORBIDDEN, self.to_string())
            }
            Auth::RequestExpired => (StatusCode::REQUEST_TIMEOUT, self.to_string()),
        };

        (status, axum::Json(ErrorResponse { error: message })).into_response()
    }
}

impl From<Auth> for StatusCode {
    fn from(err: Auth) -> Self {
        match err {
            Auth::MissingSignature | Auth::MissingTimestamp => StatusCode::UNAUTHORIZED,
            Auth::InvalidSignature | Auth::VerificationFailed => StatusCode::FORBIDDEN,
            Auth::RequestExpired => StatusCode::REQUEST_TIMEOUT,
        }
    }
}

impl HoError {
    /// Get error chain as a vector of error messages
    pub fn error_chain(&self) -> Vec<String> {
        let mut chain = vec![self.to_string()];

        // Add source errors
        let mut source = std::error::Error::source(self);
        while let Some(err) = source {
            chain.push(err.to_string());
            source = std::error::Error::source(err);
        }

        chain
    }

    /// Get a backtrace if available
    pub fn backtrace(&self) -> Option<String> {
        match self {
            HoError::Storage(err) => Some(format!("{:?}", err)),
            _ => None,
        }
    }
}

/// Helper function to create error JSON responses
pub fn error_json(message: &str, code: &str) -> serde_json::Value {
    serde_json::json!({
        "error": message,
        "code": code,
        "timestamp": chrono::Utc::now()
    })
}

/// Enhanced error JSON with full error context and trace
/// Controlled by RUST_LOG_DETAIL env var
pub fn error_json_detailed(error: &HoError) -> serde_json::Value {
    let error_chain = error.error_chain();
    let include_trace = should_include_trace();

    let mut json = serde_json::json!({
        "error": error.to_string(),
        "timestamp": chrono::Utc::now(),
    });

    // Add detailed info based on log level
    if include_trace {
        let backtrace = error.backtrace();
        json["error_chain"] = serde_json::json!(error_chain);
        json["backtrace"] = serde_json::json!(backtrace);
        json["details"] = serde_json::json!({
            "primary_error": error_chain.first().cloned(),
            "root_cause": error_chain.last().cloned(),
            "chain_length": error_chain.len()
        });
    }

    json
}

/// Check if we should include detailed traces based on env
fn should_include_trace() -> bool {
    std::env::var("RUST_LOG_DETAIL")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false)
        || std::env::var("RUST_LOG")
            .unwrap_or_default()
            .contains("debug")
        || std::env::var("RUST_LOG")
            .unwrap_or_default()
            .contains("trace")
}

/// Format error trace for storage - respects RUST_LOG_DETAIL env
pub fn format_error_trace(error: &HoError) -> Option<String> {
    if !should_include_trace() {
        return None;
    }

    let error_chain = error.error_chain();
    let mut trace = String::new();

    // Error chain
    trace.push_str("Error Chain:\n");
    for (i, msg) in error_chain.iter().enumerate() {
        trace.push_str(&format!("  [{}] {}\n", i, msg));
    }

    // Backtrace if available
    if let Some(backtrace) = error.backtrace() {
        trace.push_str("\nBacktrace:\n");
        trace.push_str(&backtrace);
    }

    Some(trace)
}

/// Helper function to create API error responses
pub fn api_error(status: StatusCode, message: &str, code: &str) -> Json<serde_json::Value> {
    Json(error_json(
        message,
        &format!("code:{},status: {}", code, status),
    ))
}

/// Helper function to create error responses
pub fn error_response(status: StatusCode, message: &str, code: &str) -> Json<serde_json::Value> {
    Json(error_json(
        message,
        &format!("code:{},status: {}", code, status),
    ))
}
