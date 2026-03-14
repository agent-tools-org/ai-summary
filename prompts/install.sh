#!/bin/bash
# Install ai-summary prompt into Claude Code global CLAUDE.md
# Usage: ./prompts/install.sh
# Uninstall: ./prompts/install.sh --uninstall

set -e

CLAUDE_MD="$HOME/.claude/CLAUDE.md"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROMPT_FILE="$SCRIPT_DIR/claude-code.md"
START_MARKER="<!-- ai-summary:start -->"
END_MARKER="<!-- ai-summary:end -->"

if [ "$1" = "--uninstall" ] || [ "$1" = "-u" ]; then
    if [ ! -f "$CLAUDE_MD" ]; then
        echo "No $CLAUDE_MD found, nothing to uninstall."
        exit 0
    fi
    if ! grep -q "$START_MARKER" "$CLAUDE_MD"; then
        echo "ai-summary prompt not found in $CLAUDE_MD, nothing to uninstall."
        exit 0
    fi
    # Remove the block between markers (inclusive)
    sed -i.bak "/$START_MARKER/,/$END_MARKER/d" "$CLAUDE_MD"
    rm -f "$CLAUDE_MD.bak"
    # Remove trailing blank lines
    sed -i.bak -e :a -e '/^\n*$/{$d;N;ba' -e '}' "$CLAUDE_MD"
    rm -f "$CLAUDE_MD.bak"
    echo "Uninstalled ai-summary prompt from $CLAUDE_MD"
    exit 0
fi

if [ ! -f "$PROMPT_FILE" ]; then
    echo "Error: prompt file not found at $PROMPT_FILE"
    exit 1
fi

mkdir -p "$(dirname "$CLAUDE_MD")"

# Remove old version if exists
if [ -f "$CLAUDE_MD" ] && grep -q "$START_MARKER" "$CLAUDE_MD"; then
    sed -i.bak "/$START_MARKER/,/$END_MARKER/d" "$CLAUDE_MD"
    rm -f "$CLAUDE_MD.bak"
    echo "Updating existing ai-summary prompt..."
fi

# Append prompt with markers
{
    [ -f "$CLAUDE_MD" ] && [ -s "$CLAUDE_MD" ] && echo ""
    echo "$START_MARKER"
    cat "$PROMPT_FILE"
    echo "$END_MARKER"
} >> "$CLAUDE_MD"

echo "Installed ai-summary prompt into $CLAUDE_MD"
echo ""
echo "Claude Code will now use ai-summary for web search and fetch."
echo "To uninstall: $0 --uninstall"
