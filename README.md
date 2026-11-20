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
- **Deep Search** — Run multiple lookups, extract relevant passages, and cite fetched pages by number
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
cargo install --locked --path .
```

Pre-built binaries for macOS (Apple Silicon / Intel) and Linux are available on [GitHub Releases](https://github.com/agent-tools-org/ai-summary/releases).

Requirements: a summarization backend (opencode CLI recommended — free). Rust 1.89+ if building from source.

## Quick Start

```bash
# Generate config file
ai-summary config

# Search (uses Gemini API > DDG > Brave)
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
gemini_model = "gemini-2.5-flash"

# Brave Search fallback (free: https://brave.com/search/api/)
brave_api_key = ""

max_pages = 3
max_page_chars = 4000
max_summary_tokens = 1024
summary_timeout_secs = 60
opencode_timeout_secs = 25
```

Search priority: **Gemini API** > **DuckDuckGo** > **Brave**

Environment variables: `GEMINI_API_KEY`, `BRAVE_API_KEY`, `AI_SUMMARY_API_URL`, `AI_SUMMARY_API_KEY`, `AI_SUMMARY_MODEL`.

An explicit summary API URL, key, or model selects the HTTP backend for that invocation. This applies to the matching `AI_SUMMARY_*` variables as well. A configured key stays with its original origin. Supply a key explicitly when switching to a different authenticated endpoint. The local oMLX key is discovered only for the default loopback service on port 8000.

Summary commands return exit 2 on provider failure and write diagnostics to stderr. Content JSON includes `schema_version`. Gemini grounded search uses version 2. It lists source URLs in `grounding_sources` and links each citation to source indices. When a byte range matches the answer, the citation uses the range. Otherwise it includes the cited text when available. Summary JSON contains completed summary text. Where supported, `--raw --json` returns the acquired content without a summary request. `--metadata` identifies the backend and model used for completed summaries.

The one-hour cache keys successful summaries by source content, question, and backend. It retains up to 512 entries. Source acquisition still runs to detect content changes. Authenticated custom HTTP requests and responses from fallback backends bypass summary caching. Existing cache entries are ignored by the new key format.

Multi-URL fetch runs up to four downloads at a time and returns pages in input order. JSON reports failed URLs and the overall result. Partial results exit 0. A batch with no usable pages exits 2. The HTTP path handles HTML, text, PDF, and DOCX. It retains up to 2 MiB of HTML or text and prioritizes a matching HTML section when the URL includes an anchor. An installed browser can retry short or failed pages. Jina retries require `JINA_API_KEY`. Opt in to Cloudflare with `--cf` and configured credentials.

A fetch or crawl accepts at most 128 pages. Summary context is limited to 256 KiB of UTF-8 text. Repository packing requires an installed `repomix`. GitHub and repository adapters enforce subprocess deadlines and output limits. Cloudflare crawl has a five-minute job deadline and follows result cursors. [Cloudflare Browser Run crawl reference](https://developers.cloudflare.com/browser-run/quick-actions/crawl-endpoint/).


## Claude Code Integration

### One-command setup

```bash
ai-summary init                # Install prompt injection + PreToolUse hook
ai-summary init --with-repomix # Also install repomix (for repo command)
ai-summary init --uninstall    # Remove everything
```

This installs three things:

1. **Prompt injection** into `~/.claude/CLAUDE.md` — Claude and all subagents use `ai-summary` instead of built-in WebSearch/WebFetch
2. **Bash hook** — rewrites simple test commands to run through `ai-summary wrap` under Claude Code’s normal permission checks
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

A failed command keeps its exit code, stdout, and stderr. If the output is at least 500 bytes, `wrap` also saves up to 1 MiB in `~/.ai-summary/tee/`. It retains the 20 most recent logs.

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
| `ai-summary doctor` | Check tools and summary endpoint readiness |

### Deep search

```bash
ai-summary search --deep "When is retrying an HTTP request safe?"
ai-summary search --deep --raw --json "When is retrying an HTTP request safe?"
ai-summary doctor --json
```

Deep search runs the original question, a primary-source query, and an independent-analysis query through DuckDuckGo. With a Brave key it also checks Brave. When primary-source results are missing or fewer than three sites appear, it runs a follow-up lookup. With Gemini configured, a summarized search can use Google grounding for that lookup. Other searches retry with keywords or exclude a repeated site. It prioritizes distinct sites and attempts to retrieve up to ten pages. For long direct downloads it looks for relevant passages throughout the extracted text. Synthesis uses the numbered excerpts and reports citation IDs it could not resolve. The raw form returns the evidence without invoking a model.

Deep search JSON includes the executed `queries`, ranked `sources` (including extraction method and final URL), `failed` downloads, `cited_ids`, and counts of fetched and cited sites. Source IDs start at 1. Query offsets in `lookup_ids` start at 0. The status is `partial` if fewer than three sites were fetched or cited, citations could not be resolved, or a lookup or download failed. It exits 2 if every download fails or synthesis fails, and still emits the report. Numeric citation validation checks source availability. Evaluate the specific claim against the cited page when accuracy matters.

`doctor` checks installed tools and the loopback model endpoint without generating text. Add `--check-hosted` to make a read-only OpenRouter key check. Summary generation uses `summary_timeout_secs` across its backends, with `opencode_timeout_secs` capping the OpenCode attempt. On Unix, SIGINT and SIGTERM stop active tool process groups owned by the CLI.

### Flags

| Flag | Description |
|------|-------------|
| `--deep` | Search multiple angles and check up to ten sources. On other acquisition commands, allow five pages. |
| `--raw` | Skip summarization, return raw content |
| `--json` | Structured JSON output (for scripting/piping) |
| `--metadata` | Add source URLs, timestamps, cache status, model to JSON output |
| `--no-cache` | Bypass the summary cache (force fresh LLM call) |
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
| 130 / 143 | Interrupted by SIGINT / SIGTERM |

## Statistics

Statistics writes are serialized and atomic. Existing unversioned `stats.json` files migrate on the next write. Malformed or newer files are preserved. Period reports use the 500 most recent records. Historical cost figures are estimates based on fixed rates.

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
