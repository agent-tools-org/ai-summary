#!/bin/bash
# PreToolUse hook for Claude Code Bash: rewrite test commands to use ai-summary wrap.
# Unlike PostToolUse, this REPLACES the command output (real token savings).
# Install: add to ~/.claude/settings.json hooks.PreToolUse with matcher "Bash".

if ! command -v jq &>/dev/null || ! command -v ai-summary &>/dev/null; then
  exit 0
fi

INPUT=$(cat)
CMD=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

if [ -z "$CMD" ] || [[ "$CMD" == *$'\n'* ]]; then
  exit 0
fi

# Only rewrite test commands (simple ones without pipes — piped commands
# are already filtered and don't need wrapping)
case "$CMD" in
  *\|*|*\>*|*\<*|*\;*|*\&*|*\(*|*\)*|*\`*) exit 0 ;;  # Skip piped/redirected/compound commands
esac

# The command must START with a test invocation (optional VAR=val env prefixes).
# A substring match here is a footgun: a command whose ARGUMENT TEXT merely
# mentions "cargo test" (e.g. an agent prompt) must not be wrapped.
STRIPPED="$CMD"
while [[ "$STRIPPED" =~ ^[A-Za-z_][A-Za-z0-9_]*=[^[:space:]]*[[:space:]]+ ]]; do
  STRIPPED="${STRIPPED#*[[:space:]]}"
done
if [[ ! "$STRIPPED" =~ ^(cargo[[:space:]]+(test|nextest)|npm[[:space:]]+test|npx[[:space:]]+(vitest|jest)|yarn[[:space:]]+test|pytest|go[[:space:]]+test|mix[[:space:]]+test|dotnet[[:space:]]+test|make[[:space:]]+test)($|[[:space:]]) ]]; then
  exit 0
fi

# Rewrite: cargo test → ai-summary wrap cargo test
REWRITTEN="ai-summary wrap $CMD"

ORIGINAL_INPUT=$(echo "$INPUT" | jq -c '.tool_input')
UPDATED_INPUT=$(echo "$ORIGINAL_INPUT" | jq --arg cmd "$REWRITTEN" '.command = $cmd')

jq -n \
  --argjson updated "$UPDATED_INPUT" \
  '{
    "hookSpecificOutput": {
      "hookEventName": "PreToolUse",
      "updatedInput": $updated
    }
  }'
