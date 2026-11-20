#!/bin/bash
# Advise Claude Code to use ai-summary on the first WebFetch call in a session.
# Leave later calls and requests without a session identifier to Claude Code.

command -v jq >/dev/null 2>&1 && command -v ai-summary >/dev/null 2>&1 || exit 0
input=$(cat)
session=$(printf '%s' "$input" | jq -r '.session_id // empty')
value=$(printf '%s' "$input" | jq -r '.tool_input.url // empty')
[[ "$session" =~ ^[[:alnum:]_-]{1,128}$ ]] && [ -n "$value" ] || exit 0

umask 077
state="$HOME/.ai-summary/hook-sessions"
mkdir -p "$state" || exit 0
(set -C; : > "$state/$session.webfetch") 2>/dev/null || exit 0

jq -n '{hookSpecificOutput: {
  hookEventName: "PreToolUse",
  permissionDecision: "deny",
  permissionDecisionReason: "Use ai-summary fetch with the original URL and question. The original WebFetch remains available on retry."
}}'
