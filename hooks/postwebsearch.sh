#!/bin/bash
# PostToolUse hook for Claude Code WebSearch
# Compresses long search results using ai-summary
# Install: Add to ~/.claude/settings.json hooks.PostToolUse

MAX_CHARS=4000
MIN_SIZE=3000

tmpfile=$(mktemp)
cat > "$tmpfile"

response_text=$(jq -r 'if (.tool_response | type) == "string" then .tool_response elif (.tool_response | type) == "object" then (.tool_response.text // .tool_response | tostring) else "" end' < "$tmpfile" 2>/dev/null)

rm -f "$tmpfile"

response_len=${#response_text}
if [ "$response_len" -lt "$MIN_SIZE" ]; then
  exit 0
fi

compressed=$(printf '%s' "$response_text" | ai-summary compress -m "$MAX_CHARS" --source hook-websearch 2>/dev/null)

if [ -z "$compressed" ]; then
  exit 0
fi

compressed_len=${#compressed}
saved_chars=$((response_len - compressed_len))

# Skip if compression didn't save at least 10%
if [ "$saved_chars" -lt $((response_len / 10)) ]; then
  exit 0
fi

saved_tokens=$((saved_chars / 4))

jq -n \
  --arg reason "[ai-summary] Compressed ${response_len} → ${compressed_len} chars (~${saved_tokens} tokens saved).

$compressed" \
  '{
    decision: "block",
    reason: $reason
  }'
exit 0