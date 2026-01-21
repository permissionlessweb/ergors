//! Git-based workspace management for ERGORS nodes
//!
//! This module provides git operations for managing project workspaces that agents work on.
//! It handles:
//! - SSH key generation from node ED25519 identity
//! - Git repository operations (clone, commit, push, pull)
//! - Worktree management for parallel task execution
//! - Registry for tracking workspaces and task worktrees
//!
//! The git layer works alongside Cnidarium storage:
//! - Git: Project files and workspaces agents actively develop
//! - Cnidarium: Internal node state (task metadata, session logs, node registry)

pub mod identity;
pub mod registry;
pub mod repository;
pub mod workspace;

pub use identity::GitIdentity;
pub use registry::{TaskWorktreeEntry, TaskWorktreeStatus, WorkspaceEntry, WorkspaceRegistry};
pub use repository::{ConflictStrategy, GitRepository, MergeResult};
pub use workspace::{TaskCompleteResult, Workspace, WorkspaceManager, WorkspaceStatus};
