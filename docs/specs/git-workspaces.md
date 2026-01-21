# Git-Based Workspace Management

## Overview

ERGORS provides git-based workspace management for coordinating project files across a distributed node network. This enables:

- **Fractal Task Isolation**: Each task gets its own git worktree, enabling parallel development
- **Cryptographic Identity**: Node ED25519 keys convert to SSH format for git authentication
- **P2P Coordination**: Workspace sync messages coordinate file transfers over SSH git
- **Cnidarium Integration**: Workspace and task metadata persists in the node's state store

### Key Distinction

| Layer | Transport | Data |
|-------|-----------|------|
| **Git** | SSH | Project files agents work on |
| **Cnidarium** | Local | Task assignments, node registry, session logs |
| **P2P Channels** | Commonware | Coordination messages (sync triggers, status) |

---

## Directory Structure

```
~/.ergors/
├── data/cnidarium/           # Internal state (task metadata, node identities)
├── workspaces/               # Git-managed project workspaces
│   ├── {project-a}/          # Project repository
│   │   ├── .git/
│   │   ├── main/             # Primary worktree (default branch)
│   │   └── tasks/            # Task-specific worktrees
│   │       ├── task-{uuid}/  # Isolated task branch
│   │       └── ...
│   └── {project-b}/
│       └── ...
└── ssh/                      # Node SSH keys (derived from ED25519 identity)
    ├── id_ed25519            # Private key for git auth
    └── id_ed25519.pub        # Public key
```

---

## CLI Commands

### Workspace Management

```bash
# Register a new workspace (clone from remote)
ergors-cli workspace add my-project --remote git@github.com:org/repo.git

# Register a local workspace (no remote)
ergors-cli workspace add local-project

# List all registered workspaces
ergors-cli workspace list
ergors-cli workspace list --limit 20

# Show workspace details
ergors-cli workspace show <workspace_id>

# Remove a workspace
ergors-cli workspace remove <workspace_id>
ergors-cli workspace remove <workspace_id> --force  # Remove even with active tasks

# Sync with remote
ergors-cli workspace sync <workspace_id>
ergors-cli workspace sync <workspace_id> --push     # Push local changes
ergors-cli workspace sync <workspace_id> --remote upstream  # Specify remote
```

### Task Worktree Management

```bash
# Create a task worktree (isolated branch)
ergors-cli workspace task create <workspace_id>
ergors-cli workspace task create <workspace_id> --task-id my-task-123
ergors-cli workspace task create <workspace_id> --assign-to <node_id>

# List active task worktrees
ergors-cli workspace task list
ergors-cli workspace task list --workspace <workspace_id>
ergors-cli workspace task list --node <node_id>

# Complete a task (commit and optionally merge)
ergors-cli workspace task complete <task_id> --message "Implement feature X"
ergors-cli workspace task complete <task_id> --message "Fix bug Y" --merge

# Fail/abandon a task
ergors-cli workspace task fail <task_id> --reason "Requirements changed"
ergors-cli workspace task fail <task_id> --reason "Blocked" --cleanup
```

### JSON Output

All commands support `--json` for scripting:

```bash
ergors-cli workspace list --json | jq '.workspaces[].name'
ergors-cli workspace task list --json | jq '.worktrees[] | select(.status == "active")'
```

---

## Workflow

### Task Lifecycle

```
┌─────────────────┐
│ Workspace Added │  Clone/init project repository
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Task Created   │  Create worktree on branch task/{task_id}
└────────┬────────┘  Cnidarium: store TaskWorktree record
         │
         ▼
┌─────────────────┐
│   Task Active   │  Agent works on files in isolated worktree
└────────┬────────┘  Changes stay local to the worktree
         │
    ┌────┴────┐
    ▼         ▼
┌───────┐  ┌───────┐
│Complete│  │ Fail  │
└───┬───┘  └───┬───┘
    │          │
    ▼          ▼
┌───────────────────┐  ┌──────────────────┐
│ Commit + Merge    │  │ Discard worktree │
│ Push to remote    │  │ Update status    │
└───────────────────┘  └──────────────────┘
```

### Multi-Node Coordination

1. **Coordinator** registers workspace and assigns tasks to executors
2. **Executor** receives task, creates worktree, performs work
3. **Executor** completes task, commits, sends `PUSH_NOTIFY` over P2P
4. **Coordinator** receives notify, fetches changes via SSH git
5. **Coordinator** merges to main, sends `MERGE_COMPLETE` to network

---

## Node Identity & Git

### SSH Key Derivation

Node ED25519 keys are exported to SSH format for git authentication:

```
NodePrivKey (ED25519) ──► ~/.ergors/ssh/id_ed25519 (OpenSSH format)
```

Git commits are signed with the node key, providing cryptographic chain of custody.

### Git Author Identity

| Field | Value |
|-------|-------|
| Name | Node type (e.g., "coordinator", "executor") |
| Email | `{node_id_short}@ergors.local` |

Example commit:
```
Author: executor <a1b2c3d4@ergors.local>
Signed-by: ED25519:a1b2c3d4e5f6...
```

---

## P2P Sync Protocol

### Message Types

| Action | Direction | Purpose |
|--------|-----------|---------|
| `FETCH_REQUEST` | Executor → Coordinator | Request latest from remote |
| `PUSH_NOTIFY` | Executor → Network | Notify peers of new commits |
| `MERGE_COMPLETE` | Coordinator → Network | Task branch merged to main |
| `CONFLICT` | Any → Coordinator | Merge conflict needs resolution |

### Channel Assignment

Workspace sync messages flow over **Channel 2** in the commonware network layer.

---

## Proto Definitions

### WorkspaceMetadata

```protobuf
message WorkspaceMetadata {
    string workspace_id = 1;
    string name = 2;
    string remote_url = 3;           // Origin remote (empty if local-only)
    string local_path = 4;           // Path under ~/.ergors/workspaces/
    bytes head_commit = 5;           // Current HEAD SHA
    string default_branch = 6;       // e.g., "main"
    google.protobuf.Timestamp created_at = 7;
    google.protobuf.Timestamp last_synced = 8;
}
```

### TaskWorktree

```protobuf
message TaskWorktree {
    string task_id = 1;
    string workspace_id = 2;
    string branch = 3;               // task/{task_id}
    string worktree_path = 4;
    bytes base_commit = 5;
    TaskWorktreeStatus status = 6;
    string assigned_node_id = 7;
    google.protobuf.Timestamp created_at = 8;
}

enum TaskWorktreeStatus {
    UNSPECIFIED = 0;
    CREATING = 1;
    ACTIVE = 2;
    COMMITTING = 3;
    MERGING = 4;
    COMPLETED = 5;
    FAILED = 6;
}
```

### WorkspaceSync

```protobuf
message WorkspaceSync {
    string workspace_id = 1;
    SyncAction action = 2;
    string branch = 3;
    bytes commit_hash = 4;
    string sender_node_id = 5;
    google.protobuf.Timestamp timestamp = 6;
}

enum SyncAction {
    UNSPECIFIED = 0;
    FETCH_REQUEST = 1;
    PUSH_NOTIFY = 2;
    MERGE_COMPLETE = 3;
    CONFLICT = 4;
}
```

---

## Cnidarium Storage

### Key Prefixes

| Prefix | Value Type | Description |
|--------|------------|-------------|
| `workspaces/{id}` | WorkspaceMetadata | Workspace registry |
| `task_worktrees/{task_id}` | TaskWorktree | Task worktree records |

### Storage Methods

```rust
// Workspace operations
storage.put_workspace(&metadata).await?;
storage.get_workspace("workspace_id").await?;
storage.list_workspaces(limit).await?;
storage.delete_workspace("workspace_id").await?;

// Task worktree operations
storage.put_task_worktree(&worktree).await?;
storage.get_task_worktree("task_id").await?;
storage.list_task_worktrees(workspace_filter, node_filter).await?;
storage.delete_task_worktree("task_id").await?;
```

---

## gRPC API

### ManagementService RPCs

```protobuf
service ManagementService {
    // Workspace management
    rpc AddWorkspace(AddWorkspaceRequest) returns (AddWorkspaceResponse);
    rpc ListWorkspaces(ListWorkspacesRequest) returns (ListWorkspacesResponse);
    rpc GetWorkspace(GetWorkspaceRequest) returns (GetWorkspaceResponse);
    rpc RemoveWorkspace(RemoveWorkspaceRequest) returns (RemoveWorkspaceResponse);
    rpc SyncWorkspace(SyncWorkspaceRequest) returns (SyncWorkspaceResponse);

    // Task worktree management
    rpc CreateTaskWorktree(CreateTaskWorktreeRequest) returns (CreateTaskWorktreeResponse);
    rpc ListTaskWorktrees(ListTaskWorktreesRequest) returns (ListTaskWorktreesResponse);
    rpc CompleteTaskWorktree(CompleteTaskWorktreeRequest) returns (CompleteTaskWorktreeResponse);
    rpc FailTaskWorktree(FailTaskWorktreeRequest) returns (FailTaskWorktreeResponse);
}
```

---

## Architecture

### Module Structure

```
packages/
├── ho-std/src/git/
│   ├── mod.rs              # Module exports
│   ├── identity.rs         # ED25519 → SSH conversion
│   ├── repository.rs       # Git repo operations
│   ├── workspace.rs        # WorkspaceManager (worktree lifecycle)
│   └── registry.rs         # WorkspaceRegistry
│
├── cw-ho/src/
│   ├── git/
│   │   ├── mod.rs          # Engine-side git module
│   │   └── coordinator.rs  # P2P sync coordination
│   ├── grpc/
│   │   └── management.rs   # Workspace gRPC handlers
│   ├── storage.rs          # Cnidarium workspace methods
│   └── network/
│       └── manager.rs      # WorkspaceSync message handling
│
└── ergors-cli/src/
    ├── commands/
    │   └── workspace.rs    # CLI workspace commands
    └── client/
        └── mod.rs          # gRPC client methods
```

### Component Interaction

```
┌─────────────────────────────────────────────────────────────┐
│                        ergors-cli                           │
│  workspace add/list/sync/task create/complete/fail          │
└─────────────────────────┬───────────────────────────────────┘
                          │ gRPC
                          ▼
┌─────────────────────────────────────────────────────────────┐
│                    ManagementService                        │
│  AddWorkspace, CreateTaskWorktree, SyncWorkspace, etc.      │
└─────────────────────────┬───────────────────────────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
┌─────────────────┐ ┌───────────┐ ┌─────────────────┐
│ ErgorsStorage   │ │ Git Ops   │ │ SyncCoordinator │
│ (Cnidarium)     │ │ (libgit2) │ │ (P2P messages)  │
└─────────────────┘ └───────────┘ └─────────────────┘
```

---

## Examples

### Register and Work on a Project

```bash
# 1. Register the workspace
ergors-cli workspace add my-project --remote git@github.com:myorg/myproject.git

# 2. Create a task worktree
ergors-cli workspace task create ws_abc123 --task-id feature-auth

# 3. Work on files in the worktree
cd ~/.ergors/workspaces/my-project/tasks/task-feature-auth/
# ... make changes ...

# 4. Complete the task
ergors-cli workspace task complete feature-auth --message "Add authentication" --merge
```

### Multi-Node Task Distribution

```bash
# On coordinator node:
ergors-cli workspace add shared-repo --remote git@internal:org/shared.git
ergors-cli workspace task create ws_shared --task-id implement-api --assign-to node_executor_1

# On executor node (receives task via P2P):
# Automatically creates worktree at ~/.ergors/workspaces/shared-repo/tasks/task-implement-api/
# ... executor agent works on files ...

# Executor completes:
ergors-cli workspace task complete implement-api --message "Implement REST API"

# Coordinator receives PUSH_NOTIFY, fetches, and merges
```

### Scripting Integration

```bash
#!/bin/bash
# Check for stuck tasks older than 1 hour
stuck=$(ergors-cli workspace task list --json | jq -r '
  .worktrees[]
  | select(.status == "active")
  | select((.created_at | fromdateiso8601) < (now - 3600))
  | .task_id
')

for task in $stuck; do
  echo "Failing stuck task: $task"
  ergors-cli workspace task fail "$task" --reason "Timeout" --cleanup
done
```

---

## Dependencies

```toml
# ho-std/Cargo.toml
git2 = "0.19"           # libgit2 bindings
ssh-key = "0.6"         # ED25519 → SSH format

# ergors-cli/Cargo.toml
uuid = { version = "1.0", features = ["v4"] }
hex = "0.4"
```

---

## Troubleshooting

### SSH Key Issues

```bash
# Verify SSH key was generated
cat ~/.ergors/ssh/id_ed25519.pub

# Test SSH connection to remote
GIT_SSH_COMMAND="ssh -i ~/.ergors/ssh/id_ed25519" git ls-remote git@github.com:org/repo.git
```

### Worktree Not Created

1. Check workspace exists: `ergors-cli workspace show <id>`
2. Verify git repo is valid: `git -C ~/.ergors/workspaces/<name>/main status`
3. Check for existing task: `ergors-cli workspace task list --workspace <id>`

### Sync Fails

1. Check remote connectivity
2. Verify SSH key has push access
3. Check for conflicts: `git -C <worktree_path> status`
4. Review engine logs: `tail -f ~/.ergors/logs/engine.log | grep -i workspace`
