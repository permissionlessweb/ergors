//! Workspace management CLI commands
//!
//! Commands for managing git workspaces and task worktrees.

use anyhow::Result;
use clap::Subcommand;

use super::CliContext;
use crate::client::ManagementClient;

/// Workspace management commands
#[derive(Subcommand)]
pub enum WorkspaceCmd {
    /// Add a new workspace (clone from remote or create local)
    Add {
        /// Workspace name
        name: String,
        /// Remote git URL (optional, creates local workspace if not provided)
        #[arg(long)]
        remote: Option<String>,
    },
    /// List registered workspaces
    List {
        /// Maximum number of workspaces to show
        #[arg(long, default_value = "50")]
        limit: u32,
    },
    /// Show workspace details
    Show {
        /// Workspace ID
        workspace_id: String,
    },
    /// Remove a workspace
    Remove {
        /// Workspace ID
        workspace_id: String,
        /// Force removal even with active worktrees
        #[arg(short, long)]
        force: bool,
    },
    /// Sync workspace with remote
    Sync {
        /// Workspace ID
        workspace_id: String,
        /// Remote name (default: origin)
        #[arg(long, default_value = "origin")]
        remote: String,
        /// Push local changes
        #[arg(long)]
        push: bool,
        /// Fetch remote changes (default: true)
        #[arg(long, default_value = "true")]
        fetch: bool,
    },
    /// Task worktree management
    #[command(subcommand)]
    Task(TaskCmd),
}

/// Task worktree commands
#[derive(Subcommand)]
pub enum TaskCmd {
    /// Create a new task worktree
    Create {
        /// Workspace ID
        workspace_id: String,
        /// Task ID (optional, generates UUID if not provided)
        #[arg(long)]
        task_id: Option<String>,
        /// Node to assign the task to
        #[arg(long)]
        assign_to: Option<String>,
    },
    /// List task worktrees
    List {
        /// Filter by workspace ID
        #[arg(long)]
        workspace: Option<String>,
        /// Filter by assigned node
        #[arg(long)]
        node: Option<String>,
    },
    /// Complete a task worktree (commit and optionally merge)
    Complete {
        /// Task ID
        task_id: String,
        /// Commit message
        #[arg(short, long)]
        message: String,
        /// Merge to main branch
        #[arg(long)]
        merge: bool,
    },
    /// Fail/abandon a task worktree
    Fail {
        /// Task ID
        task_id: String,
        /// Reason for failure
        #[arg(short, long)]
        reason: String,
        /// Cleanup the worktree
        #[arg(long)]
        cleanup: bool,
    },
}

impl WorkspaceCmd {
    pub async fn execute(&self, _ctx: &CliContext, mut _client: ManagementClient) -> Result<()> {
        // Workspace commands temporarily disabled - will simplify later
        anyhow::bail!("Workspace commands are not currently available. This functionality will be reimplemented in a future version.");
    }
}

impl TaskCmd {
    pub async fn execute(&self, _ctx: &CliContext, mut _client: ManagementClient) -> Result<()> {
        // Task commands temporarily disabled - will simplify later
        anyhow::bail!("Task commands are not currently available. This functionality will be reimplemented in a future version.");
    }
}
