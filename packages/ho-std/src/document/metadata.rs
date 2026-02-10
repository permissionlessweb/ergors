//! Document metadata types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

use super::error::{DocumentError, Result};

/// Document identifier (content hash).
///
/// DocumentId is the Blake3 hash of the document content,
/// making storage content-addressed (same content = same ID).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocumentId(String);

impl DocumentId {
    /// Create DocumentId from content by hashing.
    pub fn from_content(content: &[u8]) -> Self {
        let hash = blake3::hash(content);
        Self(hash.to_hex().to_string())
    }

    /// Create DocumentId from hex string (for deserialization).
    pub fn from_hex(hex: String) -> Result<Self> {
        // Validate hex format (64 hex chars for blake3)
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DocumentError::InvalidId(hex));
        }
        Ok(Self(hex))
    }

    /// Get hex string representation.
    pub fn as_hex(&self) -> &str {
        &self.0
    }

    /// Convert to storage key.
    pub(crate) fn to_storage_key(&self) -> String {
        format!("document/{}", self.0)
    }
}

impl fmt::Display for DocumentId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<DocumentId> for String {
    fn from(id: DocumentId) -> String {
        id.0
    }
}

/// Source type for document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SourceType {
    /// Local file path.
    File { path: String },
    /// GitHub repository.
    GitHub { url: String, commit: Option<String> },
    /// Standard input.
    Stdin,
    /// HTTP/HTTPS URL.
    Http { url: String },
    /// Other/unknown source.
    Other { description: String },
}

impl SourceType {
    /// Parse source from string.
    pub fn from_str(source: &str) -> Self {
        if source.starts_with("http://") || source.starts_with("https://") {
            if source.contains("github.com") {
                Self::GitHub {
                    url: source.to_string(),
                    commit: None,
                }
            } else {
                Self::Http {
                    url: source.to_string(),
                }
            }
        } else if source == "stdin" || source == "-" {
            Self::Stdin
        } else if source.starts_with('/') || source.contains("://") {
            Self::File {
                path: source.to_string(),
            }
        } else {
            Self::Other {
                description: source.to_string(),
            }
        }
    }

    /// Get string representation.
    pub fn as_str(&self) -> String {
        match self {
            Self::File { path } => path.clone(),
            Self::GitHub { url, .. } => url.clone(),
            Self::Stdin => "stdin".to_string(),
            Self::Http { url } => url.clone(),
            Self::Other { description } => description.clone(),
        }
    }
}

/// Document metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentMetadata {
    /// Document name/title.
    pub name: String,
    /// Source of the document.
    pub source: SourceType,
    /// Content hash (for verification).
    pub content_hash: String,
    /// Timestamp when stored.
    pub timestamp: DateTime<Utc>,
    /// Document size in bytes.
    pub size: usize,
}

impl DocumentMetadata {
    /// Create new metadata for document.
    pub fn new(name: impl Into<String>, source: impl Into<String>, content: &[u8]) -> Self {
        let hash = blake3::hash(content);
        Self {
            name: name.into(),
            source: SourceType::from_str(&source.into()),
            content_hash: hash.to_hex().to_string(),
            timestamp: Utc::now(),
            size: content.len(),
        }
    }

    /// Verify content matches metadata hash.
    pub fn verify_content(&self, content: &[u8]) -> Result<()> {
        let actual_hash = blake3::hash(content);
        let actual_hex = actual_hash.to_hex().to_string();

        if actual_hex != self.content_hash {
            return Err(DocumentError::HashMismatch {
                expected: self.content_hash.clone(),
                actual: actual_hex,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_id_from_content() {
        let content = b"test content";
        let id1 = DocumentId::from_content(content);
        let id2 = DocumentId::from_content(content);

        // Same content = same ID
        assert_eq!(id1, id2);
        assert_eq!(id1.as_hex().len(), 64); // Blake3 = 256 bits = 64 hex chars
    }

    #[test]
    fn test_document_id_different_content() {
        let id1 = DocumentId::from_content(b"content1");
        let id2 = DocumentId::from_content(b"content2");

        // Different content = different ID
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_document_id_from_hex_valid() {
        let hex = "a".repeat(64);
        let id = DocumentId::from_hex(hex.clone()).unwrap();
        assert_eq!(id.as_hex(), hex);
    }

    #[test]
    fn test_document_id_from_hex_invalid() {
        // Too short
        assert!(DocumentId::from_hex("abc".to_string()).is_err());

        // Non-hex characters
        assert!(DocumentId::from_hex("z".repeat(64)).is_err());
    }

    #[test]
    fn test_source_type_parsing() {
        let cases = vec![
            ("https://github.com/owner/repo", "github.com"),
            ("https://example.com/doc.pdf", "example.com"),
            ("/path/to/file.txt", "/path/to/file.txt"),
            ("stdin", "stdin"),
            ("-", "stdin"),
        ];

        for (input, expected_contain) in cases {
            let source = SourceType::from_str(input);
            let as_str = source.as_str();
            assert!(
                as_str.contains(expected_contain),
                "Expected '{}' to contain '{}', got '{}'",
                input,
                expected_contain,
                as_str
            );
        }
    }

    #[test]
    fn test_metadata_creation() {
        let content = b"test document content";
        let metadata = DocumentMetadata::new("test.txt", "file:///test.txt", content);

        assert_eq!(metadata.name, "test.txt");
        assert_eq!(metadata.size, content.len());
        assert_eq!(metadata.content_hash.len(), 64);
    }

    #[test]
    fn test_metadata_verify_content_success() {
        let content = b"test content";
        let metadata = DocumentMetadata::new("test", "stdin", content);

        // Verification should succeed
        assert!(metadata.verify_content(content).is_ok());
    }

    #[test]
    fn test_metadata_verify_content_mismatch() {
        let content = b"original content";
        let metadata = DocumentMetadata::new("test", "stdin", content);

        // Verification should fail with different content
        let different = b"modified content";
        let result = metadata.verify_content(different);

        assert!(result.is_err());
        match result {
            Err(DocumentError::HashMismatch { .. }) => (),
            _ => panic!("Expected HashMismatch error"),
        }
    }

    #[test]
    fn test_metadata_serialization() {
        let content = b"test";
        let metadata = DocumentMetadata::new("test.txt", "stdin", content);

        // Serialize and deserialize
        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: DocumentMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(metadata.name, deserialized.name);
        assert_eq!(metadata.content_hash, deserialized.content_hash);
        assert_eq!(metadata.size, deserialized.size);
    }
}
