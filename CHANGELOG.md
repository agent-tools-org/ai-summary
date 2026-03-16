# Changelog

## v2.11.0 (2026-03-16)

### Added
- Optional [defuddle](https://github.com/kepano/defuddle) content extraction backend — produces clean markdown output with site-specific extractors for YouTube, GitHub, Reddit, Twitter, HackerNews, and more
- Auto-detection: uses global `defuddle` CLI first, then `npx defuddle`, falls back to readability if neither available
- Install with `npm install -g defuddle` for best extraction quality (no config needed)

## v2.10.0 (2026-03-15)

### Added
- PDF document support — `ai-summary fetch <url>.pdf` extracts text via `pdf-extract` and summarizes
- DOCX document support — `ai-summary fetch <url>.docx` extracts text from Word documents via ZIP/XML parsing
- Content-type detection (`application/pdf`, `wordprocessingml.document`) with URL extension fallback
- Binary document download limit: 10MB (vs 200KB for HTML)

## v2.6.0 (2026-03-14)

### Added
- `ai-summary github` (alias: `gh`) command — search GitHub code and read files from repos via `gh` CLI
  - Code search: `ai-summary github "query" [-r owner/repo] [-l lang]`
  - File read: `ai-summary github owner/repo path/to/file`
  - Directory browse: `ai-summary github owner/repo [dir/]`
- Smart mode detection: auto-detects `owner/repo` patterns vs search queries
- Combined path format: `ai-summary github owner/repo/path/to/file` works as a single arg
- Updated Claude Code prompt to guide AI agents to prefer `ai-summary github` over raw.githubusercontent.com URLs

## v2.5.0 (2026-03-14)

### Added
- `ai-summary repo <owner/repo>` command — pack remote GitHub repos with repomix and summarize via LLM
- `--include` flag for repo command to filter files by glob patterns (e.g. `--include "src/**/*.rs"`)
- `ai-summary init --with-repomix` optional dependency installation
- `ai-summary init` now shows optional dependency status (repomix detection)
- npx fallback: `repo` command works without global repomix install if Node.js is available

## v2.4.0 (2026-03-14)

### Added
- Streaming output for Gemini CLI search — results appear in real-time via `stream-json` instead of waiting for full response
- WebFetch/WebSearch education hooks — on first use per session, deny with message to use `ai-summary` instead; all subsequent calls pass through silently
- `ai-summary init` now installs all three PreToolUse hooks (Bash, WebFetch, WebSearch)

### Changed
- Simplified WebFetch/WebSearch hooks from complex intercept-and-replace to lightweight one-time education pattern
- Removed async background processing, URL rewriting, seen-file tracking, and concurrency guards from web hooks

## v2.3.1 (2026-03-14)

### Changed
- `init` now embeds hook script in the binary (`include_str!`) — zero external file dependencies
- Hook script written to `~/.claude/hooks/ai-summary-prebash.sh` on init, removed on uninstall
- No longer requires hooks/ directory to exist next to the binary

## v2.3.0 (2026-03-14)

### Added
- `ai-summary init` command — one-command install of Claude Code integration (prompt injection + PreToolUse hook)
- `ai-summary init --uninstall` to cleanly remove the integration
- Tee mode for `wrap` command — when wrapped commands fail, raw output is saved to `/tmp/ai-summary-tee/` so AI can read the full log if needed
- Auto-cleanup keeps only 20 most recent tee files

## v2.2.0 (2026-03-14)

### Added
- Claude Code prompt injection — injects `ai-summary` instructions into global `CLAUDE.md` so Claude and all subagents use `ai-summary` instead of WebSearch/WebFetch
- `prompts/install.sh` for one-command install/uninstall of prompt injection

## v2.1.1 (2026-03-14)

### Fixed
- Hook temp file paths now human-readable (`/tmp/ai-summary-prebash-*.txt` instead of random hashes)
- Concurrency guard for hooks prevents race conditions when multiple hooks fire simultaneously

## v2.1.0 (2026-03-14)

### Added
- Jina Reader fallback works without API key (free tier via `r.jina.ai`)
- `bench` subcommand for running standardized benchmarks
- Hook results written to temp files instead of inline deny reason (better Claude Code compatibility)

## v2.0.0 (2026-03-14)

### Added
- PreToolUse hooks for Bash — rewrites test commands to run through `ai-summary wrap`, so Claude only sees compressed output (real token savings vs PostToolUse append)
- `wrap` subcommand — run a command and compress passing test output

### Fixed
- Gemini CLI search: changed prompt to trigger Google Search grounding
- `$?` exit code no longer clobbered by `-z` check in PreToolUse hooks

### Changed
- Major version bump: hook architecture shifted from PostToolUse (append) to PreToolUse (rewrite)

## v1.2.1 (2026-03-14)

### Added
- Unit tests for compress, config, and search modules (13 → 20 tests)
- Benchmark results on project website

### Changed
- Split `fetch.rs` into `fetch.rs` + `fetch_utils.rs` (407 → 299+114 lines)
- Split `stats.rs` into `stats.rs` + `stats_history.rs` (382 → 296+87 lines)
- All source files now under 300-line limit

## v1.2.0 (2026-03-14)

### Added
- Jina Reader API as fetch fallback (`JINA_API_KEY` env var or config)
- When HTTP fetch fails and agent-browser unavailable, retries via Jina Reader
- Jina API key in config template

### Changed
- GitHub repo renamed from `websummary` to `ai-summary`

## v1.1.0 (2026-03-14)

### Added
- GitHub Actions CI (clippy + test + build on every push/PR)
- GitHub Actions release workflow (cross-platform binaries on tag push)
- `cargo install ai-summary` support in README

### Fixed
- Repository URL in Cargo.toml now matches GitHub
- Bash hook: all-pass test output is now a single line instead of dumping every suite

### Changed
- README installation section now includes cargo install and GitHub Releases link

## v1.0.1 (2026-03-14)

### Fixed
- `compress` command no longer inflates `total_searches` counter in stats
- Stats accounting consolidated: `record_compress()` now updates both history and aggregates
- WebSearch hook: skip blocking when compression saves less than 10%
- WebFetch hook: same 10% minimum savings check
- Fetch redirect depth capped at 3 to prevent infinite recursion
- Fetch stats now use actual page content size instead of LLM context string size

### Added
- `crawl` command now supports `--json` output (was the only command missing it)
- Bash hook now extracts structured test totals instead of brute-force compression

## v1.0.0 (2026-03-14)

### Added
- MIT LICENSE file
- Full Cargo.toml metadata (authors, keywords, categories, repository, readme)

### Changed
- All source files formatted with `cargo fmt`
- `search.rs`: test module moved to end of file, blank lines restored between functions
- `fetch.rs`: clippy warning fixed (char comparison pattern)

## v0.9.1 (2026-03-14)

### Fixed
- ROI calculation changed from token-count comparison to cost-based comparison ($0.10/M LLM vs $3/M Claude)

## v0.9.0 (2026-03-14)

### Added
- `--json` flag for structured JSON output on all commands
- Stdin URL reading for `fetch` command (`cat urls.txt | ai-summary fetch`)
- Standard exit codes (0=success, 1=user error, 2=API/network error)
- PostToolUse hooks: `postwebfetch.sh`, `postwebsearch.sh`, `postbash.sh`
- Rich statistics with time-period breakdown (today/7d/30d/all-time)
- ROI tracking and per-mode analysis in stats
- `compress --source` flag for hook stats tracking
- `record_compress()` for tracking hook compression in history

### Changed
- Simplified bash hook to only handle passing tests (PostToolUse only fires on exit 0)
- Updated README with full hook documentation

## v0.3.0 and earlier

- Initial implementation: search, fetch, summarize, compress, crawl, stats
- Multiple search backends: Gemini CLI, Gemini API, DuckDuckGo, Brave
- Multiple LLM backends: opencode (free), oMLX (local), OpenAI, Groq, DeepSeek
- agent-browser and Cloudflare Browser Rendering support
- opencode CLI auto-detection for free summarization
