#!/bin/bash
# PreToolUse hook for Claude Code WebSearch: one-time education.
# First WebSearch in session → deny, tell Claude to use ai-summary instead.
# All subsequent WebSearch calls → allow through silently.

EDUCATED_FLAG="/tmp/ai-summary-educated-websearch"

if ! command -v jq &>/dev/null || ! command -v ai-summary &>/dev/null; then
  exit 0
fi

INPUT=$(cat)
QUERY=$(echo "$INPUT" | jq -r '.tool_input.query // empty')

if [ -z "$QUERY" ]; then
  exit 0
fi

# Already educated this session → allow through
if [ -f "$EDUCATED_FLAG" ]; then
  exit 0
fi

# First WebSearch → educate and deny
touch "$EDUCATED_FLAG"

jq -n \
  --arg query "$QUERY" \
  '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "permissionDecision": "deny",
      "permissionDecisionReason": ("Use ai-summary instead of WebSearch. Run via Bash: ai-summary \"" + $query + "\". ai-summary compresses search results, saving tokens. Use it for all web searches in this session.")
    }
  }'
