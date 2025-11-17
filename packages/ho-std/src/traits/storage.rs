//! Storage-related traits for ERGORS system

use crate::error::HoResult;
use async_trait::async_trait;
use cnidarium::StateRead;
use std::{any::Any, collections::BTreeMap};
use uuid::Uuid;

/// Write access to chain state.
pub trait StateWrite: StateRead + Send + Sync {
    /// Puts raw bytes into the verifiable key-value store with the given key.
    fn put_raw(&mut self, key: String, value: Vec<u8>);

    /// Delete a key from the verifiable key-value store.
    fn delete(&mut self, key: String);

    /// Puts raw bytes into the non-verifiable key-value store with the given key.
    fn nonverifiable_put_raw(&mut self, key: Vec<u8>, value: Vec<u8>);

    /// Delete a key from non-verifiable key-value storage.
    fn nonverifiable_delete(&mut self, key: Vec<u8>);

    /// Puts an object into the ephemeral object store with the given key.
    ///
    /// # Panics
    ///
    /// If the object is already present in the store, but its type is not the same as the type of
    /// `value`.
    fn object_put<T: Clone + Any + Send + Sync>(&mut self, key: &'static str, value: T);

    /// Deletes a key from the ephemeral object store.
    fn object_delete(&mut self, key: &'static str);

    /// Merge a set of object changes into this `StateWrite`.
    ///
    /// Unlike `object_put`, this avoids re-boxing values and messing up the downcasting.
    fn object_merge(&mut self, objects: BTreeMap<&'static str, Option<Box<dyn Any + Send + Sync>>>);

    // Record that an ABCI event occurred while building up this set of state changes.
    // fn record(&mut self, event: abci::Event);
}

impl<'a, S: StateWrite + Send + Sync> StateWrite for &'a mut S {
    fn put_raw(&mut self, key: String, value: jmt::OwnedValue) {
        (**self).put_raw(key, value)
    }

    fn delete(&mut self, key: String) {
        (**self).delete(key)
    }

    fn nonverifiable_delete(&mut self, key: Vec<u8>) {
        (**self).nonverifiable_delete(key)
    }

    fn nonverifiable_put_raw(&mut self, key: Vec<u8>, value: Vec<u8>) {
        (**self).nonverifiable_put_raw(key, value)
    }

    fn object_put<T: Clone + Any + Send + Sync>(&mut self, key: &'static str, value: T) {
        (**self).object_put(key, value)
    }

    fn object_delete(&mut self, key: &'static str) {
        (**self).object_delete(key)
    }

    fn object_merge(
        &mut self,
        objects: BTreeMap<&'static str, Option<Box<dyn Any + Send + Sync>>>,
    ) {
        (**self).object_merge(objects)
    }

    // fn record(&mut self, event: abci::Event) {
    //     (**self).record(event)
    // }
}

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
    async fn put_prompt(&self, prompt: &Self::PromptResponse) -> HoResult<()>;

    /// Store prompt with context
    async fn put_prompt_w_ctx(
        &self,
        prompt: &Self::PromptResponse,
        request: Option<&Self::PromptRequest>,
    ) -> HoResult<()>;

    /// Get a prompt by ID
    async fn get_prompt(&self, id: &Uuid) -> HoResult<Option<Self::PromptResponse>>;

    /// Query prompts
    async fn get_prompts(&self, query: &Self::Query) -> HoResult<Vec<Self::PromptResponse>>;

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
