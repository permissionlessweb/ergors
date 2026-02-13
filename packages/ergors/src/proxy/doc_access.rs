//! Document access implementation for RLM callback-based storage integration.
//!
//! Bridges DocumentStorage (ho-std) to ergors-rlm's DocumentAccessTrait,
//! allowing Python REPL workers to access documents on-demand via callbacks
//! instead of loading all documents upfront.

use crate::storage::ErgorsStorage;
use anyhow::Result;
use async_trait::async_trait;
use ho_std::document::{DocumentId, DocumentMetadata, DocumentStorage};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Max chars returned by get_document_section (100KB worth of chars).
const MAX_SECTION_LENGTH: usize = 100_000;

/// Max cached documents before eviction. Each entry holds full document content,
/// so at 50MB max doc size this bounds worst-case cache at ~1.6GB. In practice
/// documents are much smaller, so 32 entries is generous for a single query's
/// working set while preventing unbounded growth across queries.
const MAX_CACHE_ENTRIES: usize = 32;

/// Max total bytes across all cached documents. Evicts everything when exceeded.
/// 256MB is enough for typical workloads without risking OOM.
const MAX_CACHE_BYTES: usize = 256 * 1024 * 1024;

/// Engine-side implementation of document access for RLM.
///
/// Documents are stored in cnidarium via DocumentStorage.
/// This adapter provides metadata listing, section reads, and keyword search
/// without loading full document content into the Python sandbox.
///
/// Wraps document retrieval with a bounded in-memory cache. The cache evicts
/// all entries when it exceeds `MAX_CACHE_ENTRIES` or `MAX_CACHE_BYTES`,
/// preventing unbounded memory growth across queries. The snapshot is
/// point-in-time, so caching introduces zero consistency risk.
pub struct EngineDocumentAccess {
    storage: Arc<ErgorsStorage>,
    cache: RwLock<DocCache>,
}

struct DocCache {
    entries: HashMap<String, (Vec<u8>, DocumentMetadata)>,
    total_bytes: usize,
}

impl DocCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            total_bytes: 0,
        }
    }

    fn get(&self, key: &str) -> Option<&(Vec<u8>, DocumentMetadata)> {
        self.entries.get(key)
    }

    fn insert(&mut self, key: String, value: (Vec<u8>, DocumentMetadata)) {
        let entry_bytes = value.0.len();

        // Evict everything if we'd exceed limits
        if self.entries.len() >= MAX_CACHE_ENTRIES
            || self.total_bytes + entry_bytes > MAX_CACHE_BYTES
        {
            self.entries.clear();
            self.total_bytes = 0;
        }

        self.total_bytes += entry_bytes;
        self.entries.insert(key, value);
    }
}

impl EngineDocumentAccess {
    pub fn new(storage: Arc<ErgorsStorage>) -> Self {
        Self {
            storage,
            cache: RwLock::new(DocCache::new()),
        }
    }

    /// Retrieve document content+metadata, using cache if available.
    ///
    /// Supports prefix-based lookup: if `doc_id` is shorter than the full 64-char
    /// blake3 hex, resolves to the unique document matching that prefix. This is
    /// necessary because LLMs truncate IDs in their output (e.g. `a27cf7b213fd`)
    /// and reuse those truncated strings in subsequent API calls.
    async fn get_document_cached(&self, doc_id: &str) -> Result<(Vec<u8>, DocumentMetadata)> {
        // Resolve prefix to full ID if needed
        let full_id = self.resolve_doc_id(doc_id).await?;

        // Check cache first (always keyed by full ID)
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(&full_id) {
                return Ok(entry.clone());
            }
        }

        // Cache miss — fetch from storage
        let snapshot = self.storage.cs.latest_snapshot();
        let id = DocumentId::from_hex(full_id.clone())
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        let result = DocumentStorage::retrieve_document(&snapshot, &id)
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Store in cache (bounded, keyed by full ID)
        {
            let mut cache = self.cache.write().await;
            cache.insert(full_id, result.clone());
        }

        Ok(result)
    }

    /// Resolve a potentially truncated doc_id to the full 64-char hex ID.
    ///
    /// LLMs often pass mangled IDs copied from their own print output, e.g.:
    ///   - `"a27cf7b213fd"` (truncated prefix)
    ///   - `"a27cf7b213fd... permissionlessweb/akash-deploy-rs"` (prefix + display text)
    ///   - `"a27cf7b213fd... name (1234 bytes)"` (full display line)
    ///
    /// This extracts the leading hex chars and resolves via prefix match.
    async fn resolve_doc_id(&self, doc_id: &str) -> Result<String> {
        // Extract leading hex characters (stop at first non-hex char)
        let hex_prefix: String = doc_id
            .trim()
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();

        // Full-length ID — use directly
        if hex_prefix.len() == 64 {
            return Ok(hex_prefix);
        }

        // Prefix lookup — must be at least 8 chars to avoid ambiguity
        if hex_prefix.len() < 8 {
            anyhow::bail!(
                "Document ID too short: '{}' (extracted '{}', need at least 8 hex chars)",
                doc_id, hex_prefix
            );
        }

        let snapshot = self.storage.cs.latest_snapshot();
        let docs = DocumentStorage::list_documents(&snapshot, Some(100), Some(0))
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        let matches: Vec<_> = docs
            .iter()
            .filter(|(id, _)| id.as_hex().starts_with(&hex_prefix))
            .collect();

        match matches.len() {
            0 => anyhow::bail!("No document found matching prefix: {}", hex_prefix),
            1 => Ok(matches[0].0.as_hex().to_string()),
            n => anyhow::bail!(
                "Ambiguous document prefix '{}' matches {} documents",
                hex_prefix, n
            ),
        }
    }
}

#[cfg(feature = "rlm")]
#[async_trait]
impl ergors_rlm::DocumentAccessTrait for EngineDocumentAccess {
    async fn list_documents(&self, limit: usize, offset: usize) -> Result<Vec<ergors_rlm::DocumentMeta>> {
        let capped_limit = limit.min(100);
        let snapshot = self.storage.cs.latest_snapshot();
        let docs = DocumentStorage::list_documents(&snapshot, Some(capped_limit), Some(offset))
            .await
            .map_err(|e| anyhow::anyhow!("{}", e))?;
        Ok(docs
            .iter()
            .map(|(id, meta)| ergors_rlm::DocumentMeta {
                doc_id: id.to_string(),
                name: meta.name.clone(),
                source: meta.source.as_str(),
                size: meta.size,
            })
            .collect())
    }

    async fn get_document_section(
        &self,
        doc_id: &str,
        offset: usize,
        length: usize,
    ) -> Result<String> {
        let capped_length = length.min(MAX_SECTION_LENGTH);
        let (content, _meta) = self.get_document_cached(doc_id).await?;
        let text = String::from_utf8_lossy(&content);
        let chars: Vec<char> = text.chars().collect();
        let start = offset.min(chars.len());
        let end = (offset + capped_length).min(chars.len());
        Ok(chars[start..end].iter().collect())
    }

    async fn search_in_document(
        &self,
        doc_id: &str,
        query: &str,
        max_results: usize,
    ) -> Result<Vec<ergors_rlm::DocumentExcerpt>> {
        let (content, _meta) = self.get_document_cached(doc_id).await?;
        let text = String::from_utf8_lossy(&content);

        // Work entirely in char-space to avoid UTF-8 boundary panics.
        // Offsets returned are char offsets, matching get_document_section semantics.
        let chars: Vec<char> = text.chars().collect();
        let chars_lower: Vec<char> = text.to_lowercase().chars().collect();
        let query_chars: Vec<char> = query.to_lowercase().chars().collect();

        if query_chars.is_empty() {
            return Ok(vec![]);
        }

        let mut excerpts = Vec::new();
        let mut search_start = 0;

        while search_start + query_chars.len() <= chars_lower.len() {
            // Find next match in char-space
            let found = (search_start..=chars_lower.len() - query_chars.len())
                .find(|&i| chars_lower[i..i + query_chars.len()] == query_chars[..]);

            let Some(match_pos) = found else { break };

            let ctx_start = match_pos.saturating_sub(200);
            let ctx_end = (match_pos + query_chars.len() + 200).min(chars.len());

            excerpts.push(ergors_rlm::DocumentExcerpt {
                doc_id: doc_id.to_string(),
                offset: ctx_start,
                content: chars[ctx_start..ctx_end].iter().collect(),
                match_count: 1,
            });

            if excerpts.len() >= max_results {
                break;
            }
            search_start = match_pos + query_chars.len();
        }
        Ok(excerpts)
    }
}

#[cfg(test)]
#[cfg(feature = "rlm")]
mod tests {
    use super::*;
    use cnidarium::StateDelta;
    use ergors_rlm::DocumentAccessTrait;

    async fn setup_test_storage() -> (Arc<ErgorsStorage>, tempfile::TempDir) {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let storage = ErgorsStorage::new(temp_dir.path(), vec![]).await.unwrap();
        (Arc::new(storage), temp_dir)
    }

    #[tokio::test]
    async fn test_search_in_document_finds_keywords() {
        let (storage, _tmp) = setup_test_storage().await;

        // Store a test document
        let content = b"The quick brown fox jumps over the lazy dog. Authentication works via JWT tokens. The fox is fast.";
        let doc_id = {
            let mut delta = StateDelta::new(storage.cs.latest_snapshot());
            let id = DocumentStorage::store_document(&mut delta, content, "test.txt", "test")
                .await
                .unwrap();
            storage.cs.commit(delta).await.unwrap();
            id
        };

        let access = EngineDocumentAccess::new(storage);

        // Search for "fox"
        let results = access
            .search_in_document(doc_id.as_hex(), "fox", 5)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].content.contains("fox"));
        assert!(results[1].content.contains("fox"));
    }

    #[tokio::test]
    async fn test_list_documents_returns_metadata() {
        let (storage, _tmp) = setup_test_storage().await;

        // Store a document
        {
            let mut delta = StateDelta::new(storage.cs.latest_snapshot());
            DocumentStorage::store_document(&mut delta, b"hello world", "hello.txt", "stdin")
                .await
                .unwrap();
            storage.cs.commit(delta).await.unwrap();
        }

        let access = EngineDocumentAccess::new(storage);
        let docs = access.list_documents(100, 0).await.unwrap();

        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].name, "hello.txt");
        assert_eq!(docs[0].size, 11);
    }

    #[tokio::test]
    async fn test_search_in_document_multibyte_utf8() {
        let (storage, _tmp) = setup_test_storage().await;

        // Content with multi-byte chars: Japanese, emoji, accented chars
        let content = "日本語のドキュメント。authentication works here。もう一つのauthenticationセクション。🎉 end".as_bytes();
        let doc_id = {
            let mut delta = StateDelta::new(storage.cs.latest_snapshot());
            let id = DocumentStorage::store_document(&mut delta, content, "utf8.txt", "test")
                .await
                .unwrap();
            storage.cs.commit(delta).await.unwrap();
            id
        };

        let access = EngineDocumentAccess::new(storage.clone());

        // Search for "authentication" — should not panic on multi-byte boundaries
        let results = access
            .search_in_document(doc_id.as_hex(), "authentication", 5)
            .await
            .unwrap();

        assert_eq!(results.len(), 2);
        assert!(results[0].content.contains("authentication"));
        assert!(results[1].content.contains("authentication"));

        // Verify offset from search works with get_section (char-space consistency)
        let section = access
            .get_document_section(doc_id.as_hex(), results[0].offset, 50)
            .await
            .unwrap();
        assert!(section.contains("authentication"));
    }

    #[tokio::test]
    async fn test_get_document_section_returns_slice() {
        let (storage, _tmp) = setup_test_storage().await;

        let content = b"0123456789abcdef";
        let doc_id = {
            let mut delta = StateDelta::new(storage.cs.latest_snapshot());
            let id = DocumentStorage::store_document(&mut delta, content, "nums.txt", "test")
                .await
                .unwrap();
            storage.cs.commit(delta).await.unwrap();
            id
        };

        let access = EngineDocumentAccess::new(storage);
        let section = access
            .get_document_section(doc_id.as_hex(), 4, 6)
            .await
            .unwrap();

        assert_eq!(section, "456789");
    }

    #[tokio::test]
    async fn test_prefix_based_document_lookup() {
        let (storage, _tmp) = setup_test_storage().await;

        let content = b"Prefix lookup test content";
        let doc_id = {
            let mut delta = StateDelta::new(storage.cs.latest_snapshot());
            let id = DocumentStorage::store_document(&mut delta, content, "prefix.txt", "test")
                .await
                .unwrap();
            storage.cs.commit(delta).await.unwrap();
            id
        };

        let access = EngineDocumentAccess::new(storage);
        let full_hex = doc_id.as_hex();

        // 12-char prefix (what LLMs typically truncate to)
        let prefix = &full_hex[..12];
        let section = access.get_document_section(prefix, 0, 26).await.unwrap();
        assert_eq!(section, "Prefix lookup test content");

        // Search also works with prefix
        let results = access.search_in_document(prefix, "lookup", 5).await.unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].content.contains("lookup"));

        // Too-short prefix (< 8 chars) should fail
        let short = &full_hex[..4];
        assert!(access.get_document_section(short, 0, 10).await.is_err());
    }
}
