use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// Open Responses error types per specification
#[derive(Debug, Clone)]
pub enum OpenResponsesError {
    /// The request is malformed or semantically invalid (400)
    InvalidRequest { param: String, message: String },
    /// The requested resource does not exist (404)
    NotFound { message: String },
    /// The model failed while processing (500)
    ModelError { message: String },
    /// Rate limiting (429)
    TooManyRequests { message: String },
    /// Internal server error (500)
    ServerError { message: String },
}

impl OpenResponsesError {
    pub fn error_type(&self) -> &str {
        match self {
            Self::InvalidRequest { .. } => "invalid_request_error",
            Self::NotFound { .. } => "not_found_error",
            Self::ModelError { .. } => "model_error",
            Self::TooManyRequests { .. } => "too_many_requests",
            Self::ServerError { .. } => "server_error",
        }
    }

    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidRequest { .. } => StatusCode::BAD_REQUEST,
            Self::NotFound { .. } => StatusCode::NOT_FOUND,
            Self::ModelError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            Self::TooManyRequests { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::ServerError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        let (param, message) = match self {
            Self::InvalidRequest { param, message } => (Some(param.as_str()), message.as_str()),
            Self::NotFound { message } => (None, message.as_str()),
            Self::ModelError { message } => (None, message.as_str()),
            Self::TooManyRequests { message } => (None, message.as_str()),
            Self::ServerError { message } => (None, message.as_str()),
        };

        json!({
            "error": {
                "type": self.error_type(),
                "message": message,
                "param": param,
                "code": null,
            }
        })
    }

    /// Emit as an SSE event for streaming error handling
    pub fn to_sse_data(&self) -> String {
        serde_json::to_string(&self.to_json()).unwrap_or_default()
    }
}

impl IntoResponse for OpenResponsesError {
    fn into_response(self) -> Response {
        (self.status_code(), Json(self.to_json())).into_response()
    }
}
