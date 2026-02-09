#!/bin/bash
#
# sync-claude-agents.sh - Sync .team/agents/ to .claude/skills/ structure
#
# Creates the proper Claude CLI skill directory structure:
#   .claude/skills/<agent-name>/SKILL.md -> ../../.team/agents/<agent-name>.md
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

TEAM_AGENTS_DIR="$PROJECT_ROOT/.team/agents"
CLAUDE_SKILLS_DIR="$PROJECT_ROOT/.claude/skills"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo -e "${BLUE}Syncing .team/agents/ to .claude/skills/${NC}"

# Ensure directories exist
if [[ ! -d "$TEAM_AGENTS_DIR" ]]; then
    echo "Error: $TEAM_AGENTS_DIR not found"
    exit 1
fi

# Remove old skills symlink if it exists (from previous structure)
if [[ -L "$CLAUDE_SKILLS_DIR" ]]; then
    echo -e "${YELLOW}Removing old skills symlink${NC}"
    rm "$CLAUDE_SKILLS_DIR"
fi

# Create skills directory
mkdir -p "$CLAUDE_SKILLS_DIR"

# Clean up any orphaned directories (agents that no longer exist in .team/agents/)
for skill_dir in "$CLAUDE_SKILLS_DIR"/*; do
    if [[ -d "$skill_dir" ]]; then
        skill_name=$(basename "$skill_dir")
        if [[ ! -f "$TEAM_AGENTS_DIR/$skill_name.md" ]]; then
            echo -e "${YELLOW}Removing orphaned skill: $skill_name${NC}"
            rm -rf "$skill_dir"
        fi
    fi
done

# Sync each agent file to skills/
for agent_file in "$TEAM_AGENTS_DIR"/*.md; do
    if [[ ! -f "$agent_file" ]]; then
        continue
    fi

    agent_name=$(basename "$agent_file" .md)
    skill_dir="$CLAUDE_SKILLS_DIR/$agent_name"

    # Create skill directory
    mkdir -p "$skill_dir"

    # Create or update SKILL.md symlink
    # Use relative path for portability
    # From .claude/skills/<agent-name>/SKILL.md -> .team/agents/<agent-name>.md
    # Need to go up 3 levels: ../../../
    relative_path="../../../.team/agents/$agent_name.md"

    if [[ -L "$skill_dir/SKILL.md" ]]; then
        # Update existing symlink
        ln -sf "$relative_path" "$skill_dir/SKILL.md"
        echo -e "${GREEN}✓${NC} Updated: $agent_name/SKILL.md"
    else
        # Create new symlink
        ln -sf "$relative_path" "$skill_dir/SKILL.md"
        echo -e "${GREEN}✓${NC} Created: $agent_name/SKILL.md"
    fi
done

echo ""
echo -e "${GREEN}Sync complete!${NC} Available skills:"
ls -1 "$CLAUDE_SKILLS_DIR" | grep -v "^opencode-expert$" | sed 's/^/  - /'

echo ""
echo -e "${BLUE}Skills are now available in Claude CLI.${NC}"
