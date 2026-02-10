//! Error types for document storage.

use thiserror::Error;

/// Document storage error types.
#[derive(Debug, Error)]
pub enum DocumentError {
    /// Document not found in storage.
    #[error("Document not found: {0}")]
    NotFound(String),

    /// Storage backend error.
    #[error("Storage error: {0}")]
    Storage(#[from] anyhow::Error),

    /// Serialization/deserialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Invalid document ID format.
    #[error("Invalid document ID: {0}")]
    InvalidId(String),

    /// Document content too large.
    #[error("Document too large: {0} bytes (max: {1} bytes)")]
    TooLarge(usize, usize),

    /// Content hash mismatch (corruption detected).
    #[error("Content hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    /// GitHub integration error.
    #[error("GitHub error: {0}")]
    GitHub(String),

    /// Invalid source URL.
    #[error("Invalid source: {0}")]
    InvalidSource(String),

    /// IO error (file operations).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for document operations.
pub type Result<T> = std::result::Result<T, DocumentError>;
