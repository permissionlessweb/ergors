//! Workspace registry for tracking git workspaces and task worktrees
//!
//! The registry maintains metadata about:
//! - Registered project workspaces (git repos)
//! - Active task worktrees
//! - Sync status with other nodes
//!
//! This metadata is stored in Cnidarium for persistence and replication,
//! while the actual git operations happen via the workspace module.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::HoResult;
use crate::llm::HoError;

/// Workspace metadata stored in registry
#[derive(Debug, Clone)]
pub struct WorkspaceEntry {
    /// Unique workspace identifier (derived from project name or remote URL)
    pub workspace_id: String,
    /// Human-readable name
    pub name: String,
    /// Remote URL (if any)
    pub remote_url: Option<String>,
    /// Local path under workspaces directory
    pub local_path: PathBuf,
    /// Current HEAD commit hash
    pub head_commit: Option<Vec<u8>>,
    /// Default branch name
    pub default_branch: String,
    /// When the workspace was registered
    pub created_at: u64,
    /// Last sync timestamp
    pub last_synced: Option<u64>,
}

impl WorkspaceEntry {
    /// Create a new workspace entry
    pub fn new(workspace_id: String, name: String, local_path: PathBuf) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            workspace_id,
            name,
            remote_url: None,
            local_path,
            head_commit: None,
            default_branch: "main".to_string(),
            created_at: now,
            last_synced: None,
        }
    }

    /// Create from a remote URL
    pub fn from_remote(
        workspace_id: String,
        name: String,
        remote_url: String,
        local_path: PathBuf,
    ) -> Self {
        let mut entry = Self::new(workspace_id, name, local_path);
        entry.remote_url = Some(remote_url);
        entry
    }

    /// Update the HEAD commit
    pub fn update_head(&mut self, commit_hash: Vec<u8>) {
        self.head_commit = Some(commit_hash);
        self.last_synced = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        );
    }
}

/// Task worktree entry
#[derive(Debug, Clone)]
pub struct TaskWorktreeEntry {
    /// Task ID
    pub task_id: String,
    /// Parent workspace ID
    pub workspace_id: String,
    /// Branch name (task/{task_id})
    pub branch: String,
    /// Path to the worktree
    pub worktree_path: PathBuf,
    /// Base commit this worktree branched from
    pub base_commit: Option<Vec<u8>>,
    /// Current status
    pub status: TaskWorktreeStatus,
    /// Node assigned to this task
    pub assigned_node_id: Option<String>,
    /// When the worktree was created
    pub created_at: u64,
}

/// Status of a task worktree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskWorktreeStatus {
    /// Worktree is being created
    Creating,
    /// Worktree is active and ready
    Active,
    /// Changes being committed
    Committing,
    /// Being merged to main
    Merging,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
}

impl TaskWorktreeEntry {
    /// Create a new task worktree entry
    pub fn new(task_id: String, workspace_id: String, worktree_path: PathBuf) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        Self {
            branch: format!("task/{}", task_id),
            task_id,
            workspace_id,
            worktree_path,
            base_commit: None,
            status: TaskWorktreeStatus::Creating,
            assigned_node_id: None,
            created_at: now,
        }
    }

    /// Set the assigned node
    pub fn assign_to_node(&mut self, node_id: String) {
        self.assigned_node_id = Some(node_id);
    }

    /// Update status
    pub fn set_status(&mut self, status: TaskWorktreeStatus) {
        self.status = status;
    }
}

/// Workspace registry for tracking all workspaces and task worktrees
#[derive(Debug)]
pub struct WorkspaceRegistry {
    /// Base directory for all workspaces
    workspaces_dir: PathBuf,
    /// Registered workspaces by ID
    workspaces: HashMap<String, WorkspaceEntry>,
    /// Active task worktrees by task ID
    task_worktrees: HashMap<String, TaskWorktreeEntry>,
}

impl WorkspaceRegistry {
    /// Create a new workspace registry
    pub fn new(workspaces_dir: PathBuf) -> Self {
        Self {
            workspaces_dir,
            workspaces: HashMap::new(),
            task_worktrees: HashMap::new(),
        }
    }

    /// Get the workspaces directory
    pub fn workspaces_dir(&self) -> &Path {
        &self.workspaces_dir
    }

    /// Register a new empty workspace
    pub fn register_empty(&mut self, name: &str) -> HoResult<&WorkspaceEntry> {
        let workspace_id = sanitize_workspace_id(name);

        if self.workspaces.contains_key(&workspace_id) {
            return Err(HoError::Cfg(format!(
                "Workspace '{}' already registered",
                workspace_id
            )));
        }

        let local_path = self.workspaces_dir.join(&workspace_id);
        let entry = WorkspaceEntry::new(workspace_id.clone(), name.to_string(), local_path);

        self.workspaces.insert(workspace_id.clone(), entry);
        Ok(self.workspaces.get(&workspace_id).unwrap())
    }

    /// Register a workspace from a remote URL
    pub fn register_from_remote(
        &mut self,
        name: &str,
        remote_url: &str,
    ) -> HoResult<&WorkspaceEntry> {
        let workspace_id = sanitize_workspace_id(name);

        if self.workspaces.contains_key(&workspace_id) {
            return Err(HoError::Cfg(format!(
                "Workspace '{}' already registered",
                workspace_id
            )));
        }

        let local_path = self.workspaces_dir.join(&workspace_id);
        let entry = WorkspaceEntry::from_remote(
            workspace_id.clone(),
            name.to_string(),
            remote_url.to_string(),
            local_path,
        );

        self.workspaces.insert(workspace_id.clone(), entry);
        Ok(self.workspaces.get(&workspace_id).unwrap())
    }

    /// Get a workspace by ID
    pub fn get_workspace(&self, workspace_id: &str) -> Option<&WorkspaceEntry> {
        self.workspaces.get(workspace_id)
    }

    /// Get a mutable workspace by ID
    pub fn get_workspace_mut(&mut self, workspace_id: &str) -> Option<&mut WorkspaceEntry> {
        self.workspaces.get_mut(workspace_id)
    }

    /// List all registered workspaces
    pub fn list_workspaces(&self) -> Vec<&WorkspaceEntry> {
        self.workspaces.values().collect()
    }

    /// Unregister a workspace
    pub fn unregister(&mut self, workspace_id: &str) -> HoResult<WorkspaceEntry> {
        // Check for active task worktrees
        let active_tasks: Vec<_> = self
            .task_worktrees
            .values()
            .filter(|t| t.workspace_id == workspace_id && t.status == TaskWorktreeStatus::Active)
            .collect();

        if !active_tasks.is_empty() {
            return Err(HoError::Cfg(format!(
                "Cannot unregister workspace '{}': {} active task worktrees",
                workspace_id,
                active_tasks.len()
            )));
        }

        self.workspaces
            .remove(workspace_id)
            .ok_or_else(|| HoError::Cfg(format!("Workspace '{}' not found", workspace_id)))
    }

    /// Register a task worktree
    pub fn register_task_worktree(
        &mut self,
        task_id: &str,
        workspace_id: &str,
    ) -> HoResult<&TaskWorktreeEntry> {
        // Verify workspace exists
        let workspace = self
            .workspaces
            .get(workspace_id)
            .ok_or_else(|| HoError::Cfg(format!("Workspace '{}' not found", workspace_id)))?;

        if self.task_worktrees.contains_key(task_id) {
            return Err(HoError::Cfg(format!(
                "Task worktree '{}' already exists",
                task_id
            )));
        }

        let worktree_path = workspace
            .local_path
            .join("tasks")
            .join(format!("task-{}", task_id));
        let entry =
            TaskWorktreeEntry::new(task_id.to_string(), workspace_id.to_string(), worktree_path);

        self.task_worktrees.insert(task_id.to_string(), entry);
        Ok(self.task_worktrees.get(task_id).unwrap())
    }

    /// Get a task worktree by task ID
    pub fn get_task_worktree(&self, task_id: &str) -> Option<&TaskWorktreeEntry> {
        self.task_worktrees.get(task_id)
    }

    /// Get a mutable task worktree by task ID
    pub fn get_task_worktree_mut(&mut self, task_id: &str) -> Option<&mut TaskWorktreeEntry> {
        self.task_worktrees.get_mut(task_id)
    }

    /// List all task worktrees for a workspace
    pub fn list_task_worktrees(&self, workspace_id: &str) -> Vec<&TaskWorktreeEntry> {
        self.task_worktrees
            .values()
            .filter(|t| t.workspace_id == workspace_id)
            .collect()
    }

    /// List all task worktrees
    pub fn list_all_task_worktrees(&self) -> Vec<&TaskWorktreeEntry> {
        self.task_worktrees.values().collect()
    }

    /// Remove a task worktree from registry
    pub fn remove_task_worktree(&mut self, task_id: &str) -> Option<TaskWorktreeEntry> {
        self.task_worktrees.remove(task_id)
    }

    /// Count active tasks for a workspace
    pub fn active_task_count(&self, workspace_id: &str) -> usize {
        self.task_worktrees
            .values()
            .filter(|t| t.workspace_id == workspace_id && t.status == TaskWorktreeStatus::Active)
            .count()
    }

    /// Get workspaces with pending sync
    pub fn workspaces_pending_sync(&self) -> Vec<&WorkspaceEntry> {
        self.workspaces
            .values()
            .filter(|w| w.last_synced.is_none() || w.remote_url.is_some())
            .collect()
    }
}

/// Sanitize a name into a valid workspace ID
fn sanitize_workspace_id(name: &str) -> String {
    name.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_workspace_id() {
        assert_eq!(sanitize_workspace_id("My Project"), "my-project");
        assert_eq!(sanitize_workspace_id("project_name"), "project_name");
        assert_eq!(sanitize_workspace_id("foo/bar"), "foo-bar");
    }

    #[test]
    fn test_workspace_entry_creation() {
        let entry = WorkspaceEntry::new(
            "test-ws".to_string(),
            "Test Workspace".to_string(),
            PathBuf::from("/tmp/test"),
        );

        assert_eq!(entry.workspace_id, "test-ws");
        assert_eq!(entry.default_branch, "main");
        assert!(entry.created_at > 0);
    }

    #[test]
    fn test_task_worktree_entry() {
        let entry = TaskWorktreeEntry::new(
            "task-123".to_string(),
            "project-a".to_string(),
            PathBuf::from("/tmp/tasks/task-123"),
        );

        assert_eq!(entry.branch, "task/task-123");
        assert_eq!(entry.status, TaskWorktreeStatus::Creating);
    }
}
