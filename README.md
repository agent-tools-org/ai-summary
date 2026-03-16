# ai-summary

Web search & summarization CLI for AI coding agents. Reduces token consumption by compressing web content through local LLMs or Gemini before feeding it to Claude Code (or any LLM-powered tool).

## How It Works

```
┌──────────────┐     ┌──────────┐     ┌───────────┐     ┌─────────┐     ┌──────────────┐
│  Web Search  │────▶│  Fetch   │────▶│ Defuddle / │────▶│  Cache  │────▶│  LLM Summary │
│ Gemini/DDG/  │     │  Pages   │     │ Readability│     │ (1h TTL)│     │ Local/Remote │
│   Brave      │     │          │     │            │     │         │     │              │
└──────────────┘     └──────────┘     └───────────┘     └─────────┘     └──────────────┘
                                                                              │
                                                                              ▼
                                                                    ┌──────────────────┐
                                                                    │ Compressed output │
                                                                    │ (60-98% smaller) │
                                                                    └──────────────────┘
```

Instead of sending raw 50K+ page content to Claude, ai-summary returns a focused 1-4K summary — saving tokens and money.

## Features

- **Search + Summarize** — Gemini (Google Search grounding), DuckDuckGo, or Brave Search
- **Fetch + Summarize** — Fetch any URL, extract content via [defuddle](https://github.com/kepano/defuddle) (markdown, site-specific extractors) or readability fallback, summarize with LLM
- **PDF & DOCX Support** — Fetch and summarize PDF papers and Word documents directly
- **Stdin Summarize** — Pipe any text through for compression
- **Fast Compress** — No-LLM text extraction for instant compression
- **GitHub Code Search** — Search code and read files from GitHub repos via `gh` CLI + LLM summarization
- **Repo Summarize** — Pack remote GitHub repos with [repomix](https://github.com/yamadashy/repomix) and summarize via LLM
- **Test Output Compression** — `wrap` subcommand compresses passing test output (cargo test, npm test, pytest, etc.)
- **JS-heavy Pages** — agent-browser and Cloudflare Browser Rendering support
- **Pipe-friendly** — `cat urls.txt | ai-summary fetch`, `--json` output, standard exit codes
- **Claude Code Hook** — PreToolUse hook rewrites test commands for real token savings
- **Summary Cache** — 1-hour TTL, SHA256-keyed, browser-style caching with `--no-cache` bypass
- **Structured Metadata** — `--metadata` flag adds source URLs, timestamps, cache status, and model info to JSON output
- **Rich Statistics** — Time-period breakdown, ROI tracking, per-mode analysis
- **Multiple LLM Backends** — opencode (free), oMLX (local), OpenAI, Groq, DeepSeek, or any OpenAI-compatible API

## Installation

```bash
# Quick install (recommended) — downloads prebuilt binary
curl -fsSL https://ai-summary.agent-tools.org/install.sh | sh

# Or from crates.io
cargo install ai-summary

# Or build from source
git clone https://github.com/agent-tools-org/ai-summary.git
cd ai-summary
cargo install --path .
```

Pre-built binaries for macOS (Apple Silicon / Intel) and Linux are available on [GitHub Releases](https://github.com/agent-tools-org/ai-summary/releases).

Requirements: a summarization backend (opencode CLI recommended — free). Rust 1.70+ if building from source.

## Quick Start

```bash
# Generate config file
ai-summary config

# Search (uses Gemini CLI > Gemini API > DDG > Brave)
ai-summary "what is the latest Rust version"

# Fetch URLs and summarize
ai-summary fetch https://example.com/article -p "what are the key points"

# Fetch and summarize PDF/DOCX documents
ai-summary fetch https://arxiv.org/pdf/1706.03762 -p "what is this paper about"

# Fetch from stdin
cat urls.txt | ai-summary fetch -p "summarize each"

# Compress piped text (no LLM, instant)
echo "large text..." | ai-summary compress -m 4000

# Search GitHub code (requires gh CLI)
ai-summary github "error handling" -r tokio-rs/tokio -l rust

# Read a file from a GitHub repo
ai-summary github owner/repo src/main.rs -p "explain this"

# Browse a repo directory
ai-summary github owner/repo src/

# Summarize a remote GitHub repo
ai-summary repo user/repo -p "explain the architecture"

# Wrap test commands (compress passing output)
ai-summary wrap cargo test

# JSON output (for scripting)
ai-summary --json "query" | jq '.summary'

# Check token savings
ai-summary stats
```

## Configuration

Config file: `~/.ai-summary/config.toml` (auto-created with `ai-summary config`)

```toml
# LLM backend — local oMLX (recommended for Apple Silicon)
api_url = "http://127.0.0.1:8000"
api_key = ""  # Leave empty for oMLX auto-detection
model = "Qwen3.5-9B-MLX-4bit"

# Search provider — Gemini + Google Search (recommended)
gemini_api_key = ""  # Free: https://aistudio.google.com/apikey
gemini_model = "gemini-2.0-flash"

# Brave Search fallback (free: https://brave.com/search/api/)
brave_api_key = ""

max_pages = 3
max_page_chars = 4000
max_summary_tokens = 1024
```

Search priority: **Gemini CLI** > **Gemini API** > **DuckDuckGo** > **Brave**

Environment variables: `GEMINI_API_KEY`, `BRAVE_API_KEY`, `AI_SUMMARY_API_URL`, `AI_SUMMARY_API_KEY`, `AI_SUMMARY_MODEL`.

## Claude Code Integration

### One-command setup

```bash
ai-summary init                # Install prompt injection + PreToolUse hook
ai-summary init --with-repomix # Also install repomix (for repo command)
ai-summary init --uninstall    # Remove everything
```

This installs three things:

1. **Prompt injection** into `~/.claude/CLAUDE.md` — Claude and all subagents use `ai-summary` instead of built-in WebSearch/WebFetch
2. **Bash hook** — rewrites test commands to run through `ai-summary wrap` for real token savings
3. **WebFetch/WebSearch hooks** — on first use per session, denies and reminds Claude to use `ai-summary`; subsequent calls pass through silently

```
Without hook:                          With hook:

Claude ──cargo test──▶ shell ──▶ cargo  Claude ──cargo test──▶ hook ──▶ ai-summary wrap
  ▲                              │        ▲                           │
  │     ~3000 tokens (raw)       │        │     ~15 tokens            │  run + filter
  └──────────────────────────────┘        └───────────────────────────┘
```

Supported test commands: `cargo test`, `cargo nextest`, `npm test`, `npx vitest`, `npx jest`, `yarn test`, `pytest`, `go test`, `mix test`, `dotnet test`, `make test`.

### Tee mode

When a wrapped command fails, the full raw output is saved to `/tmp/ai-summary-tee/` so the AI can read it if the summary isn't enough:

```
TESTS FAILED: 9 passed, 1 failed, 0 ignored.
test bar ... FAILED
[ai-summary] Full output saved to: /tmp/ai-summary-tee/1710000000_cargo_test.log
```

Requires `jq` and `ai-summary` in PATH.

## Subcommands

| Command | Description |
|---------|-------------|
| `ai-summary <query>` | Search the web and summarize results |
| `ai-summary fetch <urls> -p <prompt>` | Fetch URLs and summarize |
| `ai-summary sum <prompt>` | Summarize stdin text via LLM |
| `ai-summary compress -m <chars>` | Fast text compression (no LLM) |
| `ai-summary wrap <command>` | Run command, compress passing test output |
| `ai-summary github <query> [-r repo] [-l lang]` | Search GitHub code via `gh` CLI |
| `ai-summary github <owner/repo> [path]` | Read file or browse directory from GitHub repo |
| `ai-summary repo <owner/repo> -p <prompt>` | Pack remote repo with repomix and summarize |
| `ai-summary crawl <url> -p <prompt>` | Crawl website via Cloudflare Browser Rendering |
| `ai-summary init` | Install Claude Code integration (prompt + hook) |
| `ai-summary stats` | Show token savings statistics |
| `ai-summary reset-stats` | Reset statistics |
| `ai-summary reset-cache` | Clear the summary cache |
| `ai-summary config` | Show or create config file |

### Flags

| Flag | Description |
|------|-------------|
| `--deep` | Fetch more pages (5 instead of 3) |
| `--raw` | Skip summarization, return raw content |
| `--json` | Structured JSON output (for scripting/piping) |
| `--metadata` | Add source URLs, timestamps, cache status, model to JSON output |
| `--no-cache` | Bypass the summary cache (force fresh LLM call) |
| `--doc` | Doc-aware mode (reserved for future) |
| `--browser` | Use agent-browser for JS-heavy pages |
| `--cf` | Use Cloudflare Browser Rendering |
| `--api-url` | Override API endpoint |
| `--api-key` | Override API key |
| `--model` | Override model name |

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | User error (bad args, no input) |
| 2 | API/network error (no results, fetch failed) |

## Statistics

```
ai-summary Token Savings
════════════════════════════════════════════════════════════

Metric               Today   7 days  30 days   All Time
────────────────────────────────────────────────────────────
Queries                  8       17       17         21
Pages fetched            8       17       17         17
Tokens saved         14.4K    29.2K    29.2K      31.3K
Cost saved           $0.04    $0.09    $0.09      $0.09
Compression            84%      84%      84%        76%
────────────────────────────────────────────────────────────

ROI: $0.011 LLM cost -> $0.09 Claude cost saved (9x return)
```

## License

MIT
