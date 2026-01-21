//! Workspace management for parallel task execution
//!
//! Manages git worktrees for isolated task execution. Each task gets its own
//! worktree branched from main, enabling parallel work without conflicts.

use crate::error::HoResult;
use crate::llm::HoError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{GitIdentity, GitRepository, MergeResult};

/// Status of a workspace/worktree
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceStatus {
    /// Worktree is being created
    Creating,
    /// Worktree is active and ready for work
    Active,
    /// Changes are being committed
    Committing,
    /// Worktree is being merged to main
    Merging,
    /// Worktree is being cleaned up
    Cleanup,
    /// Task completed successfully
    Completed,
    /// Task failed
    Failed,
}

/// A single workspace (git worktree) for a task
#[derive(Debug)]
pub struct Workspace {
    /// Task ID this workspace is for
    pub task_id: String,
    /// Workspace ID (derived from project + task)
    pub workspace_id: String,
    /// Path to the worktree
    pub path: PathBuf,
    /// Branch name (task/{task_id})
    pub branch: String,
    /// When the workspace was created
    pub created_at: Instant,
    /// Current status
    pub status: WorkspaceStatus,
}

impl Workspace {
    /// Create a new workspace descriptor
    pub fn new(task_id: String, project_id: &str, base_path: &Path) -> Self {
        let branch = format!("task/{}", task_id);
        let workspace_id = format!("{}:{}", project_id, task_id);
        let path = base_path.join("tasks").join(format!("task-{}", task_id));

        Self {
            task_id,
            workspace_id,
            path,
            branch,
            created_at: Instant::now(),
            status: WorkspaceStatus::Creating,
        }
    }
}

/// Manages workspaces for a single project
#[derive(Debug)]
pub struct WorkspaceManager {
    /// Project ID
    project_id: String,
    /// Base path for the project (contains .git, main/, tasks/)
    base_path: PathBuf,
    /// Main repository
    repo: GitRepository,
    /// Git identity for commits
    identity: GitIdentity,
    /// SSH key path for remote operations
    ssh_key_path: PathBuf,
    /// Active workspaces
    workspaces: HashMap<String, Workspace>,
    /// Maximum concurrent workspaces
    max_workspaces: usize,
}

impl WorkspaceManager {
    /// Create a new workspace manager for an existing project
    pub fn new(
        project_id: String,
        base_path: PathBuf,
        identity: GitIdentity,
        ssh_key_path: PathBuf,
    ) -> HoResult<Self> {
        let repo = GitRepository::open(&base_path)?;

        Ok(Self {
            project_id,
            base_path,
            repo,
            identity,
            ssh_key_path,
            workspaces: HashMap::new(),
            max_workspaces: 10,
        })
    }

    /// Initialize a new project workspace from a remote URL
    pub fn init_from_remote(
        project_id: String,
        remote_url: &str,
        base_path: PathBuf,
        identity: GitIdentity,
        ssh_key_path: PathBuf,
    ) -> HoResult<Self> {
        // Clone into the main worktree
        let main_path = base_path.join("main");
        let repo = GitRepository::clone_with_ssh(remote_url, &main_path, Some(&ssh_key_path))?;

        // Create tasks directory
        std::fs::create_dir_all(base_path.join("tasks"))
            .map_err(|e| HoError::Cfg(format!("Failed to create tasks directory: {}", e)))?;

        Ok(Self {
            project_id,
            base_path,
            repo,
            identity,
            ssh_key_path,
            workspaces: HashMap::new(),
            max_workspaces: 10,
        })
    }

    /// Initialize a new empty project workspace
    pub fn init_empty(
        project_id: String,
        base_path: PathBuf,
        identity: GitIdentity,
        ssh_key_path: PathBuf,
    ) -> HoResult<Self> {
        // Initialize main worktree
        let main_path = base_path.join("main");
        let repo = GitRepository::init(&main_path)?;

        // Create tasks directory
        std::fs::create_dir_all(base_path.join("tasks"))
            .map_err(|e| HoError::Cfg(format!("Failed to create tasks directory: {}", e)))?;

        Ok(Self {
            project_id,
            base_path,
            repo,
            identity,
            ssh_key_path,
            workspaces: HashMap::new(),
            max_workspaces: 10,
        })
    }

    /// Get project ID
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    /// Get base path
    pub fn base_path(&self) -> &Path {
        &self.base_path
    }

    /// Create a new workspace for a task
    pub fn create_workspace(&mut self, task_id: &str) -> HoResult<&Workspace> {
        if self.workspaces.len() >= self.max_workspaces {
            return Err(HoError::Cfg(format!(
                "Maximum workspaces ({}) reached",
                self.max_workspaces
            )));
        }

        if self.workspaces.contains_key(task_id) {
            return Err(HoError::Cfg(format!(
                "Workspace for task {} already exists",
                task_id
            )));
        }

        let mut workspace = Workspace::new(task_id.to_string(), &self.project_id, &self.base_path);

        // Create the worktree
        self.repo.create_worktree(
            &format!("task-{}", task_id),
            &workspace.path,
            &workspace.branch,
        )?;

        workspace.status = WorkspaceStatus::Active;
        self.workspaces.insert(task_id.to_string(), workspace);

        Ok(self.workspaces.get(task_id).unwrap())
    }

    /// Get a workspace by task ID
    pub fn get_workspace(&self, task_id: &str) -> Option<&Workspace> {
        self.workspaces.get(task_id)
    }

    /// Get mutable workspace by task ID
    pub fn get_workspace_mut(&mut self, task_id: &str) -> Option<&mut Workspace> {
        self.workspaces.get_mut(task_id)
    }

    /// List all active workspaces
    pub fn list_workspaces(&self) -> Vec<&Workspace> {
        self.workspaces.values().collect()
    }

    /// Complete a task - commit changes, merge to main, cleanup
    /// Returns the merge commit hash on success
    pub fn complete_task(&mut self, task_id: &str, commit_message: &str) -> HoResult<TaskCompleteResult> {
        let workspace = self
            .workspaces
            .get_mut(task_id)
            .ok_or_else(|| HoError::Cfg(format!("Workspace for task {} not found", task_id)))?;

        workspace.status = WorkspaceStatus::Committing;
        let branch_name = workspace.branch.clone();

        // Open the worktree repository
        let mut worktree_repo = GitRepository::open(&workspace.path)?;
        worktree_repo.set_identity(self.identity.clone());

        // Stage and commit changes
        worktree_repo.stage_all()?;
        let task_commit_hash = worktree_repo.commit(commit_message)?;

        workspace.status = WorkspaceStatus::Merging;

        // Merge task branch into main
        // First ensure we're on main branch in the main repo
        self.repo.checkout_branch("main").or_else(|_| {
            // If main doesn't exist, try master
            self.repo.checkout_branch("master")
        })?;

        // Set identity for merge commit
        let mut main_repo_with_identity = GitRepository::open(&self.base_path)?;
        main_repo_with_identity.set_identity(self.identity.clone());

        // Perform the merge
        match main_repo_with_identity.merge_branch(&branch_name)? {
            MergeResult::FastForward(hash) => {
                tracing::info!("Fast-forward merged task {} to main: {}", task_id, hash);
                workspace.status = WorkspaceStatus::Completed;

                // Cleanup the worktree and branch
                self.cleanup_workspace_and_branch(task_id, &branch_name)?;

                Ok(TaskCompleteResult::Merged {
                    task_commit: task_commit_hash,
                    merge_commit: hash,
                })
            }
            MergeResult::Merged(hash) => {
                tracing::info!("Merged task {} to main with commit: {}", task_id, hash);
                workspace.status = WorkspaceStatus::Completed;

                // Cleanup the worktree and branch
                self.cleanup_workspace_and_branch(task_id, &branch_name)?;

                Ok(TaskCompleteResult::Merged {
                    task_commit: task_commit_hash,
                    merge_commit: hash,
                })
            }
            MergeResult::UpToDate => {
                tracing::info!("Task {} branch already up-to-date with main", task_id);
                workspace.status = WorkspaceStatus::Completed;

                // Cleanup the worktree and branch
                self.cleanup_workspace_and_branch(task_id, &branch_name)?;

                Ok(TaskCompleteResult::Merged {
                    task_commit: task_commit_hash.clone(),
                    merge_commit: task_commit_hash,
                })
            }
            MergeResult::Conflict(conflicts) => {
                tracing::warn!(
                    "Merge conflict for task {} - {} files in conflict",
                    task_id,
                    conflicts.len()
                );
                workspace.status = WorkspaceStatus::Failed;

                Ok(TaskCompleteResult::Conflict {
                    task_commit: task_commit_hash,
                    conflicting_files: conflicts,
                })
            }
        }
    }

    /// Cleanup workspace and delete the task branch
    fn cleanup_workspace_and_branch(&mut self, task_id: &str, branch_name: &str) -> HoResult<()> {
        // First cleanup the worktree
        self.cleanup_workspace(task_id)?;

        // Then delete the branch (now safe since worktree is removed)
        if let Err(e) = self.repo.delete_branch(branch_name) {
            // Log but don't fail - branch cleanup is not critical
            tracing::warn!("Failed to delete branch {}: {}", branch_name, e);
        }

        Ok(())
    }

    /// Fail a task - discard changes and cleanup
    pub fn fail_task(&mut self, task_id: &str, reason: &str) -> HoResult<()> {
        let workspace = self
            .workspaces
            .get_mut(task_id)
            .ok_or_else(|| HoError::Cfg(format!("Workspace for task {} not found", task_id)))?;

        tracing::warn!("Task {} failed: {}", task_id, reason);
        workspace.status = WorkspaceStatus::Failed;

        // Cleanup the worktree
        self.cleanup_workspace(task_id)?;

        Ok(())
    }

    /// Cleanup a workspace (remove worktree and branch)
    fn cleanup_workspace(&mut self, task_id: &str) -> HoResult<()> {
        let workspace = self
            .workspaces
            .get_mut(task_id)
            .ok_or_else(|| HoError::Cfg(format!("Workspace for task {} not found", task_id)))?;

        workspace.status = WorkspaceStatus::Cleanup;

        // Remove the worktree
        self.repo.remove_worktree(&format!("task-{}", task_id))?;

        // Remove the worktree directory
        if workspace.path.exists() {
            std::fs::remove_dir_all(&workspace.path)
                .map_err(|e| HoError::Cfg(format!("Failed to remove worktree directory: {}", e)))?;
        }

        // Remove from tracking
        self.workspaces.remove(task_id);

        Ok(())
    }

    /// Cleanup stale workspaces (older than max_age)
    pub fn cleanup_stale(&mut self, max_age: std::time::Duration) -> HoResult<Vec<String>> {
        let now = Instant::now();
        let stale_tasks: Vec<String> = self
            .workspaces
            .iter()
            .filter(|(_, w)| now.duration_since(w.created_at) > max_age)
            .map(|(id, _)| id.clone())
            .collect();

        for task_id in &stale_tasks {
            tracing::warn!("Cleaning up stale workspace for task {}", task_id);
            self.cleanup_workspace(task_id)?;
        }

        Ok(stale_tasks)
    }

    /// Sync with remote (fetch and optionally push)
    pub fn sync_remote(&self, remote_name: &str) -> HoResult<()> {
        self.repo.fetch(remote_name, Some(&self.ssh_key_path))?;
        Ok(())
    }

    /// Push main branch to remote
    pub fn push_main(&self, remote_name: &str) -> HoResult<()> {
        self.repo
            .push(remote_name, "main", Some(&self.ssh_key_path))?;
        Ok(())
    }
}

/// Result of completing a task
#[derive(Debug, Clone)]
pub enum TaskCompleteResult {
    /// Task was successfully merged
    Merged {
        /// Commit hash from the task branch
        task_commit: String,
        /// Commit hash after merge (may be same as task_commit for fast-forward)
        merge_commit: String,
    },
    /// Task has conflicts that need resolution
    Conflict {
        /// Commit hash from the task branch (changes are committed but not merged)
        task_commit: String,
        /// List of files with conflicts
        conflicting_files: Vec<String>,
    },
}
