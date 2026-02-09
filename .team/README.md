# .team/ Directory Structure

This directory serves as the **single source of truth** for all team-shared resources in this project.

## Directory Layout

```
.team/
├── agents/          # Reusable AI agents (Claude Code agents, OpenCode agents)
├── skills/          # Claude Code skills (tool integrations, workflows)
├── tools/           # Custom tool definitions and scripts
├── mcp/             # MCP server configurations
└── plans/           # Implementation plans and design docs
```

## Symlinking Strategy

To ensure compatibility with both Claude CLI and OpenCode, symlinks are created in both `.claude/` and `.opencode/` directories:

```
.claude/skills/<agent-name>/SKILL.md -> ../../../.team/agents/<agent-name>.md
.claude/mcp                           -> ../.team/mcp

.opencode/agents -> ../.team/agents
.opencode/tools  -> ../.team/tools
.opencode/plans  -> ../.team/plans
```

### Setting Up Symlinks

**For Claude CLI agents** (automated via script):
```bash
just sync-agents
```

This script (`scripts/sync-claude-agents.sh`) creates the proper Claude CLI skill structure:
- Reads all `.team/agents/*.md` files
- Creates `.claude/skills/<agent-name>/SKILL.md` symlinks
- Cleans up orphaned skills when agents are removed

**For OpenCode** (manual, one-time setup):
```bash
cd .opencode
ln -sfn ../.team/agents agents
ln -sfn ../.team/tools tools
ln -sfn ../.team/plans plans
```

**For Claude MCP servers** (manual, one-time setup):
```bash
cd .claude
ln -sfn ../.team/mcp mcp
```

## Agent vs Skill

- **Agents** (`.team/agents/`) = Behavior definitions, personas, specialized workflows
- **Skills** (`.team/skills/`) = Tool-use capabilities, integrations, procedural knowledge

In Claude's architecture, both are markdown files that can be loaded as skills. The distinction is organizational - agents focus on "who" (persona/role), skills focus on "how" (procedures/tools).

## Usage

When working in this project:
- Claude CLI automatically loads agents/skills from `.claude/`
- OpenCode automatically loads agents/tools/plans from `.opencode/`
- All content is served from `.team/` via symlinks (no duplication)

Never edit files in `.claude/` or `.opencode/` directly - always edit the source files in `.team/`.
