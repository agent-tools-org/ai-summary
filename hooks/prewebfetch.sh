#!/bin/bash
# PreToolUse hook for Claude Code WebFetch: one-time education.
# First WebFetch in session → deny, tell Claude to use ai-summary instead.
# All subsequent WebFetch calls → allow through silently.

EDUCATED_FLAG="/tmp/ai-summary-educated-webfetch"

if ! command -v jq &>/dev/null || ! command -v ai-summary &>/dev/null; then
  exit 0
fi

INPUT=$(cat)
URL=$(echo "$INPUT" | jq -r '.tool_input.url // empty')

if [ -z "$URL" ]; then
  exit 0
fi

# Already educated this session → allow through
if [ -f "$EDUCATED_FLAG" ]; then
  exit 0
fi

# First WebFetch → educate and deny
touch "$EDUCATED_FLAG"

jq -n \
  --arg url "$URL" \
  '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "permissionDecision": "deny",
      "permissionDecisionReason": ("Use ai-summary instead of WebFetch. Run via Bash: ai-summary fetch \"" + $url + "\" -p \"your question\". ai-summary compresses web content, saving tokens. Use it for all web fetches in this session.")
    }
  }'
