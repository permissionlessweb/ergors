# Syncing .team/agents to Claude CLI Skills

## Overview

Claude CLI expects skills in a specific directory structure:

```
.claude/skills/
  <skill-name>/
    SKILL.md  (the actual skill content)
```

Our agents live in `.team/agents/` as `.md` files. To make them available to Claude CLI, we created an automated sync script.

## Usage

Run the sync command whenever you:

- Add a new agent to `.team/agents/`
- Modify an existing agent
- Remove an agent
- Clone the repository for the first time

```bash
just sync-agents
```

## What the Script Does

1. **Scans** `.team/agents/*.md` for all agent files
2. **Creates** a directory structure in `.claude/skills/`:

   ```
   .claude/skills/
     ergors/
       SKILL.md -> ../../../.team/agents/ergors.md
     akash/
       SKILL.md -> ../../../.team/agents/akash.md
     ...
   ```

3. **Updates** existing symlinks if they already exist
4. **Cleans up** orphaned skill directories when agents are removed from `.team/agents/`

## Verifying Sync

After running `just sync-agents`, check that skills are available:

```bash
ls .claude/skills/
```

You should see directories for each agent:

- akash
- bootstrap
- config
- ergors
- linus-torvald
- opencode-agent-creator
- provider-nerd
- script-kitty

## Troubleshooting

**Skills not showing up in Claude CLI?**

1. Verify symlinks are correct:

   ```bash
   ls -la .claude/skills/ergors/
   # Should show: SKILL.md -> ../../../.team/agents/ergors.md
   ```

2. Verify the symlink target exists:

   ```bash
   ls -la .team/agents/ergors.md
   ```

3. Re-run the sync:

   ```bash
   just sync-agents
   ```

**Need to remove an agent?**

1. Delete the agent file from `.team/agents/`
2. Run `just sync-agents` - it will automatically clean up the orphaned skill directory

## Technical Details

- **Script location**: `scripts/sync-claude-agents.sh`
- **Justfile recipe**: `just sync-agents`
- **Symlink type**: Relative paths (for portability)
- **Automatic cleanup**: Yes (orphaned directories removed automatically)
