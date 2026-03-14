# Competitive Analysis (March 2026)

## Market Position

ai-summary is a **compression proxy for AI coding agents** — it replaces 3-5 Claude Code tool calls (search + multiple fetches) with one CLI command that returns a focused summary, reducing token consumption by 10-50x.

## Tested Competitors

### 1. Jina Reader API (r.jina.ai / s.jina.ai)

**What it does:** URL → clean markdown extraction. Search API returns raw page content for multiple results.

**Tested results (docs.rs/reqwest):**
- Output: 13,629 chars raw markdown, 2.7s
- Search: 1,503,997 chars (1.5MB!) for "latest rust version" — 10 full pages dumped

**Strengths:**
- Fast content extraction (2.7s vs our 13.5s)
- Better JS rendering (cloud-based headless browser)
- Clean markdown output, supports 29 languages

**Weaknesses:**
- No summarization — raw dump only, needs post-processing
- Search output is unusable (1.5MB of raw pages)
- Requires API key for search ($)
- SaaS dependency — no offline/local option

**Verdict:** Complementary, not competitive. Jina extracts; we extract + summarize. Could potentially use Jina as a fetch backend.

### 2. Claude Code Native WebFetch

**What it does:** Built-in URL fetch with Turndown HTML→markdown and optional dynamic filtering.

**Tested results (docs.rs/reqwest):**
- Output: ~1,200 chars structured summary, ~3s
- Best output quality (organized with headers and categories)

**Strengths:**
- Fastest (built-in, no round-trip)
- Best output quality (uses Claude itself for filtering)
- 15-minute cache
- Dynamic filtering on Opus 4.6/Sonnet 4.6

**Weaknesses:**
- Every fetch costs Claude tokens ($3/M input)
- No search capability — needs separate WebSearch + multiple WebFetch calls
- A search workflow = WebSearch + 3-5 WebFetch = 5,000-50,000 tokens consumed

**Verdict:** Our primary "competitor." But each WebFetch costs tokens; our hook intercepts and compresses the response, saving those tokens. Our search command replaces the multi-step workflow entirely.

### 3. Repomix

**What it does:** Packs entire codebases into AI-friendly single files. Tree-sitter based code compression (~70% token reduction).

**Strengths:**
- Intelligent compression using AST parsing
- Token counting per file
- Security scanning (Secretlint)
- Large ecosystem (npm, 18K+ GitHub stars)

**Weaknesses:**
- Codebase-only — no web search/fetch
- Node.js dependency

**Verdict:** Different market. Repomix = codebase context, ai-summary = web content context. Complementary tools.

### 4. token-optimizer-mcp

**What it does:** MCP server for Claude Code with Brotli compression, SQLite caching, FAISS vector store. Claims 95%+ token reduction.

**Strengths:**
- MCP integration (modern standard)
- Persistent caching
- Structural code chunking and skeleton extraction
- Vector-based similarity retrieval

**Weaknesses:**
- MCP setup complexity
- Heavyweight (SQLite, FAISS, Brotli)
- General-purpose middleman, not specialized for web content

**Verdict:** Over-engineered for our use case. We solve a simpler problem more directly.

## Head-to-Head: Same Query Comparison

### Fetch: docs.rs/reqwest — "key features"

| Tool | Output (chars) | Tokens to Claude | Time | Cost |
|------|---------------|-----------------|------|------|
| Jina Reader | 13,629 | ~3,400 | 2.7s | Free* |
| Claude WebFetch | ~1,200 | ~300 (in context) | ~3s | ~$0.01 |
| **ai-summary** | **1,073** | **~268** | 13.5s | **Free** |

### Search: "what is the latest rust version"

| Tool | Output (chars) | Gives answer? | Time | Cost |
|------|---------------|--------------|------|------|
| Jina Search | 1,503,997 | No (raw pages) | 6.0s | API key |
| Claude WebSearch | ~2,000 | Snippets only | ~2s | tokens |
| **ai-summary** | **733** | **Yes: "Rust 1.94.0, March 5, 2026"** | 15.7s | **Free** |

## Our Moat

1. **E2E pipeline** — One command: search → fetch → extract → summarize. No competitor does this.
2. **Compression ratio** — 13x smaller than Jina, direct answer vs raw dump.
3. **Zero cost** — Gemini CLI + opencode = completely free.
4. **Passive savings** — PostToolUse hooks work without user intervention.
5. **Stats/ROI tracking** — Only tool that quantifies savings.

## Key Risk

Claude Code's native improvements (dynamic filtering, better context management) may reduce the value of our WebFetch/WebSearch hooks over time. The search+summarize core flow and test output compression are more defensible.
