#!/bin/bash
# PostToolUse hook for Claude Code WebFetch
# First fetch of a URL → block + provide compressed version
# Second fetch of same URL → pass through (CC decided it needs full content)
#
# Install: Add to ~/.claude/settings.json hooks.PostToolUse

MAX_CHARS=4000
MIN_SIZE=3000
SEEN_DIR="/tmp/ai-summary-seen"
mkdir -p "$SEEN_DIR"

# Read input to temp file to avoid pipe issues with large content
tmpfile=$(mktemp)
cat > "$tmpfile"

# Extract URL
url=$(jq -r '.tool_input.url // empty' < "$tmpfile")

# Extract response text (handle both string and object formats)
response_text=$(jq -r 'if (.tool_response | type) == "string" then .tool_response elif (.tool_response | type) == "object" then (.tool_response.text // .tool_response | tostring) else "" end' < "$tmpfile" 2>/dev/null)

rm -f "$tmpfile"

# No URL or small response → pass through
response_len=${#response_text}
if [ -z "$url" ] || [ "$response_len" -lt "$MIN_SIZE" ]; then
  exit 0
fi

# Hash the URL for filename
url_hash=$(echo -n "$url" | md5 -q 2>/dev/null || echo -n "$url" | md5sum | cut -d' ' -f1)
seen_file="$SEEN_DIR/$url_hash"

# Second fetch of same URL → pass through, remove marker
if [ -f "$seen_file" ]; then
  rm -f "$seen_file"
  exit 0
fi

# First fetch → compress and block
compressed=$(printf '%s' "$response_text" | ai-summary compress -m "$MAX_CHARS" --source hook-webfetch 2>/dev/null)

if [ -z "$compressed" ]; then
  exit 0
fi

# Mark URL as seen
echo "$url" > "$seen_file"

# Check if compression actually saved anything (at least 10%)
compressed_len=${#compressed}
saved_chars=$((response_len - compressed_len))
if [ "$saved_chars" -lt $((response_len / 10)) ]; then
  rm -f "$seen_file"
  exit 0
fi

# Block with compressed content
saved_tokens=$((saved_chars / 4))

jq -n \
  --arg reason "[ai-summary] Compressed ${response_len} → ${compressed_len} chars (~${saved_tokens} tokens saved). If you need the full uncompressed content, fetch this URL again.

$compressed" \
  '{
    decision: "block",
    reason: $reason
  }'
exit 0
