//! Document storage module for ERGORS.
//!
//! Provides direct document storage without RAG-specific features (no embeddings, no chunking at storage level).
//! Documents are content-addressed: DocumentId = hash(content) for idempotency.
//!
//! ## Features
//! - Store entire documents directly to cnidarium storage
//! - Content-addressed storage (same content = same ID)
//! - Metadata tracking (name, source, timestamp, hash)
//! - Large file support via chunking at transport layer
//! - GitHub repository ingestion (via githem)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use ho_std::document::{DocumentStorage, DocumentMetadata};
//!
//! // Store document
//! let content = b"Document content";
//! let metadata = DocumentMetadata::new("doc.txt", "file:///path/to/doc.txt");
//! let doc_id = storage.store_document(content, metadata).await?;
//!
//! // Retrieve document
//! let (content, metadata) = storage.retrieve_document(&doc_id).await?;
//!
//! // List documents
//! let documents = storage.list_documents(None, None).await?;
//!
//! // Delete document
//! storage.delete_document(&doc_id).await?;
//! ```

pub mod error;
pub mod metadata;
pub mod storage;
// github_ingest module removed - Discord-specific code moved to gateway package

pub use error::{DocumentError, Result};
pub use metadata::{DocumentId, DocumentMetadata, SourceType};
pub use storage::DocumentStorage;
