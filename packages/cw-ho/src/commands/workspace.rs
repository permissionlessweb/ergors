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
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            WorkspaceCmd::Add { name, remote } => {
                let response = client.add_workspace(name, remote.as_deref()).await?;

                if response.success {
                    if ctx.json {
                        if let Some(ws) = &response.workspace {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "workspace_id": ws.workspace_id,
                                    "name": ws.name,
                                    "remote_url": ws.remote_url,
                                    "local_path": ws.local_path,
                                }))?
                            );
                        }
                    } else {
                        println!("Workspace added successfully!");
                        if let Some(ws) = &response.workspace {
                            println!("  ID:     {}", ws.workspace_id);
                            println!("  Name:   {}", ws.name);
                            println!("  Path:   {}", ws.local_path);
                            if !ws.remote_url.is_empty() {
                                println!("  Remote: {}", ws.remote_url);
                            }
                        }
                    }
                } else {
                    println!("Failed to add workspace: {}", response.error_message);
                }
                Ok(())
            }
            WorkspaceCmd::List { limit } => {
                let response = client.list_workspaces(*limit).await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "workspaces": response.workspaces.iter().map(|ws| {
                                serde_json::json!({
                                    "workspace_id": ws.workspace_id,
                                    "name": ws.name,
                                    "remote_url": ws.remote_url,
                                    "local_path": ws.local_path,
                                })
                            }).collect::<Vec<_>>(),
                            "total_count": response.total_count,
                        }))?
                    );
                } else {
                    println!("Workspaces ({} total)", response.total_count);
                    println!("====================");

                    if response.workspaces.is_empty() {
                        println!("No workspaces registered.");
                        println!("\nUse 'ergors workspace add <name>' to add a workspace.");
                    } else {
                        for ws in &response.workspaces {
                            let remote = if ws.remote_url.is_empty() {
                                "local"
                            } else {
                                &ws.remote_url
                            };
                            println!("  {} - {} ({})", ws.workspace_id, ws.name, remote);
                        }
                    }
                }
                Ok(())
            }
            WorkspaceCmd::Show { workspace_id } => {
                let response = client.get_workspace(workspace_id).await?;

                if let Some(ws) = &response.workspace {
                    if ctx.json {
                        let head_hex = if ws.head_commit.is_empty() {
                            None
                        } else {
                            Some(hex::encode(&ws.head_commit))
                        };
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "workspace_id": ws.workspace_id,
                                "name": ws.name,
                                "remote_url": ws.remote_url,
                                "local_path": ws.local_path,
                                "default_branch": ws.default_branch,
                                "head_commit": head_hex,
                                "active_worktrees": response.active_worktrees.len(),
                            }))?
                        );
                    } else {
                        println!("Workspace: {}", ws.name);
                        println!("=================");
                        println!("ID:            {}", ws.workspace_id);
                        println!("Path:          {}", ws.local_path);
                        println!("Default Branch: {}", ws.default_branch);

                        if !ws.remote_url.is_empty() {
                            println!("Remote URL:    {}", ws.remote_url);
                        }

                        if !ws.head_commit.is_empty() {
                            println!("HEAD:          {}", hex::encode(&ws.head_commit));
                        }

                        if !response.active_worktrees.is_empty() {
                            println!("\nActive Task Worktrees:");
                            for wt in &response.active_worktrees {
                                println!(
                                    "  {} - {} (status: {})",
                                    wt.task_id, wt.branch, wt.status
                                );
                            }
                        }
                    }
                } else {
                    println!("Workspace not found: {}", workspace_id);
                }
                Ok(())
            }
            WorkspaceCmd::Remove {
                workspace_id,
                force,
            } => {
                let result = client.remove_workspace(workspace_id, *force).await?;

                if result.success {
                    println!("Workspace removed: {}", workspace_id);
                } else {
                    println!("Failed to remove workspace: {}", result.message);
                }
                Ok(())
            }
            WorkspaceCmd::Sync {
                workspace_id,
                remote,
                push,
                fetch,
            } => {
                let response = client
                    .sync_workspace(workspace_id, remote, *push, *fetch)
                    .await?;

                if response.success {
                    if ctx.json {
                        let new_head_hex = if response.new_head_commit.is_empty() {
                            None
                        } else {
                            Some(hex::encode(&response.new_head_commit))
                        };
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "success": true,
                                "message": response.message,
                                "new_head": new_head_hex,
                            }))?
                        );
                    } else {
                        println!("Sync successful: {}", response.message);
                        if !response.new_head_commit.is_empty() {
                            println!("New HEAD: {}", hex::encode(&response.new_head_commit));
                        }
                    }
                } else {
                    println!("Sync failed: {}", response.message);
                }
                Ok(())
            }
            WorkspaceCmd::Task(task_cmd) => task_cmd.execute(ctx, client).await,
        }
    }
}

impl TaskCmd {
    pub async fn execute(&self, ctx: &CliContext, mut client: ManagementClient) -> Result<()> {
        match self {
            TaskCmd::Create {
                workspace_id,
                task_id,
                assign_to,
            } => {
                let tid = task_id
                    .clone()
                    .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                let response = client
                    .create_task_worktree(workspace_id, &tid, assign_to.as_deref())
                    .await?;

                if response.success {
                    if ctx.json {
                        if let Some(wt) = &response.worktree {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&serde_json::json!({
                                    "task_id": wt.task_id,
                                    "workspace_id": wt.workspace_id,
                                    "branch": wt.branch,
                                    "worktree_path": wt.worktree_path,
                                }))?
                            );
                        }
                    } else {
                        println!("Task worktree created!");
                        if let Some(wt) = &response.worktree {
                            println!("  Task ID:  {}", wt.task_id);
                            println!("  Branch:   {}", wt.branch);
                            println!("  Path:     {}", wt.worktree_path);
                            if !wt.assigned_node_id.is_empty() {
                                println!("  Assigned: {}", wt.assigned_node_id);
                            }
                        }
                    }
                } else {
                    println!("Failed to create task worktree: {}", response.error_message);
                }
                Ok(())
            }
            TaskCmd::List { workspace, node } => {
                let response = client
                    .list_task_worktrees(workspace.as_deref(), node.as_deref())
                    .await?;

                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "worktrees": response.worktrees.iter().map(|wt| {
                                serde_json::json!({
                                    "task_id": wt.task_id,
                                    "workspace_id": wt.workspace_id,
                                    "branch": wt.branch,
                                    "status": wt.status,
                                    "assigned_node_id": wt.assigned_node_id,
                                })
                            }).collect::<Vec<_>>(),
                        }))?
                    );
                } else {
                    println!("Task Worktrees");
                    println!("==============");

                    if response.worktrees.is_empty() {
                        println!("No active task worktrees.");
                    } else {
                        for wt in &response.worktrees {
                            let assigned = if wt.assigned_node_id.is_empty() {
                                "unassigned".to_string()
                            } else {
                                wt.assigned_node_id.clone()
                            };
                            println!(
                                "  {} | {} | {} | {}",
                                wt.task_id, wt.workspace_id, wt.branch, assigned
                            );
                        }
                    }
                }
                Ok(())
            }
            TaskCmd::Complete {
                task_id,
                message,
                merge,
            } => {
                let response = client
                    .complete_task_worktree(task_id, message, *merge)
                    .await?;

                if response.success {
                    if ctx.json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&serde_json::json!({
                                "success": true,
                                "commit_hash": response.commit_hash,
                                "merged": response.merged,
                            }))?
                        );
                    } else {
                        println!("Task completed!");
                        println!("  Commit: {}", response.commit_hash);
                        if response.merged {
                            println!("  Merged to main");
                        }
                    }
                } else {
                    println!("Failed to complete task: {}", response.error_message);
                }
                Ok(())
            }
            TaskCmd::Fail {
                task_id,
                reason,
                cleanup,
            } => {
                let result = client.fail_task_worktree(task_id, reason, *cleanup).await?;

                if result.success {
                    println!("Task marked as failed: {}", task_id);
                    if *cleanup {
                        println!("Worktree cleaned up.");
                    }
                } else {
                    println!("Failed to fail task: {}", result.message);
                }
                Ok(())
            }
        }
    }
}
