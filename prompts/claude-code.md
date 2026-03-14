# ai-summary: Web Search & Fetch Proxy

`ai-summary` is installed on this system. Use it **instead of** built-in WebSearch/WebFetch tools for web research — it compresses content through a local LLM, saving 60-98% of tokens.

## When to Use

- **Any web search**: use `ai-summary "<query>"` instead of WebSearch
- **Fetching URLs**: use `ai-summary fetch <url> -p "<question>"` instead of WebFetch
- **Multiple URLs**: use `ai-summary fetch <url1> <url2> -p "<question>"`
- **GitHub code search**: use `ai-summary github "<query>"` to find code on GitHub
- **GitHub file reading**: use `ai-summary github owner/repo path/to/file` to read files from repos
- **Deep research**: add `--deep` flag for more pages (5 instead of 3)

## Commands

```bash
# Search the web (replaces WebSearch)
ai-summary "firecrawl api pricing and rate limits"

# Fetch + summarize a URL (replaces WebFetch)
ai-summary fetch https://docs.example.com/api -p "what are the key endpoints"

# Fetch multiple URLs
ai-summary fetch https://url1.com https://url2.com -p "compare these"

# Search GitHub code (requires gh CLI)
ai-summary github "extensions hook" -r badlogic/pi-mono -l typescript
ai-summary github "llm provider abstraction" -l rust

# Read a file from a GitHub repo
ai-summary github owner/repo path/to/file.ts -p "how does this work"

# Browse a repo directory
ai-summary github owner/repo src/

# Summarize a remote GitHub repo (requires repomix)
ai-summary repo user/repo -p "explain the architecture"

# Deep search (more pages)
ai-summary --deep "rust async best practices 2026"

# Raw content without summarization
ai-summary --raw fetch https://example.com
```

## GitHub Code (requires `gh` CLI)

When you need to read code from a GitHub repository, **always prefer `ai-summary github`** over:
- Fetching raw.githubusercontent.com URLs (fragile, 404-prone)
- Using WebFetch on GitHub pages (noisy HTML, wastes tokens)
- Guessing file paths in remote repos

`ai-summary github` uses the GitHub API via `gh` CLI — reliable, structured, and summarized.

### When to use which GitHub command

| Need | Command |
|------|---------|
| Find code patterns across repos | `ai-summary github "query" [-l lang]` |
| Find code in a specific repo | `ai-summary github "query" -r owner/repo` |
| Read a specific file | `ai-summary github owner/repo path/to/file` |
| Browse repo structure | `ai-summary github owner/repo [dir/]` |
| Understand entire repo | `ai-summary repo owner/repo -p "question"` |

## Rules

1. **Always prefer `ai-summary` over WebSearch/WebFetch** — it returns compressed, focused content
2. **Always prefer `ai-summary github` for GitHub code** — never fetch raw.githubusercontent.com URLs directly
3. Run via Bash tool: `ai-summary "query"` — read stdout for results
4. If `ai-summary` fails (exit code non-zero), fall back to built-in tools
5. For subagents (Agent tool): instruct them to use `ai-summary` via Bash instead of WebSearch/WebFetch
6. Do NOT use WebSearch or WebFetch directly unless `ai-summary` is unavailable or fails
