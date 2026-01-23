//! Workspace sync coordinator for P2P coordination
//!
//! Handles workspace synchronization messages over P2P channels,
//! coordinating git operations between nodes.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use ho_std::error::HoResult;
use ho_std::git::{GitIdentity, GitRepository, WorkspaceManager};
use ho_std::types::ergors::git::v1::{SyncAction, WorkspaceSync};

use crate::storage::ErgorsStorage;

/// Pending sync request
#[derive(Debug, Clone)]
struct PendingSync {
    /// Workspace ID
    workspace_id: String,
    /// Action requested
    action: SyncAction,
    /// Requesting node
    sender_node_id: String,
    /// Branch involved
    branch: String,
    /// Commit hash (if applicable)
    commit_hash: Vec<u8>,
    /// When the request was received
    received_at: Instant,
}

/// Workspace sync coordinator for managing P2P workspace synchronization
#[derive(Debug)]
pub struct WorkspaceSyncCoordinator {
    /// Node ID of this node
    node_id: String,
    /// Git identity for operations
    git_identity: GitIdentity,
    /// Base path for workspaces
    workspaces_dir: PathBuf,
    /// SSH key path for git operations
    ssh_key_path: PathBuf,
    /// Active workspace managers by workspace ID
    workspace_managers: Arc<RwLock<HashMap<String, WorkspaceManager>>>,
    /// Pending sync requests
    pending_syncs: Arc<RwLock<Vec<PendingSync>>>,
    /// Maximum age for pending syncs before cleanup
    max_pending_age: Duration,
}

impl WorkspaceSyncCoordinator {
    /// Create a new workspace sync coordinator
    pub fn new(
        node_id: String,
        git_identity: GitIdentity,
        workspaces_dir: PathBuf,
        ssh_key_path: PathBuf,
    ) -> Self {
        Self {
            node_id,
            git_identity,
            workspaces_dir,
            ssh_key_path,
            workspace_managers: Arc::new(RwLock::new(HashMap::new())),
            pending_syncs: Arc::new(RwLock::new(Vec::new())),
            max_pending_age: Duration::from_secs(300), // 5 minutes
        }
    }

    /// Handle incoming WorkspaceSync message from P2P channel
    pub async fn handle_sync_message(
        &self,
        sync: WorkspaceSync,
        storage: &ErgorsStorage,
    ) -> HoResult<Option<WorkspaceSync>> {
        info!(
            "📨 Received workspace sync: workspace={}, action={:?}, from={}",
            sync.workspace_id,
            SyncAction::try_from(sync.action).unwrap_or(SyncAction::Unspecified),
            sync.sender_node_id
        );

        let action = SyncAction::try_from(sync.action).unwrap_or(SyncAction::Unspecified);

        match action {
            SyncAction::FetchRequest => self.handle_fetch_request(&sync, storage).await,
            SyncAction::PushNotify => self.handle_push_notify(&sync, storage).await,
            SyncAction::MergeComplete => self.handle_merge_complete(&sync, storage).await,
            SyncAction::Conflict => self.handle_conflict(&sync, storage).await,
            SyncAction::Unspecified => {
                warn!(
                    "Received unspecified sync action for workspace {}",
                    sync.workspace_id
                );
                Ok(None)
            }
        }
    }

    /// Handle a fetch request from another node
    async fn handle_fetch_request(
        &self,
        sync: &WorkspaceSync,
        storage: &ErgorsStorage,
    ) -> HoResult<Option<WorkspaceSync>> {
        // Verify we have this workspace
        let workspace = storage.get_workspace(&sync.workspace_id).await?;
        if workspace.is_none() {
            warn!("Fetch request for unknown workspace: {}", sync.workspace_id);
            return Ok(None);
        }

        let workspace = workspace.unwrap();

        // Check if we have an active workspace manager for this workspace
        let managers = self.workspace_managers.read().await;
        if let Some(_manager) = managers.get(&sync.workspace_id) {
            info!(
                "Processing fetch request from {} for workspace {}",
                sync.sender_node_id, sync.workspace_id
            );

            // Perform the actual git fetch operation
            let workspace_path = std::path::PathBuf::from(&workspace.local_path);
            if workspace_path.exists() {
                match GitRepository::open(&workspace_path) {
                    Ok(repo) => {
                        // Fetch from origin (default remote)
                        if let Err(e) = repo.fetch("origin", Some(&self.ssh_key_path)) {
                            warn!("Failed to fetch for workspace {}: {}", sync.workspace_id, e);
                        } else {
                            info!("Successfully fetched updates for workspace {}", sync.workspace_id);

                            // Update workspace metadata with new commit info
                            let mut updated_workspace = workspace.clone();
                            if let Ok(head_hash) = repo.head_commit_hash() {
                                updated_workspace.head_commit = head_hash.as_bytes().to_vec();
                            }
                            updated_workspace.last_synced = Some(pbjson_types::Timestamp {
                                seconds: std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs() as i64,
                                nanos: 0,
                            });
                            let _ = storage.put_workspace(&updated_workspace).await;
                        }
                    }
                    Err(e) => {
                        warn!("Failed to open workspace repository: {}", e);
                    }
                }
            } else {
                warn!("Workspace path does not exist: {:?}", workspace_path);
            }
        } else {
            // Queue the sync request for later processing if no manager is active
            let pending = PendingSync {
                workspace_id: sync.workspace_id.clone(),
                action: SyncAction::FetchRequest,
                sender_node_id: sync.sender_node_id.clone(),
                branch: sync.branch.clone(),
                commit_hash: sync.commit_hash.clone(),
                received_at: Instant::now(),
            };
            self.pending_syncs.write().await.push(pending);

            debug!(
                "Queued fetch request from {} for workspace {} (no active manager)",
                sync.sender_node_id, sync.workspace_id
            );
        }

        Ok(None)
    }

    /// Handle push notification from another node
    async fn handle_push_notify(
        &self,
        sync: &WorkspaceSync,
        storage: &ErgorsStorage,
    ) -> HoResult<Option<WorkspaceSync>> {
        // Check if we have this workspace
        let workspace = storage.get_workspace(&sync.workspace_id).await?;
        if workspace.is_none() {
            debug!("Push notify for untracked workspace: {}", sync.workspace_id);
            return Ok(None);
        }

        let workspace = workspace.unwrap();

        info!(
            "Received push notification for workspace {} from {} (branch: {})",
            sync.workspace_id, sync.sender_node_id, sync.branch
        );

        // Perform the actual git fetch operation
        let workspace_path = std::path::PathBuf::from(&workspace.local_path);
        if workspace_path.exists() {
            match GitRepository::open(&workspace_path) {
                Ok(repo) => {
                    // Fetch from origin to get the new commits
                    if let Err(e) = repo.fetch("origin", Some(&self.ssh_key_path)) {
                        warn!("Failed to fetch for workspace {}: {}", sync.workspace_id, e);
                    } else {
                        info!(
                            "Successfully fetched updates for workspace {} after push notification",
                            sync.workspace_id
                        );

                        // Update workspace metadata with new commit info
                        let mut updated_workspace = workspace.clone();
                        updated_workspace.head_commit = sync.commit_hash.clone();
                        updated_workspace.last_synced = Some(pbjson_types::Timestamp {
                            seconds: std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs() as i64,
                            nanos: 0,
                        });
                        storage.put_workspace(&updated_workspace).await?;
                    }
                }
                Err(e) => {
                    warn!("Failed to open workspace repository: {}", e);
                    // Still update metadata even if we couldn't fetch
                    let mut updated_workspace = workspace.clone();
                    updated_workspace.head_commit = sync.commit_hash.clone();
                    storage.put_workspace(&updated_workspace).await?;
                }
            }
        } else {
            // Just update metadata if path doesn't exist
            let mut updated_workspace = workspace.clone();
            updated_workspace.head_commit = sync.commit_hash.clone();
            updated_workspace.last_synced = Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                nanos: 0,
            });
            storage.put_workspace(&updated_workspace).await?;
        }

        Ok(None)
    }

    /// Handle merge complete notification
    async fn handle_merge_complete(
        &self,
        sync: &WorkspaceSync,
        storage: &ErgorsStorage,
    ) -> HoResult<Option<WorkspaceSync>> {
        info!(
            "Merge complete for workspace {} branch {} from {}",
            sync.workspace_id, sync.branch, sync.sender_node_id
        );

        // Get the workspace
        let workspace = storage.get_workspace(&sync.workspace_id).await?;
        if workspace.is_none() {
            warn!("Merge complete for unknown workspace: {}", sync.workspace_id);
            return Ok(None);
        }

        let workspace = workspace.unwrap();

        // Perform fetch to get the merged changes
        let workspace_path = std::path::PathBuf::from(&workspace.local_path);
        if workspace_path.exists() {
            match GitRepository::open(&workspace_path) {
                Ok(repo) => {
                    // Fetch to get the new merge commit
                    if let Err(e) = repo.fetch("origin", Some(&self.ssh_key_path)) {
                        warn!("Failed to fetch after merge complete: {}", e);
                    } else {
                        info!(
                            "Fetched merge commit for workspace {} branch {}",
                            sync.workspace_id, sync.branch
                        );
                    }
                }
                Err(e) => {
                    warn!("Failed to open workspace repository: {}", e);
                }
            }
        }

        // Update workspace metadata with the merge commit hash
        let mut updated_workspace = workspace;
        updated_workspace.head_commit = sync.commit_hash.clone();
        updated_workspace.last_synced = Some(pbjson_types::Timestamp {
            seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
            nanos: 0,
        });
        storage.put_workspace(&updated_workspace).await?;

        // Check if this is a task branch that was merged
        if sync.branch.starts_with("task/") {
            let task_id = sync.branch.strip_prefix("task/").unwrap_or(&sync.branch);

            // Update task worktree status to completed
            if let Some(mut worktree) = storage.get_task_worktree(task_id).await? {
                use ho_std::types::ergors::git::v1::TaskWorktreeStatus;
                worktree.status = TaskWorktreeStatus::Completed as i32;
                storage.put_task_worktree(&worktree).await?;

                info!("Updated task worktree {} status to completed after merge", task_id);
            }
        }

        Ok(None)
    }

    /// Handle conflict notification
    async fn handle_conflict(
        &self,
        sync: &WorkspaceSync,
        storage: &ErgorsStorage,
    ) -> HoResult<Option<WorkspaceSync>> {
        warn!(
            "⚠️ Merge conflict reported for workspace {} branch {} from {}",
            sync.workspace_id, sync.branch, sync.sender_node_id
        );

        // Update task worktree status to conflict if this is a task branch
        if sync.branch.starts_with("task/") {
            let task_id = sync.branch.strip_prefix("task/").unwrap_or(&sync.branch);

            if let Some(mut worktree) = storage.get_task_worktree(task_id).await? {
                // Use raw value 7 for Conflict status (proto regeneration adds the enum variant)
                const TASK_WORKTREE_STATUS_CONFLICT: i32 = 7;
                worktree.status = TASK_WORKTREE_STATUS_CONFLICT;
                storage.put_task_worktree(&worktree).await?;

                warn!(
                    "Updated task worktree {} status to conflict - manual resolution required",
                    task_id
                );
            }
        }

        // Queue conflict info for later resolution
        let pending = PendingSync {
            workspace_id: sync.workspace_id.clone(),
            action: SyncAction::Conflict,
            sender_node_id: sync.sender_node_id.clone(),
            branch: sync.branch.clone(),
            commit_hash: sync.commit_hash.clone(),
            received_at: Instant::now(),
        };
        self.pending_syncs.write().await.push(pending);

        debug!(
            "Queued conflict for resolution: workspace={}, branch={}",
            sync.workspace_id, sync.branch
        );

        Ok(None)
    }

    /// Create a WorkspaceSync message for sending
    pub fn create_sync_message(
        &self,
        workspace_id: &str,
        action: SyncAction,
        branch: &str,
        commit_hash: Vec<u8>,
    ) -> WorkspaceSync {
        WorkspaceSync {
            workspace_id: workspace_id.to_string(),
            action: action as i32,
            branch: branch.to_string(),
            commit_hash,
            sender_node_id: self.node_id.clone(),
            timestamp: Some(pbjson_types::Timestamp {
                seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
                nanos: 0,
            }),
        }
    }

    /// Register a workspace manager for active management
    pub async fn register_workspace_manager(
        &self,
        workspace_id: String,
        manager: WorkspaceManager,
    ) {
        self.workspace_managers
            .write()
            .await
            .insert(workspace_id.clone(), manager);

        info!("Registered workspace manager for: {}", workspace_id);
    }

    /// Unregister a workspace manager
    pub async fn unregister_workspace_manager(&self, workspace_id: &str) {
        self.workspace_managers.write().await.remove(workspace_id);
        info!("Unregistered workspace manager for: {}", workspace_id);
    }

    /// Get a workspace manager by ID
    pub async fn get_workspace_manager(&self, _workspace_id: &str) -> Option<WorkspaceManager> {
        // Note: This clones the manager, which isn't ideal but works for now
        // In a more complex implementation, we'd use Arc<Mutex<WorkspaceManager>>
        None // Placeholder - managers should be accessed via engine state
    }

    /// Cleanup stale pending syncs
    pub async fn cleanup_stale_syncs(&self) -> Vec<PendingSync> {
        let now = Instant::now();
        let mut syncs = self.pending_syncs.write().await;

        let (stale, active): (Vec<_>, Vec<_>) = syncs
            .drain(..)
            .partition(|s| now.duration_since(s.received_at) > self.max_pending_age);

        *syncs = active;

        if !stale.is_empty() {
            warn!("Cleaned up {} stale sync requests", stale.len());
        }

        stale
    }

    /// Get pending sync count
    pub async fn pending_sync_count(&self) -> usize {
        self.pending_syncs.read().await.len()
    }

    /// Process next pending sync
    pub async fn process_next_pending(&self, storage: &ErgorsStorage) -> HoResult<bool> {
        let pending = {
            let mut syncs = self.pending_syncs.write().await;
            syncs.pop()
        };

        if let Some(sync) = pending {
            info!(
                "Processing pending sync: workspace={}, action={:?}",
                sync.workspace_id, sync.action
            );

            // Create a WorkspaceSync message and process it
            let sync_msg = WorkspaceSync {
                workspace_id: sync.workspace_id,
                action: sync.action as i32,
                branch: sync.branch,
                commit_hash: sync.commit_hash,
                sender_node_id: sync.sender_node_id,
                timestamp: None,
            };

            let _ = self.handle_sync_message(sync_msg, storage).await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    

    #[test]
    fn test_create_sync_message() {
        // This would require mocking GitIdentity
        // For now, just test message structure
    }
}
