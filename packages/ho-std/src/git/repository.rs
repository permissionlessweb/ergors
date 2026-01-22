//! Git repository operations for ERGORS workspaces
//!
//! Provides a high-level wrapper around git2 for common operations:
//! - Repository initialization and cloning
//! - Worktree creation and management
//! - Commit and push operations

use crate::error::HoResult;
use crate::llm::HoError;
use git2::{
    build::RepoBuilder, Commit, Cred, FetchOptions, IndexAddOption, PushOptions, RemoteCallbacks,
    Repository, Signature,
};
use std::path::{Path, PathBuf};

use super::GitIdentity;

/// Git repository wrapper for ERGORS workspace operations
pub struct GitRepository {
    /// Underlying git2 repository
    repo: Repository,
    /// Path to the repository
    path: PathBuf,
    /// Git identity for commits
    identity: Option<GitIdentity>,
}

impl GitRepository {
    /// Open an existing repository
    pub fn open(path: &Path) -> HoResult<Self> {
        let repo = Repository::open(path)
            .map_err(|e| HoError::Cfg(format!("Failed to open repository at {:?}: {}", path, e)))?;

        Ok(Self {
            repo,
            path: path.to_path_buf(),
            identity: None,
        })
    }

    /// Initialize a new repository
    pub fn init(path: &Path) -> HoResult<Self> {
        std::fs::create_dir_all(path)
            .map_err(|e| HoError::Cfg(format!("Failed to create directory {:?}: {}", path, e)))?;

        let repo = Repository::init(path)
            .map_err(|e| HoError::Cfg(format!("Failed to init repository at {:?}: {}", path, e)))?;

        Ok(Self {
            repo,
            path: path.to_path_buf(),
            identity: None,
        })
    }

    /// Initialize a bare repository (for worktree-based workflow)
    pub fn init_bare(path: &Path) -> HoResult<Self> {
        std::fs::create_dir_all(path)
            .map_err(|e| HoError::Cfg(format!("Failed to create directory {:?}: {}", path, e)))?;

        let repo = Repository::init_bare(path).map_err(|e| {
            HoError::Cfg(format!(
                "Failed to init bare repository at {:?}: {}",
                path, e
            ))
        })?;

        Ok(Self {
            repo,
            path: path.to_path_buf(),
            identity: None,
        })
    }

    /// Clone a repository with SSH authentication
    pub fn clone_with_ssh(url: &str, path: &Path, ssh_key_path: Option<&Path>) -> HoResult<Self> {
        std::fs::create_dir_all(path)
            .map_err(|e| HoError::Cfg(format!("Failed to create directory {:?}: {}", path, e)))?;

        let mut callbacks = RemoteCallbacks::new();

        if let Some(key_path) = ssh_key_path {
            let key_path = key_path.to_path_buf();
            callbacks.credentials(move |_url, username_from_url, _allowed_types| {
                Cred::ssh_key(username_from_url.unwrap_or("git"), None, &key_path, None)
            });
        }

        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch_opts);

        let repo = builder
            .clone(url, path)
            .map_err(|e| HoError::Cfg(format!("Failed to clone {}: {}", url, e)))?;

        Ok(Self {
            repo,
            path: path.to_path_buf(),
            identity: None,
        })
    }

    /// Set the git identity for commits
    pub fn set_identity(&mut self, identity: GitIdentity) {
        self.identity = Some(identity);
    }

    /// Get the repository path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get current HEAD commit hash
    pub fn head_commit_hash(&self) -> HoResult<String> {
        let head = self
            .repo
            .head()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD: {}", e)))?;

        let commit = head
            .peel_to_commit()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD commit: {}", e)))?;

        Ok(commit.id().to_string())
    }

    /// Get current branch name
    pub fn current_branch(&self) -> HoResult<String> {
        let head = self
            .repo
            .head()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD: {}", e)))?;

        if head.is_branch() {
            let name = head.shorthand().unwrap_or("HEAD").to_string();
            Ok(name)
        } else {
            // Detached HEAD
            Ok("HEAD".to_string())
        }
    }

    /// Create a new branch
    pub fn create_branch(&self, name: &str) -> HoResult<()> {
        let head = self
            .repo
            .head()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD: {}", e)))?;

        let commit = head
            .peel_to_commit()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD commit: {}", e)))?;

        self.repo
            .branch(name, &commit, false)
            .map_err(|e| HoError::Cfg(format!("Failed to create branch {}: {}", name, e)))?;

        Ok(())
    }

    /// Checkout a branch
    pub fn checkout_branch(&self, name: &str) -> HoResult<()> {
        let refname = format!("refs/heads/{}", name);
        let obj = self
            .repo
            .revparse_single(&refname)
            .map_err(|e| HoError::Cfg(format!("Failed to find branch {}: {}", name, e)))?;

        self.repo
            .checkout_tree(&obj, None)
            .map_err(|e| HoError::Cfg(format!("Failed to checkout tree: {}", e)))?;

        self.repo
            .set_head(&refname)
            .map_err(|e| HoError::Cfg(format!("Failed to set HEAD: {}", e)))?;

        Ok(())
    }

    /// Stage all changes
    pub fn stage_all(&self) -> HoResult<()> {
        let mut index = self
            .repo
            .index()
            .map_err(|e| HoError::Cfg(format!("Failed to get index: {}", e)))?;

        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .map_err(|e| HoError::Cfg(format!("Failed to stage files: {}", e)))?;

        index
            .write()
            .map_err(|e| HoError::Cfg(format!("Failed to write index: {}", e)))?;

        Ok(())
    }

    /// Create a commit
    pub fn commit(&self, message: &str) -> HoResult<String> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| HoError::Cfg("Git identity not set".into()))?;

        let sig = Signature::now(&identity.git_author_name(), &identity.git_author_email())
            .map_err(|e| HoError::Cfg(format!("Failed to create signature: {}", e)))?;

        let mut index = self
            .repo
            .index()
            .map_err(|e| HoError::Cfg(format!("Failed to get index: {}", e)))?;

        let tree_id = index
            .write_tree()
            .map_err(|e| HoError::Cfg(format!("Failed to write tree: {}", e)))?;

        let tree = self
            .repo
            .find_tree(tree_id)
            .map_err(|e| HoError::Cfg(format!("Failed to find tree: {}", e)))?;

        // Get parent commit if exists
        let parent_commit: Option<Commit> = self
            .repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok());

        let parents: Vec<&Commit> = parent_commit.iter().collect();

        let commit_id = self
            .repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(|e| HoError::Cfg(format!("Failed to create commit: {}", e)))?;

        Ok(commit_id.to_string())
    }

    /// Push to remote
    pub fn push(
        &self,
        remote_name: &str,
        branch: &str,
        ssh_key_path: Option<&Path>,
    ) -> HoResult<()> {
        let mut remote = self
            .repo
            .find_remote(remote_name)
            .map_err(|e| HoError::Cfg(format!("Failed to find remote {}: {}", remote_name, e)))?;

        let mut callbacks = RemoteCallbacks::new();

        if let Some(key_path) = ssh_key_path {
            let key_path = key_path.to_path_buf();
            callbacks.credentials(move |_url, username_from_url, _allowed_types| {
                Cred::ssh_key(username_from_url.unwrap_or("git"), None, &key_path, None)
            });
        }

        let mut push_opts = PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        let refspec = format!("refs/heads/{}:refs/heads/{}", branch, branch);
        remote
            .push(&[&refspec], Some(&mut push_opts))
            .map_err(|e| HoError::Cfg(format!("Failed to push: {}", e)))?;

        Ok(())
    }

    /// Fetch from remote
    pub fn fetch(&self, remote_name: &str, ssh_key_path: Option<&Path>) -> HoResult<()> {
        let mut remote = self
            .repo
            .find_remote(remote_name)
            .map_err(|e| HoError::Cfg(format!("Failed to find remote {}: {}", remote_name, e)))?;

        let mut callbacks = RemoteCallbacks::new();

        if let Some(key_path) = ssh_key_path {
            let key_path = key_path.to_path_buf();
            callbacks.credentials(move |_url, username_from_url, _allowed_types| {
                Cred::ssh_key(username_from_url.unwrap_or("git"), None, &key_path, None)
            });
        }

        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        remote
            .fetch(&[] as &[&str], Some(&mut fetch_opts), None)
            .map_err(|e| HoError::Cfg(format!("Failed to fetch: {}", e)))?;

        Ok(())
    }

    /// Create a worktree for a task
    pub fn create_worktree(&self, name: &str, path: &Path, branch: &str) -> HoResult<()> {
        // First create the branch if it doesn't exist
        let branch_exists = self
            .repo
            .find_branch(branch, git2::BranchType::Local)
            .is_ok();

        if !branch_exists {
            self.create_branch(branch)?;
        }

        // Find the branch reference
        let reference = self
            .repo
            .find_branch(branch, git2::BranchType::Local)
            .map_err(|e| HoError::Cfg(format!("Failed to find branch {}: {}", branch, e)))?;

        // Create the worktree
        self.repo
            .worktree(
                name,
                path,
                Some(git2::WorktreeAddOptions::new().reference(Some(reference.get()))),
            )
            .map_err(|e| HoError::Cfg(format!("Failed to create worktree: {}", e)))?;

        Ok(())
    }

    /// Remove a worktree
    pub fn remove_worktree(&self, name: &str) -> HoResult<()> {
        let worktree = self
            .repo
            .find_worktree(name)
            .map_err(|e| HoError::Cfg(format!("Failed to find worktree {}: {}", name, e)))?;

        // Prune the worktree (marks it for removal)
        worktree
            .prune(Some(
                &mut git2::WorktreePruneOptions::new().working_tree(true),
            ))
            .map_err(|e| HoError::Cfg(format!("Failed to prune worktree: {}", e)))?;

        Ok(())
    }

    /// List all worktrees
    pub fn list_worktrees(&self) -> HoResult<Vec<String>> {
        let worktrees = self
            .repo
            .worktrees()
            .map_err(|e| HoError::Cfg(format!("Failed to list worktrees: {}", e)))?;

        Ok(worktrees
            .iter()
            .filter_map(|s| s.map(String::from))
            .collect())
    }

    /// Add a remote
    pub fn add_remote(&self, name: &str, url: &str) -> HoResult<()> {
        self.repo
            .remote(name, url)
            .map_err(|e| HoError::Cfg(format!("Failed to add remote {}: {}", name, e)))?;
        Ok(())
    }

    /// Get the underlying git2::Repository reference
    pub fn inner(&self) -> &Repository {
        &self.repo
    }

    /// Merge a branch into the current branch
    /// Returns the merge commit hash on success, or an error with conflict info
    pub fn merge_branch(&self, branch_name: &str) -> HoResult<MergeResult> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| HoError::Cfg("Git identity not set".into()))?;

        // Find the branch to merge
        let branch = self
            .repo
            .find_branch(branch_name, git2::BranchType::Local)
            .map_err(|e| HoError::Cfg(format!("Failed to find branch {}: {}", branch_name, e)))?;

        let branch_commit = branch
            .get()
            .peel_to_commit()
            .map_err(|e| HoError::Cfg(format!("Failed to get branch commit: {}", e)))?;

        // Get HEAD commit
        let head = self
            .repo
            .head()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD: {}", e)))?;

        let head_commit = head
            .peel_to_commit()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD commit: {}", e)))?;

        // Perform merge analysis - need to create AnnotatedCommit first
        let annotated = self
            .repo
            .find_annotated_commit(branch_commit.id())
            .map_err(|e| HoError::Cfg(format!("Failed to find annotated commit: {}", e)))?;

        let (analysis, _preference) = self
            .repo
            .merge_analysis(&[&annotated])
            .map_err(|e| HoError::Cfg(format!("Failed to analyze merge: {}", e)))?;

        if analysis.is_up_to_date() {
            // Already up to date
            return Ok(MergeResult::UpToDate);
        }

        if analysis.is_fast_forward() {
            // Fast-forward merge
            let refname = head.name().ok_or_else(|| HoError::Cfg("HEAD has no name".into()))?;
            self.repo
                .reference(
                    refname,
                    branch_commit.id(),
                    true,
                    &format!("Fast-forward merge from {}", branch_name),
                )
                .map_err(|e| HoError::Cfg(format!("Failed to update reference: {}", e)))?;

            self.repo
                .checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                .map_err(|e| HoError::Cfg(format!("Failed to checkout HEAD: {}", e)))?;

            return Ok(MergeResult::FastForward(branch_commit.id().to_string()));
        }

        // Normal merge
        let annotated = self
            .repo
            .find_annotated_commit(branch_commit.id())
            .map_err(|e| HoError::Cfg(format!("Failed to find annotated commit: {}", e)))?;

        self.repo
            .merge(&[&annotated], None, None)
            .map_err(|e| HoError::Cfg(format!("Failed to merge: {}", e)))?;

        // Check for conflicts
        let index = self
            .repo
            .index()
            .map_err(|e| HoError::Cfg(format!("Failed to get index: {}", e)))?;

        if index.has_conflicts() {
            // Collect conflict paths
            let conflicts: Vec<String> = index
                .conflicts()
                .map_err(|e| HoError::Cfg(format!("Failed to get conflicts: {}", e)))?
                .filter_map(|c| c.ok())
                .filter_map(|c| {
                    c.our.map(|e| {
                        String::from_utf8_lossy(&e.path).to_string()
                    })
                })
                .collect();

            // Abort the merge to leave repo in clean state
            self.repo
                .cleanup_state()
                .map_err(|e| HoError::Cfg(format!("Failed to cleanup state: {}", e)))?;

            return Ok(MergeResult::Conflict(conflicts));
        }

        // Write the merge commit
        let mut index = self
            .repo
            .index()
            .map_err(|e| HoError::Cfg(format!("Failed to get index: {}", e)))?;

        let tree_id = index
            .write_tree()
            .map_err(|e| HoError::Cfg(format!("Failed to write tree: {}", e)))?;

        let tree = self
            .repo
            .find_tree(tree_id)
            .map_err(|e| HoError::Cfg(format!("Failed to find tree: {}", e)))?;

        let sig = Signature::now(&identity.git_author_name(), &identity.git_author_email())
            .map_err(|e| HoError::Cfg(format!("Failed to create signature: {}", e)))?;

        let message = format!("Merge branch '{}' into {}", branch_name, self.current_branch().unwrap_or_default());

        let commit_id = self
            .repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                &message,
                &tree,
                &[&head_commit, &branch_commit],
            )
            .map_err(|e| HoError::Cfg(format!("Failed to create merge commit: {}", e)))?;

        // Cleanup merge state
        self.repo
            .cleanup_state()
            .map_err(|e| HoError::Cfg(format!("Failed to cleanup state: {}", e)))?;

        Ok(MergeResult::Merged(commit_id.to_string()))
    }

    /// Delete a local branch
    pub fn delete_branch(&self, branch_name: &str) -> HoResult<()> {
        let mut branch = self
            .repo
            .find_branch(branch_name, git2::BranchType::Local)
            .map_err(|e| HoError::Cfg(format!("Failed to find branch {}: {}", branch_name, e)))?;

        branch
            .delete()
            .map_err(|e| HoError::Cfg(format!("Failed to delete branch {}: {}", branch_name, e)))?;

        Ok(())
    }

    /// Get the commit hash for a branch
    pub fn branch_commit_hash(&self, branch_name: &str) -> HoResult<String> {
        let branch = self
            .repo
            .find_branch(branch_name, git2::BranchType::Local)
            .map_err(|e| HoError::Cfg(format!("Failed to find branch {}: {}", branch_name, e)))?;

        let commit = branch
            .get()
            .peel_to_commit()
            .map_err(|e| HoError::Cfg(format!("Failed to get branch commit: {}", e)))?;

        Ok(commit.id().to_string())
    }

    /// Reset to a specific commit (hard reset)
    pub fn reset_hard(&self, commit_hash: &str) -> HoResult<()> {
        let oid = git2::Oid::from_str(commit_hash)
            .map_err(|e| HoError::Cfg(format!("Invalid commit hash: {}", e)))?;

        let commit = self
            .repo
            .find_commit(oid)
            .map_err(|e| HoError::Cfg(format!("Failed to find commit: {}", e)))?;

        self.repo
            .reset(
                commit.as_object(),
                git2::ResetType::Hard,
                None,
            )
            .map_err(|e| HoError::Cfg(format!("Failed to reset: {}", e)))?;

        Ok(())
    }
}

/// Result of a merge operation
#[derive(Debug, Clone)]
pub enum MergeResult {
    /// Fast-forward merge, contains commit hash
    FastForward(String),
    /// Normal merge with new commit, contains commit hash
    Merged(String),
    /// Already up to date
    UpToDate,
    /// Merge has conflicts, contains list of conflicting paths
    Conflict(Vec<String>),
}

/// Strategy for resolving conflicts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    /// Keep our (current branch) changes
    Ours,
    /// Keep their (incoming) changes
    Theirs,
    /// Abort the merge
    Abort,
}

impl GitRepository {
    /// Resolve conflicts using a simple strategy (ours or theirs)
    ///
    /// This assumes the repository is in a conflicted state from a merge.
    /// For more complex conflict resolution, use the per-file methods.
    pub fn resolve_conflicts_with_strategy(
        &self,
        strategy: ConflictStrategy,
    ) -> HoResult<Option<String>> {
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| HoError::Cfg("Git identity not set".into()))?;

        // Check if we're in a merge state
        let state = self.repo.state();
        if state != git2::RepositoryState::Merge {
            return Err(HoError::Cfg("Repository is not in a merge state".into()));
        }

        // Handle abort strategy
        if strategy == ConflictStrategy::Abort {
            self.repo
                .cleanup_state()
                .map_err(|e| HoError::Cfg(format!("Failed to abort merge: {}", e)))?;
            return Ok(None);
        }

        let mut index = self
            .repo
            .index()
            .map_err(|e| HoError::Cfg(format!("Failed to get index: {}", e)))?;

        // Get conflicts
        let conflicts: Vec<git2::IndexConflict> = index
            .conflicts()
            .map_err(|e| HoError::Cfg(format!("Failed to get conflicts: {}", e)))?
            .filter_map(|c| c.ok())
            .collect();

        // Resolve each conflict
        for conflict in conflicts {
            let path = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .map(|e| String::from_utf8_lossy(&e.path).to_string())
                .unwrap_or_default();

            if path.is_empty() {
                continue;
            }

            // Choose the entry based on strategy
            let entry = match strategy {
                ConflictStrategy::Ours => conflict.our.as_ref(),
                ConflictStrategy::Theirs => conflict.their.as_ref(),
                ConflictStrategy::Abort => unreachable!(),
            };

            if let Some(entry) = entry {
                // Get the blob content
                let blob = self
                    .repo
                    .find_blob(entry.id)
                    .map_err(|e| HoError::Cfg(format!("Failed to find blob: {}", e)))?;

                // Write the content to the working directory
                let file_path = self.path.join(&path);
                std::fs::write(&file_path, blob.content())
                    .map_err(|e| HoError::Cfg(format!("Failed to write file: {}", e)))?;

                // Stage the resolved file
                index
                    .add_path(std::path::Path::new(&path))
                    .map_err(|e| HoError::Cfg(format!("Failed to stage file: {}", e)))?;
            }
        }

        // Write index to commit the staged resolutions
        index
            .write()
            .map_err(|e| HoError::Cfg(format!("Failed to write index: {}", e)))?;

        // Create the merge commit
        let tree_id = index
            .write_tree()
            .map_err(|e| HoError::Cfg(format!("Failed to write tree: {}", e)))?;

        let tree = self
            .repo
            .find_tree(tree_id)
            .map_err(|e| HoError::Cfg(format!("Failed to find tree: {}", e)))?;

        let sig = Signature::now(&identity.git_author_name(), &identity.git_author_email())
            .map_err(|e| HoError::Cfg(format!("Failed to create signature: {}", e)))?;

        // Get the merge heads
        let head = self
            .repo
            .head()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD: {}", e)))?;

        let head_commit = head
            .peel_to_commit()
            .map_err(|e| HoError::Cfg(format!("Failed to get HEAD commit: {}", e)))?;

        // Get MERGE_HEAD
        let merge_head_path = self.repo.path().join("MERGE_HEAD");
        let merge_head_content = std::fs::read_to_string(&merge_head_path)
            .map_err(|e| HoError::Cfg(format!("Failed to read MERGE_HEAD: {}", e)))?;

        let merge_head_oid = git2::Oid::from_str(merge_head_content.trim())
            .map_err(|e| HoError::Cfg(format!("Invalid MERGE_HEAD: {}", e)))?;

        let merge_commit = self
            .repo
            .find_commit(merge_head_oid)
            .map_err(|e| HoError::Cfg(format!("Failed to find merge commit: {}", e)))?;

        let strategy_str = match strategy {
            ConflictStrategy::Ours => "ours",
            ConflictStrategy::Theirs => "theirs",
            ConflictStrategy::Abort => "abort",
        };
        let message = format!("Merge conflict resolved (strategy: {})", strategy_str);

        let commit_id = self
            .repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                &message,
                &tree,
                &[&head_commit, &merge_commit],
            )
            .map_err(|e| HoError::Cfg(format!("Failed to create commit: {}", e)))?;

        // Cleanup merge state
        self.repo
            .cleanup_state()
            .map_err(|e| HoError::Cfg(format!("Failed to cleanup state: {}", e)))?;

        Ok(Some(commit_id.to_string()))
    }

    /// Check if the repository is in a conflicted merge state
    pub fn has_conflicts(&self) -> bool {
        if self.repo.state() != git2::RepositoryState::Merge {
            return false;
        }

        self.repo
            .index()
            .map(|index| index.has_conflicts())
            .unwrap_or(false)
    }

    /// Get list of conflicting files
    pub fn get_conflicting_files(&self) -> HoResult<Vec<String>> {
        let index = self
            .repo
            .index()
            .map_err(|e| HoError::Cfg(format!("Failed to get index: {}", e)))?;

        let conflicts: Vec<String> = index
            .conflicts()
            .map_err(|e| HoError::Cfg(format!("Failed to get conflicts: {}", e)))?
            .filter_map(|c| c.ok())
            .filter_map(|c| {
                c.our.map(|e| String::from_utf8_lossy(&e.path).to_string())
            })
            .collect();

        Ok(conflicts)
    }
}

impl std::fmt::Debug for GitRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitRepository")
            .field("path", &self.path)
            .field("has_identity", &self.identity.is_some())
            .finish()
    }
}
