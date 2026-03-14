#!/bin/bash
# PostToolUse hook for Claude Code Bash: summarize large passing test output.
# Note: Claude Code only calls PostToolUse for successful commands (exit 0),
# so this hook only sees passing test runs, not failures.
# Install: add to ~/.claude/settings.json hooks.PostToolUse with matcher "Bash".

MIN_SIZE=3000

# Require jq
command -v jq >/dev/null 2>&1 || exit 0

# Read hook input
tmpfile=$(mktemp 2>/dev/null) || exit 0
cat >"$tmpfile" 2>/dev/null

# Extract command — quick exit if not a test command
cmd=$(jq -r '.tool_input.command // ""' <"$tmpfile" 2>/dev/null) || { rm -f "$tmpfile"; exit 0; }
case "$cmd" in
  *"cargo test"*|*"cargo nextest"*|*"npm test"*|*"npx vitest"*|*"npx jest"*) ;;
  *"yarn test"*|*pytest*|*"go test"*|*"mix test"*|*"dotnet test"*|*"make test"*) ;;
  *) rm -f "$tmpfile" 2>/dev/null; exit 0 ;;
esac

# Extract output
output=$(jq -r '
  if (.tool_response | type) == "string" then .tool_response
  else ([.tool_response.stdout?, .tool_response.stderr?] | map(select(. != null and . != "")) | join("\n"))
  end' <"$tmpfile" 2>/dev/null) || output=""
rm -f "$tmpfile" 2>/dev/null

# Too short → pass through
if [ ${#output} -lt $MIN_SIZE ]; then exit 0; fi

# Count totals from "test result:" lines (macOS-compatible)
total_passed=$(printf '%s\n' "$output" | grep -o '[0-9]* passed' | awk '{s+=$1} END {print s+0}')
total_failed=$(printf '%s\n' "$output" | grep -o '[0-9]* failed' | awk '{s+=$1} END {print s+0}')
total_ignored=$(printf '%s\n' "$output" | grep -o '[0-9]* ignored' | awk '{s+=$1} END {print s+0}')

header="All tests passed: ${total_passed} passed, ${total_failed} failed, ${total_ignored} ignored."

# If there are failures, include the failed test names and error context
failed_tests=$(printf '%s\n' "$output" | grep -E '^test .+ FAILED' || true)
errors=$(printf '%s\n' "$output" | grep -E '(error\[|panicked at)' | head -20 || true)

if [ -n "$failed_tests" ]; then
  header="TESTS FAILED: ${total_passed} passed, ${total_failed} failed, ${total_ignored} ignored."
  formatted="${header}

${failed_tests}
${errors}"
else
  # All passed — just the one-line summary is enough
  formatted="${header}"
fi

saved_chars=$(( ${#output} - ${#formatted} ))
saved_tokens=$(( saved_chars / 4 ))

# Track stats
if command -v ai-summary >/dev/null 2>&1; then
  printf '%s' "$output" | ai-summary compress -m 99999 --source hook-bash >/dev/null 2>&1 || true
fi

jq -n \
  --arg reason "[ai-summary] ${formatted} (${#output} → ${#formatted} chars, ~${saved_tokens} tokens saved)" \
  '{decision:"block",reason:$reason}'
