# Team Shared Resources

This directory contains all team-shared resources for the Ergors project. It serves as the **single source of truth** for agents, skills, tools, and configurations across all AI coding assistants.

## Structure

```
.team/
├── agents/          # OpenCode agents (ergors, akash, bootstrap, config, provider-nerd, etc.)
├── skills/          # Claude skills (opencode-expert, etc.)
├── mcp/             # MCP (Model Context Protocol) servers
├── tools/           # Custom tools and scripts
└── plans/           # Planning documents and specifications
```

## Design Philosophy

### Single Source of Truth

All content lives here. The `.opencode/` and `.claude/` folders contain **symlinks** to these directories:

```
.opencode/agents  → .team/agents
.opencode/tools   → .team/tools
.opencode/plans   → .team/plans
.claude/skills    → .team/skills
.claude/mcp       → .team/mcp
```

### Benefits

1. **No Duplication**: Maintain content in one place, accessible by all CLIs
2. **Consistency**: Same agents/skills/tools for all team members and tools
3. **Easy Updates**: Update once, reflects everywhere via symlinks
4. **Organized**: Clear separation of concerns by resource type
5. **Version Control**: Single directory tree to track changes

## Directory Details

### `agents/`

OpenCode agents for specialized Ergors functionality:

- **ergors.md** - Main agent (daemon management, delegation)
- **akash.md** - Akash deployment specialist
- **bootstrap.md** - Node bootstrap and sentinel operations
- **config.md** - Configuration management
- **provider-nerd.md** - LLM provider management
- **opencode-agent-creator.md** - Meta-agent for creating new agents

### `skills/`

Claude skills for specialized knowledge domains:

- **opencode-expert/** - Comprehensive OpenCode usage guide

### `mcp/`

Model Context Protocol servers for extended capabilities:

- **ergors-server.ts** - Ergors MCP server implementation
- **package.json** - MCP server dependencies

### `tools/`

Custom tools and utility scripts:

- **ergors.ts** - Ergors tool integration
- **lib/** - Shared library code (gRPC clients, etc.)

### `plans/`

Planning documents and architectural specifications for major features.

## Usage Guidelines

### For Developers

**Always edit in `.team/`**:
```bash
# ✅ Correct: Edit in .team
code .team/agents/akash.md

# ❌ Wrong: Editing through symlink (works but confusing)
code .opencode/agents/akash.md
```

**When adding new content**:
```bash
# Add to .team, symlinks make it available everywhere
echo "---\nname: new-agent\n---" > .team/agents/new-agent.md

# Automatically accessible via symlinks
ls .opencode/agents/new-agent.md  # ✅ Works
ls .claude/agents/new-agent.md    # ✅ Works (if symlink exists)
```

### For AI Assistants

When referencing or modifying team resources:

1. **Read**: Use `.team/` paths for clarity
2. **Write**: Always write to `.team/` directly
3. **Reference**: Cite `.team/` in documentation
4. **Symlinks**: Understand they exist but prefer direct paths

Example:
```markdown
❌ "The agent is at .opencode/agents/ergors.md"
✅ "The agent is at .team/agents/ergors.md (accessible via .opencode/agents/)"
```

## Migration Notes

This structure was created on 2026-02-07 to consolidate:
- `.opencode/agents/` → `.team/agents/`
- `.agents/skills/` → `.team/skills/`
- `.claude/mcp/` → `.team/mcp/`
- `.opencode/tools/` → `.team/tools/`
- `.opencode/plans/` → `.team/plans/`

Backup folders (`.backup` suffix) preserve original content during migration.

## Adding New Resources

### New Agent

```bash
# Create in .team/agents
cat > .team/agents/new-agent.md << 'EOF'
---
name: new-agent
description: Brief description with keywords for triggering
mode: subagent
parent: ergors
---

# New Agent

Content here...
EOF

# Automatically accessible via .opencode/agents/ symlink
```

### New Skill

```bash
# Create skill directory in .team/skills
mkdir -p .team/skills/new-skill
cat > .team/skills/new-skill/SKILL.md << 'EOF'
---
name: new-skill
description: Skill description
---

# New Skill

Content here...
EOF

# Automatically accessible via .claude/skills/ symlink
```

### New Tool

```bash
# Add tool to .team/tools
cat > .team/tools/new-tool.ts << 'EOF'
// Tool implementation
EOF

# Automatically accessible via .opencode/tools/ symlink
```

## Maintenance

### Checking Symlinks

```bash
# Verify all symlinks are intact
ls -la .opencode/ | grep "^l"
ls -la .claude/ | grep "^l"
```

### Regenerating Symlinks

If symlinks break:

```bash
cd .opencode
rm agents tools plans  # Remove broken symlinks
ln -s ../.team/agents agents
ln -s ../.team/tools tools
ln -s ../.team/plans plans

cd ../.claude
rm skills mcp  # Remove broken symlinks
ln -s ../.team/skills skills
ln -s ../.team/mcp mcp
```

## Git Integration

The `.team/` folder is tracked in git:

```gitignore
# .gitignore
.team/**
!.team/README.md
!.team/agents/**
!.team/skills/**
!.team/mcp/**
!.team/tools/**
!.team/plans/**

# Exclude backups
*.backup/
```

## Questions?

For questions about this structure:
1. Check this README
2. Review CLAUDE.md for project-wide conventions
3. Ask the `opencode-agent-creator` meta-agent for guidance on creating new agents
