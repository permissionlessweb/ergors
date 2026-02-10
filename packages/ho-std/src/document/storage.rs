//! Document storage implementation using cnidarium.

use anyhow::Context;
use cnidarium::{StateRead, StateWrite};
use futures::StreamExt;
use tracing::{debug, info, warn};

use super::error::{DocumentError, Result};
use super::metadata::{DocumentId, DocumentMetadata};

/// Document storage interface.
///
/// Provides content-addressed document storage using cnidarium backend.
pub struct DocumentStorage;

impl DocumentStorage {
    /// Maximum document size (50MB).
    pub const MAX_DOCUMENT_SIZE: usize = 50 * 1024 * 1024;

    /// Store a document.
    ///
    /// Returns the DocumentId (content hash).
    /// If document with same content already exists, returns existing ID (idempotent).
    pub async fn store_document<S: StateWrite>(
        state: &mut S,
        content: &[u8],
        name: impl Into<String>,
        source: impl Into<String>,
    ) -> Result<DocumentId> {
        let name = name.into();
        let source = source.into();

        // Validate size
        if content.len() > Self::MAX_DOCUMENT_SIZE {
            return Err(DocumentError::TooLarge(
                content.len(),
                Self::MAX_DOCUMENT_SIZE,
            ));
        }

        // Create metadata
        let metadata = DocumentMetadata::new(&name, &source, content);
        let doc_id = DocumentId::from_content(content);

        debug!(
            "Storing document: id={}, name={}, size={} bytes",
            doc_id,
            name,
            content.len()
        );

        // Check if already exists (idempotency)
        let content_key = Self::content_key(&doc_id);
        if let Some(existing) = state.get_raw(&content_key).await? {
            if !existing.is_empty() {
                info!(
                    "Document already exists: id={}, returning existing ID",
                    doc_id
                );
                return Ok(doc_id);
            }
        }

        // Store content
        state.put_raw(content_key, content.to_vec());

        // Store metadata
        let metadata_key = Self::metadata_key(&doc_id);
        let metadata_bytes = serde_json::to_vec(&metadata)?;
        state.put_raw(metadata_key, metadata_bytes);

        info!("Document stored: id={}, name={}", doc_id, name);
        Ok(doc_id)
    }

    /// Retrieve a document by ID.
    ///
    /// Returns (content, metadata) tuple.
    /// Returns error if document not found.
    pub async fn retrieve_document<S: StateRead>(
        state: &S,
        doc_id: &DocumentId,
    ) -> Result<(Vec<u8>, DocumentMetadata)> {
        debug!("Retrieving document: id={}", doc_id);

        // Get content
        let content_key = Self::content_key(doc_id);
        let content = state
            .get_raw(&content_key)
            .await
            .context("Failed to read content")?
            .ok_or_else(|| DocumentError::NotFound(doc_id.to_string()))?;

        // Get metadata
        let metadata_key = Self::metadata_key(doc_id);
        let metadata_bytes = state
            .get_raw(&metadata_key)
            .await
            .context("Failed to read metadata")?
            .ok_or_else(|| DocumentError::NotFound(format!("{} (metadata missing)", doc_id)))?;

        let metadata: DocumentMetadata = serde_json::from_slice(&metadata_bytes)?;

        // Verify content hash
        if let Err(e) = metadata.verify_content(&content) {
            warn!("Content hash mismatch for document {}: {}", doc_id, e);
            return Err(e);
        }

        debug!(
            "Document retrieved: id={}, size={} bytes",
            doc_id,
            content.len()
        );

        Ok((content, metadata))
    }

    /// List all documents with optional pagination.
    ///
    /// Returns Vec of (DocumentId, DocumentMetadata).
    pub async fn list_documents<S: StateRead>(
        state: &S,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<(DocumentId, DocumentMetadata)>> {
        debug!(
            "Listing documents: limit={:?}, offset={:?}",
            limit, offset
        );

        let prefix = "document/metadata/";
        let stream = state.prefix_raw(prefix);
        futures::pin_mut!(stream);

        let mut documents = Vec::new();
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(usize::MAX);

        let mut count = 0;
        while let Some(result) = stream.next().await {
            let (key, value) = result.context("Failed to read from stream")?;

            // Skip offset
            if count < offset {
                count += 1;
                continue;
            }

            // Apply limit
            if documents.len() >= limit {
                break;
            }

            // Extract document ID from key (key is String, not Vec<u8>)
            if let Some(id_hex) = key.strip_prefix(prefix) {
                match DocumentId::from_hex(id_hex.to_string()) {
                    Ok(doc_id) => {
                        match serde_json::from_slice::<DocumentMetadata>(&value) {
                            Ok(metadata) => {
                                documents.push((doc_id, metadata));
                            }
                            Err(e) => {
                                warn!("Failed to deserialize metadata for {}: {}", id_hex, e);
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Invalid document ID in key {}: {}", key, e);
                        continue;
                    }
                }
            }

            count += 1;
        }

        debug!("Listed {} documents", documents.len());
        Ok(documents)
    }

    /// Delete a document by ID.
    ///
    /// Returns Ok if document deleted or didn't exist.
    pub async fn delete_document<S: StateWrite>(
        state: &mut S,
        doc_id: &DocumentId,
    ) -> Result<()> {
        debug!("Deleting document: id={}", doc_id);

        // Delete content
        let content_key = Self::content_key(doc_id);
        state.delete(content_key);

        // Delete metadata
        let metadata_key = Self::metadata_key(doc_id);
        state.delete(metadata_key);

        info!("Document deleted: id={}", doc_id);
        Ok(())
    }

    /// Get storage key for document content.
    fn content_key(doc_id: &DocumentId) -> String {
        format!("document/content/{}", doc_id.as_hex())
    }

    /// Get storage key for document metadata.
    fn metadata_key(doc_id: &DocumentId) -> String {
        format!("document/metadata/{}", doc_id.as_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cnidarium::StateDelta;
    use cnidarium::Storage;
    use tempfile::TempDir;

    async fn setup_test_storage() -> (Storage, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::load(temp_dir.path().to_path_buf(), vec![])
            .await
            .unwrap();
        (storage, temp_dir)
    }

    #[tokio::test]
    async fn test_store_and_retrieve_document() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        let content = b"test document content";
        let doc_id = DocumentStorage::store_document(&mut state, content, "test.txt", "stdin")
            .await
            .unwrap();

        // Retrieve document
        let (retrieved_content, metadata) =
            DocumentStorage::retrieve_document(&state, &doc_id)
                .await
                .unwrap();

        assert_eq!(retrieved_content, content);
        assert_eq!(metadata.name, "test.txt");
        assert_eq!(metadata.size, content.len());
    }

    #[tokio::test]
    async fn test_store_document_idempotent() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        let content = b"idempotent test";

        // Store twice
        let id1 = DocumentStorage::store_document(&mut state, content, "doc1.txt", "stdin")
            .await
            .unwrap();
        let id2 = DocumentStorage::store_document(&mut state, content, "doc2.txt", "stdin")
            .await
            .unwrap();

        // Same content = same ID
        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn test_retrieve_nonexistent_document() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let state = StateDelta::new(snapshot);

        let fake_id = DocumentId::from_hex("a".repeat(64)).unwrap();
        let result = DocumentStorage::retrieve_document(&state, &fake_id).await;

        assert!(result.is_err());
        match result {
            Err(DocumentError::NotFound(_)) => (),
            _ => panic!("Expected NotFound error"),
        }
    }

    #[tokio::test]
    async fn test_list_documents_empty() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let state = StateDelta::new(snapshot);

        let documents = DocumentStorage::list_documents(&state, None, None)
            .await
            .unwrap();

        assert_eq!(documents.len(), 0);
    }

    #[tokio::test]
    async fn test_list_documents_with_data() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        // Store multiple documents
        DocumentStorage::store_document(&mut state, b"doc1", "doc1.txt", "stdin")
            .await
            .unwrap();
        DocumentStorage::store_document(&mut state, b"doc2", "doc2.txt", "stdin")
            .await
            .unwrap();
        DocumentStorage::store_document(&mut state, b"doc3", "doc3.txt", "stdin")
            .await
            .unwrap();

        // List all
        let documents = DocumentStorage::list_documents(&state, None, None)
            .await
            .unwrap();

        assert_eq!(documents.len(), 3);
    }

    #[tokio::test]
    async fn test_list_documents_with_pagination() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        // Store 5 documents
        for i in 0..5 {
            let content = format!("document {}", i);
            DocumentStorage::store_document(&mut state, content.as_bytes(), format!("doc{}.txt", i), "stdin")
                .await
                .unwrap();
        }

        // Get first 2
        let page1 = DocumentStorage::list_documents(&state, Some(2), Some(0))
            .await
            .unwrap();
        assert_eq!(page1.len(), 2);

        // Get next 2
        let page2 = DocumentStorage::list_documents(&state, Some(2), Some(2))
            .await
            .unwrap();
        assert_eq!(page2.len(), 2);

        // Get last 1
        let page3 = DocumentStorage::list_documents(&state, Some(2), Some(4))
            .await
            .unwrap();
        assert_eq!(page3.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_document() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        let content = b"document to delete";
        let doc_id = DocumentStorage::store_document(&mut state, content, "delete_me.txt", "stdin")
            .await
            .unwrap();

        // Verify it exists
        assert!(DocumentStorage::retrieve_document(&state, &doc_id)
            .await
            .is_ok());

        // Delete
        DocumentStorage::delete_document(&mut state, &doc_id)
            .await
            .unwrap();

        // Verify it's gone
        let result = DocumentStorage::retrieve_document(&state, &doc_id).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_document_too_large() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        // Create document larger than limit
        let large_content = vec![0u8; DocumentStorage::MAX_DOCUMENT_SIZE + 1];
        let result =
            DocumentStorage::store_document(&mut state, &large_content, "huge.txt", "stdin").await;

        assert!(result.is_err());
        match result {
            Err(DocumentError::TooLarge(_, _)) => (),
            _ => panic!("Expected TooLarge error"),
        }
    }

    #[tokio::test]
    async fn test_content_hash_verification() {
        let (storage, _temp_dir) = setup_test_storage().await;
        let snapshot = storage.latest_snapshot();
        let mut state = StateDelta::new(snapshot);

        let content = b"test content";
        let doc_id = DocumentStorage::store_document(&mut state, content, "test.txt", "stdin")
            .await
            .unwrap();

        // Retrieve and verify (should succeed)
        let (retrieved, metadata) = DocumentStorage::retrieve_document(&state, &doc_id)
            .await
            .unwrap();

        assert!(metadata.verify_content(&retrieved).is_ok());
    }
}
