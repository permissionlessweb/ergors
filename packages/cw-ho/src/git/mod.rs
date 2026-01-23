//! Engine-side git operations for workspace management
//!
//! This module provides the engine-side integration of git workspaces,
//! handling:
//! - Workspace sync coordination over P2P channels
//! - Integration with storage layer for metadata persistence
//! - Git operations orchestration for task workflows
use crate::storage::{
    ErgorsStorage, TASK_WORKTREE_PREFIX, WORKSPACE_PREFIX, WORKTREE_BY_NODE_PREFIX,
    WORKTREE_BY_WORKSPACE_PREFIX,
};
use cnidarium::{StateRead, StateWrite};
use futures::StreamExt;
use ho_std::llm::HoResult;
use tracing::{info, warn};
pub mod coordinator;
pub use coordinator::WorkspaceSyncCoordinator;

impl ErgorsStorage {
    // ========================================
    // Git Workspace Storage Methods
    // ========================================

    /// Store workspace metadata.
    pub async fn put_workspace(
        &self,
        workspace: &ho_std::types::ergors::git::v1::WorkspaceMetadata,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Main workspace record
        let workspace_key = format!("{}{}", WORKSPACE_PREFIX, workspace.workspace_id);
        let workspace_data = serde_json::to_vec(workspace)?;
        delta.put_raw(workspace_key.clone(), workspace_data);

        // Index by timestamp
        if let Some(ref ts) = workspace.created_at {
            let ts_index_key = format!(
                "workspaces_by_time/{:020}:{}",
                ts.seconds, workspace.workspace_id
            );
            delta.put_raw(ts_index_key, workspace.workspace_id.as_bytes().to_vec());
        }

        self.cs.commit(delta).await?;

        info!(
            "💾 Stored workspace: {} (name: {}, remote: {})",
            workspace.workspace_id,
            workspace.name,
            if workspace.remote_url.is_empty() {
                "local"
            } else {
                &workspace.remote_url
            }
        );

        Ok(())
    }

    /// Get workspace metadata by ID.
    pub async fn get_workspace(
        &self,
        workspace_id: &str,
    ) -> HoResult<Option<ho_std::types::ergors::git::v1::WorkspaceMetadata>> {
        let snapshot = self.cs.latest_snapshot();
        let workspace_key = format!("{}{}", WORKSPACE_PREFIX, workspace_id);

        match snapshot.get_raw(&workspace_key).await {
            Ok(Some(data)) => {
                let workspace: ho_std::types::ergors::git::v1::WorkspaceMetadata =
                    serde_json::from_slice(&data)?;
                Ok(Some(workspace))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get workspace {}: {}", workspace_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// List all workspaces.
    pub async fn list_workspaces(
        &self,
    ) -> HoResult<Vec<ho_std::types::ergors::git::v1::WorkspaceMetadata>> {
        let snapshot = self.cs.latest_snapshot();
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(WORKSPACE_PREFIX);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    match serde_json::from_slice::<ho_std::types::ergors::git::v1::WorkspaceMetadata>(
                        &value,
                    ) {
                        Ok(workspace) => {
                            results.push(workspace);
                        }
                        Err(e) => {
                            warn!("Failed to deserialize workspace: {}", e);
                        }
                    }
                }
                Err(e) => {
                    warn!("Error reading workspace stream: {}", e);
                }
            }
        }

        // Sort by created_at (most recent first)
        results.sort_by(|a, b| {
            let b_ts = b.created_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            let a_ts = a.created_at.as_ref().map(|t| t.seconds).unwrap_or(0);
            b_ts.cmp(&a_ts)
        });

        info!("🔍 Listed {} workspaces", results.len());
        Ok(results)
    }

    /// Delete workspace metadata.
    pub async fn delete_workspace(&self, workspace_id: &str) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());
        let workspace_key = format!("{}{}", WORKSPACE_PREFIX, workspace_id);
        delta.delete(workspace_key);
        self.cs.commit(delta).await?;
        info!("🗑️  Deleted workspace: {}", workspace_id);
        Ok(())
    }

    /// Store task worktree metadata.
    pub async fn put_task_worktree(
        &self,
        worktree: &ho_std::types::ergors::git::v1::TaskWorktree,
    ) -> HoResult<()> {
        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Main worktree record
        let worktree_key = format!("{}{}", TASK_WORKTREE_PREFIX, worktree.task_id);
        let worktree_data = serde_json::to_vec(worktree)?;
        delta.put_raw(worktree_key.clone(), worktree_data);

        // Index by workspace
        let workspace_index_key = format!(
            "{}{}:{}",
            WORKTREE_BY_WORKSPACE_PREFIX, worktree.workspace_id, worktree.task_id
        );
        delta.put_raw(workspace_index_key, worktree.task_id.as_bytes().to_vec());

        // Index by assigned node (if any)
        if !worktree.assigned_node_id.is_empty() {
            let node_index_key = format!(
                "{}{}:{}",
                WORKTREE_BY_NODE_PREFIX, worktree.assigned_node_id, worktree.task_id
            );
            delta.put_raw(node_index_key, worktree.task_id.as_bytes().to_vec());
        }

        // Index by timestamp
        if let Some(ref ts) = worktree.created_at {
            let ts_index_key = format!("worktrees_by_time/{:020}:{}", ts.seconds, worktree.task_id);
            delta.put_raw(ts_index_key, worktree.task_id.as_bytes().to_vec());
        }

        self.cs.commit(delta).await?;

        info!(
            "💾 Stored task worktree: {} (workspace: {}, branch: {})",
            worktree.task_id, worktree.workspace_id, worktree.branch
        );

        Ok(())
    }

    /// Get task worktree by task ID.
    pub async fn get_task_worktree(
        &self,
        task_id: &str,
    ) -> HoResult<Option<ho_std::types::ergors::git::v1::TaskWorktree>> {
        let snapshot = self.cs.latest_snapshot();
        let worktree_key = format!("{}{}", TASK_WORKTREE_PREFIX, task_id);

        match snapshot.get_raw(&worktree_key).await {
            Ok(Some(data)) => {
                let worktree: ho_std::types::ergors::git::v1::TaskWorktree =
                    serde_json::from_slice(&data)?;
                Ok(Some(worktree))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                warn!("Failed to get task worktree {}: {}", task_id, e);
                Err(ho_std::error::HoError::Anyhow(e))
            }
        }
    }

    /// List task worktrees for a workspace.
    pub async fn list_task_worktrees_by_workspace(
        &self,
        workspace_id: &str,
    ) -> HoResult<Vec<ho_std::types::ergors::git::v1::TaskWorktree>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = format!("{}{}:", WORKTREE_BY_WORKSPACE_PREFIX, workspace_id);
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(&prefix);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    let task_id = String::from_utf8_lossy(&value);
                    if let Some(worktree) = self.get_task_worktree(&task_id).await? {
                        results.push(worktree);
                    }
                }
                Err(e) => {
                    warn!("Error reading worktree by workspace stream: {}", e);
                }
            }
        }

        Ok(results)
    }

    /// List task worktrees assigned to a node.
    pub async fn list_task_worktrees_by_node(
        &self,
        node_id: &str,
    ) -> HoResult<Vec<ho_std::types::ergors::git::v1::TaskWorktree>> {
        let snapshot = self.cs.latest_snapshot();
        let prefix = format!("{}{}:", WORKTREE_BY_NODE_PREFIX, node_id);
        let mut results = Vec::new();

        let mut stream = snapshot.prefix_raw(&prefix);
        while let Some(entry_result) = stream.next().await {
            match entry_result {
                Ok((_key, value)) => {
                    let task_id = String::from_utf8_lossy(&value);
                    if let Some(worktree) = self.get_task_worktree(&task_id).await? {
                        results.push(worktree);
                    }
                }
                Err(e) => {
                    warn!("Error reading worktree by node stream: {}", e);
                }
            }
        }

        Ok(results)
    }

    /// Delete task worktree metadata.
    pub async fn delete_task_worktree(&self, task_id: &str) -> HoResult<()> {
        // First get the worktree to delete its indices
        let worktree = match self.get_task_worktree(task_id).await? {
            Some(w) => w,
            None => return Ok(()), // Already deleted
        };

        let mut delta = cnidarium::StateDelta::new(self.cs.latest_snapshot());

        // Delete main record
        let worktree_key = format!("{}{}", TASK_WORKTREE_PREFIX, task_id);
        delta.delete(worktree_key);

        // Delete workspace index
        let workspace_index_key = format!(
            "{}{}:{}",
            WORKTREE_BY_WORKSPACE_PREFIX, worktree.workspace_id, task_id
        );
        delta.delete(workspace_index_key);

        // Delete node index
        if !worktree.assigned_node_id.is_empty() {
            let node_index_key = format!(
                "{}{}:{}",
                WORKTREE_BY_NODE_PREFIX, worktree.assigned_node_id, task_id
            );
            delta.delete(node_index_key);
        }

        self.cs.commit(delta).await?;
        info!("🗑️  Deleted task worktree: {}", task_id);
        Ok(())
    }

    /// Count active task worktrees for a workspace.
    pub async fn count_active_worktrees(&self, workspace_id: &str) -> HoResult<usize> {
        use ho_std::types::ergors::git::v1::TaskWorktreeStatus;

        let worktrees = self.list_task_worktrees_by_workspace(workspace_id).await?;
        let active_count = worktrees
            .iter()
            .filter(|w| w.status == TaskWorktreeStatus::Active as i32)
            .count();

        Ok(active_count)
    }
}
