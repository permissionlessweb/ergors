//! Storage-related traits for CW-HO system

use crate::error::HoResult;
use async_trait::async_trait;
use uuid::Uuid;

/// Core trait for storage queries
pub trait StorageQueryTrait {
    type Timestamp;

    /// Get session ID filter
    fn session_id(&self) -> Option<&str>;

    /// Get user ID filter
    fn user_id(&self) -> Option<&str>;

    /// Get start time filter
    fn start_time(&self) -> Option<&Self::Timestamp>;

    /// Get end time filter
    fn end_time(&self) -> Option<&Self::Timestamp>;

    /// Get limit
    fn limit(&self) -> Option<u32>;

    /// Get offset
    fn offset(&self) -> Option<u32>;

    /// Get additional filters
    fn filters(&self) -> &std::collections::HashMap<String, String>;

    /// Set session ID filter
    fn set_session_id(&mut self, session_id: String);

    /// Set user ID filter
    fn set_user_id(&mut self, user_id: String);

    /// Set time range
    fn set_time_range(&mut self, start: Self::Timestamp, end: Self::Timestamp);

    /// Set pagination
    fn set_pagination(&mut self, limit: u32, offset: u32);

    /// Add filter
    fn add_filter(&mut self, key: String, value: String);

    /// Set pagination
    fn data_file_path(&mut self, limit: u32, offset: u32);
}

/// Core trait for storage snapshots
pub trait StorageSnapshotTrait {
    type Timestamp;

    /// Get snapshot ID
    fn id(&self) -> &str;

    /// Get creation timestamp
    fn created_at(&self) -> &Self::Timestamp;

    /// Get state root
    fn state_root(&self) -> &str;

    /// Get version
    fn version(&self) -> u64;

    /// Get data
    fn data(&self) -> &std::collections::HashMap<String, Vec<u8>>;

    /// Set state root
    fn set_state_root(&mut self, root: String);

    /// Add data entry
    fn add_data(&mut self, key: String, value: Vec<u8>);

    /// Remove data entry
    fn remove_data(&mut self, key: &str);
}

/// Core trait for storage metrics
pub trait StorageMetricsTrait {
    type Timestamp;

    /// Get total entries
    fn total_entries(&self) -> u64;

    /// Get storage size in bytes
    fn storage_size_bytes(&self) -> u64;

    /// Get index size in bytes
    fn index_size_bytes(&self) -> u64;

    /// Get last compaction time
    fn last_compaction(&self) -> &Self::Timestamp;

    /// Get fragmentation ratio
    fn fragmentation_ratio(&self) -> f64;

    /// Update metrics
    fn update_metrics(
        &mut self,
        entries: u64,
        storage_size: u64,
        index_size: u64,
        fragmentation: f64,
    );

    /// Check if compaction is needed
    fn needs_compaction(&self) -> bool {
        self.fragmentation_ratio() > 0.3 // 30% fragmentation threshold
    }
}

/// Core trait for storage operations
#[async_trait]
pub trait StorageTrait {
    type PromptResponse;
    type PromptRequest;
    type Query: StorageQueryTrait;
    type Snapshot: StorageSnapshotTrait;
    type Metrics: StorageMetricsTrait;

    /// Initialize storage
    async fn new<P: AsRef<std::path::Path> + Send>(data_dir: P) -> HoResult<Self>
    where
        Self: Sized;

    /// Store a prompt response
    async fn store_prompt(&self, prompt: &Self::PromptResponse) -> HoResult<()>;

    /// Store prompt with context
    async fn store_prompt_with_context(
        &self,
        prompt: &Self::PromptResponse,
        request: Option<&Self::PromptRequest>,
    ) -> HoResult<()>;

    /// Get a prompt by ID
    async fn get_prompt(&self, id: &Uuid) -> HoResult<Option<Self::PromptResponse>>;

    /// Query prompts
    async fn query_prompts(&self, query: &Self::Query) -> HoResult<Vec<Self::PromptResponse>>;

    /// Create a snapshot
    async fn create_snapshot(&self) -> HoResult<Self::Snapshot>;

    /// Restore from snapshot
    async fn restore_from_snapshot(&self, snapshot: &Self::Snapshot) -> HoResult<()>;

    /// Prune old data
    async fn prune_storage(&self) -> HoResult<()>;

    /// Get storage metrics
    async fn get_metrics(&self) -> HoResult<Self::Metrics>;

    /// Compact storage
    async fn compact(&self) -> HoResult<()>;

    /// Health check
    async fn health_check(&self) -> HoResult<()>;

    /// Get total stored items
    async fn count(&self) -> HoResult<u64>;

    /// Clear all data (dangerous operation)
    async fn clear_all(&self) -> HoResult<()>;
}

/// Core trait for storage indexing
#[async_trait]
pub trait StorageIndexTrait {
    type Timestamp;

    /// Create index entry
    fn create_index(key: String, value: String) -> Self;

    /// Get key
    fn key(&self) -> &str;

    /// Get value
    fn value(&self) -> &str;

    /// Get creation time
    fn created_at(&self) -> &Self::Timestamp;

    /// Update index
    async fn update_index(&mut self, new_value: String) -> HoResult<()>;

    /// Check if index is expired
    fn is_expired(&self, ttl_seconds: u64) -> bool;
}
