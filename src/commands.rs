// Search, fetch, and stdin summarize command flows.
// Exports: run_search(), run_fetch(), run_summarize().
// Deps: crate::{Cli, config, fetch, llm, search, stats}, reqwest blocking client.

use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::io::Read;
use std::process::Command;
use std::time::Instant;

use crate::config::Config;
use crate::fetch::fetch_pages_parallel;
use crate::llm::llm_summarize;
use crate::search::{has_gemini_cli, search_gemini, search_gemini_cli, search_web};
use crate::stats::{fmtn, record_search};
use crate::Cli;

pub fn run_search(cli: &Cli, cfg: &Config, client: &Client, query: &str) {
    let t0 = Instant::now();
    let use_cf = cli.cf && !cfg.cf_account_id.is_empty() && !cfg.cf_api_token.is_empty();
    let use_browser = cli.browser && !use_cf;

    if !cli.raw && has_gemini_cli() {
        let streaming = !cli.json;
        eprintln!("Searching via Gemini CLI: {query}");
        if let Some(result) = search_gemini_cli(query, streaming) {
            let dur = t0.elapsed().as_secs_f64();
            eprintln!("\nDone ({dur:.1}s)");
            record_search(
                query,
                "gemini-cli",
                1,
                result.raw_chars,
                result.summary_chars,
                result.usage.as_ref(),
                dur,
            );
            let tokens_saved = (result.raw_chars / 4).saturating_sub(result.summary_chars / 4);
            if cli.json {
                let json_val = json!({
                    "query": query,
                    "mode": "gemini-cli",
                    "summary": result.text,
                    "sources": 1,
                    "tokens_saved": tokens_saved,
                    "duration_secs": dur,
                });
                println!("{}", serde_json::to_string(&json_val).unwrap());
            } else if !streaming {
                println!("{}", result.text);
            }
            return;
        }
        eprintln!("[ai-summary] Gemini CLI failed, trying next provider...");
    }

    if !cfg.gemini_api_key.is_empty() && !cli.raw {
        eprintln!("Searching via Gemini API + Google Search: {query}");
        if let Some(result) = search_gemini(client, query, &cfg.gemini_api_key, &cfg.gemini_model) {
            let dur = t0.elapsed().as_secs_f64();
            eprintln!("Done ({dur:.1}s)");
            record_search(
                query,
                "gemini",
                1,
                result.raw_chars,
                result.summary_chars,
                result.usage.as_ref(),
                dur,
            );
            let tokens_saved = (result.raw_chars / 4).saturating_sub(result.summary_chars / 4);
            if cli.json {
                let json_val = json!({
                    "query": query,
                    "mode": "gemini",
                    "summary": result.text,
                    "sources": 1,
                    "tokens_saved": tokens_saved,
                    "duration_secs": dur,
                });
                println!("{}", serde_json::to_string(&json_val).unwrap());
            } else {
                println!("{}", result.text);
            }
            return;
        }
        eprintln!("[ai-summary] Gemini API failed, falling back to DDG...");
    }

    eprintln!("Searching: {query}");
    let results = search_web(client, query, 6, &cfg.brave_api_key);
    let t1 = Instant::now();
    if results.is_empty() {
        eprintln!("No search results found.");
        std::process::exit(2);
    }
    eprintln!(
        "Found {} results ({:.1}s)",
        results.len(),
        t1.duration_since(t0).as_secs_f64()
    );

    if use_cf {
        eprintln!("Fetching via Cloudflare Browser Rendering...");
    } else if use_browser {
        eprintln!("Fetching via agent-browser...");
    }
    let urls: Vec<String> = results.iter().map(|result| result.url.clone()).collect();
    let pages = fetch_pages_parallel(
        client,
        cfg,
        &urls,
        cfg.max_pages,
        cfg.max_page_chars,
        use_cf,
        use_browser,
    );
    let t2 = Instant::now();
    eprintln!(
        "Fetched {} pages ({:.1}s)",
        pages.len(),
        t2.duration_since(t1).as_secs_f64()
    );

    if cli.raw && !cli.json {
        for (index, result) in results.iter().enumerate() {
            println!(
                "\n### [{}] {}\nURL: {}\nSnippet: {}",
                index + 1,
                result.title,
                result.url,
                result.snippet
            );
            if let Some(page) = pages.iter().find(|page| page.url == result.url) {
                println!("Content: {}", &page.text[..page.text.len().min(2000)]);
            }
        }
        return;
    }

    let mut ctx = String::new();
    let mut raw_total: u64 = 0;
    for (index, result) in results.iter().enumerate() {
        ctx.push_str(&format!(
            "### Source {}: {}\nURL: {}\nSnippet: {}\n",
            index + 1,
            result.title,
            result.url,
            result.snippet
        ));
        raw_total += result.snippet.len() as u64;
        if let Some(page) = pages.iter().find(|page| page.url == result.url) {
            ctx.push_str(&format!("Content:\n{}\n\n", page.text));
            raw_total += page.text.len() as u64;
        }
    }

    let result = llm_summarize(client, cfg, query, &ctx);
    let dur = t0.elapsed().as_secs_f64();
    eprintln!("Done ({dur:.1}s total)");

    record_search(
        query,
        "search",
        results.len() as u32,
        raw_total,
        result.summary_chars,
        result.usage.as_ref(),
        dur,
    );
    let tokens_saved = (raw_total / 4).saturating_sub(result.summary_chars / 4);
    if cli.json {
        let json_val = json!({
            "query": query,
            "mode": "search",
            "summary": result.text,
            "sources": results.len(),
            "tokens_saved": tokens_saved,
            "duration_secs": dur,
        });
        println!("{}", serde_json::to_string(&json_val).unwrap());
    } else {
        println!("{}", result.text);
    }
}

pub fn run_fetch(
    cli: &Cli,
    cfg: &Config,
    client: &Client,
    urls: &[String],
    prompt: &Option<String>,
) {
    let t0 = Instant::now();
    let use_cf = cli.cf && !cfg.cf_account_id.is_empty() && !cfg.cf_api_token.is_empty();
    let use_browser = cli.browser && !use_cf;
    if use_cf {
        eprintln!(
            "Fetching {} URLs via Cloudflare Browser Rendering...",
            urls.len()
        );
    } else if use_browser {
        eprintln!("Fetching {} URLs via agent-browser...", urls.len());
    } else {
        eprintln!("Fetching {} URLs...", urls.len());
    }
    let pages = fetch_pages_parallel(
        client,
        cfg,
        urls,
        urls.len(),
        cfg.max_page_chars,
        use_cf,
        use_browser,
    );
    let t1 = Instant::now();
    eprintln!(
        "Fetched {} pages ({:.1}s)",
        pages.len(),
        t1.duration_since(t0).as_secs_f64()
    );

    if pages.is_empty() {
        eprintln!("Could not fetch any pages.");
        std::process::exit(2);
    }

    if cli.raw && !cli.json {
        for (index, page) in pages.iter().enumerate() {
            println!(
                "\n### [{}] {}\n{}",
                index + 1,
                page.url,
                &page.text[..page.text.len().min(3000)]
            );
        }
        return;
    }

    let mut ctx = String::new();
    let mut raw_total: u64 = 0;
    for (index, page) in pages.iter().enumerate() {
        ctx.push_str(&format!(
            "### Source {}: {}\n{}\n\n",
            index + 1,
            page.url,
            page.text
        ));
        raw_total += page.text.len() as u64;
    }

    let q = prompt
        .as_deref()
        .unwrap_or("Summarize the following content");
    let result = llm_summarize(client, cfg, q, &ctx);
    let dur = t0.elapsed().as_secs_f64();
    eprintln!("Done ({dur:.1}s total)");

    record_search(
        q,
        "fetch",
        pages.len() as u32,
        raw_total,
        result.summary_chars,
        result.usage.as_ref(),
        dur,
    );
    let tokens_saved = (raw_total / 4).saturating_sub(result.summary_chars / 4);
    if cli.json {
        let json_val = json!({
            "urls": urls,
            "summary": result.text,
            "pages_fetched": pages.len(),
            "tokens_saved": tokens_saved,
            "duration_secs": dur,
        });
        println!("{}", serde_json::to_string(&json_val).unwrap());
    } else {
        println!("{}", result.text);
    }
}

pub fn run_summarize(cli: &Cli, cfg: &Config, client: &Client, prompt: &str) {
    let t0 = Instant::now();
    let mut input = String::new();
    if atty::is(atty::Stream::Stdin) {
        eprintln!("Reading from stdin (Ctrl+D to end)...");
    }
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("[ai-summary] Failed to read stdin: {e}");
        std::process::exit(1);
    }

    if input.trim().is_empty() {
        eprintln!("Error: No input received on stdin.");
        std::process::exit(1);
    }

    let raw_chars = input.len() as u64;
    eprintln!("Read {} chars from stdin", fmtn(raw_chars));

    if cli.raw && !cli.json {
        println!("{input}");
        return;
    }

    let result = llm_summarize(client, cfg, prompt, &input);
    let dur = t0.elapsed().as_secs_f64();
    eprintln!("Done ({dur:.1}s total)");

    record_search(
        if prompt.is_empty() { "(stdin)" } else { prompt },
        "stdin",
        1,
        raw_chars,
        result.summary_chars,
        result.usage.as_ref(),
        dur,
    );
    let tokens_saved = (raw_chars / 4).saturating_sub(result.summary_chars / 4);
    if cli.json {
        let json_val = json!({
            "prompt": prompt,
            "summary": result.text,
            "raw_chars": raw_chars,
            "summary_chars": result.summary_chars,
            "tokens_saved": tokens_saved,
            "duration_secs": dur,
        });
        println!("{}", serde_json::to_string(&json_val).unwrap());
    } else {
        println!("{}", result.text);
    }
}

pub fn run_github(
    cli: &Cli,
    cfg: &Config,
    client: &Client,
    args: &[String],
    repo_flag: &Option<String>,
    language: &Option<String>,
    prompt: &Option<String>,
) {
    let t0 = Instant::now();

    if !Command::new("gh")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        eprintln!("[ai-summary] gh CLI not found. Install: https://cli.github.com");
        std::process::exit(1);
    }

    if args.is_empty() {
        eprintln!("Usage:");
        eprintln!("  ai-summary github \"query\" [-r owner/repo] [-l lang] [-p \"prompt\"]");
        eprintln!("  ai-summary github owner/repo [path] [-p \"prompt\"]");
        std::process::exit(1);
    }

    let is_search = repo_flag.is_some() || language.is_some();
    let repo_path = if !is_search { parse_repo_path(args) } else { None };
    let is_read = repo_path.is_some();

    let content = if let Some((ref repo, ref path)) = repo_path {
        if path.is_empty() {
            eprintln!("Browsing: {repo}");
        } else {
            eprintln!("Reading: {repo}/{path}");
        }
        gh_read(repo, path)
    } else {
        let query = args.join(" ");
        eprintln!("Searching GitHub code: {query}");
        gh_search_code(&query, repo_flag.as_deref(), language.as_deref())
    };

    let raw_chars = content.len() as u64;
    let mode_label = if is_read { "github-read" } else { "github-search" };

    if cli.raw && !cli.json {
        println!("{content}");
        return;
    }

    let default_q = if is_read {
        "Explain this code: its purpose, key functions, and how it works"
    } else {
        "Summarize these code search results: key patterns, important files, and how they work"
    };
    let q = prompt.as_deref().unwrap_or(default_q);

    let result = llm_summarize(client, cfg, q, &content);
    let dur = t0.elapsed().as_secs_f64();
    eprintln!("Done ({dur:.1}s total)");

    record_search(q, mode_label, 1, raw_chars, result.summary_chars, result.usage.as_ref(), dur);
    let tokens_saved = (raw_chars / 4).saturating_sub(result.summary_chars / 4);

    if cli.json {
        let json_val = json!({
            "mode": mode_label,
            "summary": result.text,
            "raw_chars": raw_chars,
            "tokens_saved": tokens_saved,
            "duration_secs": dur,
        });
        println!("{}", serde_json::to_string(&json_val).unwrap());
    } else {
        println!("{}", result.text);
    }
}

/// Parse args into (repo, path) if the first arg looks like owner/repo.
fn parse_repo_path(args: &[String]) -> Option<(String, String)> {
    let first = &args[0];
    let parts: Vec<&str> = first.splitn(3, '/').collect();
    if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() || first.contains(' ') {
        return None;
    }
    let valid = |s: &str| s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.');
    if !valid(parts[0]) || !valid(parts[1]) {
        return None;
    }
    let repo = format!("{}/{}", parts[0], parts[1]);
    let mut path = if parts.len() == 3 { parts[2].to_string() } else { String::new() };
    if args.len() > 1 {
        if !path.is_empty() {
            path.push('/');
        }
        path.push_str(&args[1..].join("/"));
    }
    Some((repo, path))
}

/// Search GitHub code via `gh search code`.
fn gh_search_code(query: &str, repo: Option<&str>, language: Option<&str>) -> String {
    let mut cmd = Command::new("gh");
    cmd.args(["search", "code", query, "--limit", "20", "--json", "path,repository,textMatches"]);
    if let Some(r) = repo {
        cmd.args(["--repo", r]);
    }
    if let Some(l) = language {
        cmd.args(["--language", l]);
    }
    let output = match cmd.output() {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!("[ai-summary] gh search failed: {}", String::from_utf8_lossy(&o.stderr));
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("[ai-summary] gh exec error: {e}");
            std::process::exit(1);
        }
    };

    let json_str = String::from_utf8_lossy(&output.stdout);
    let results: Vec<Value> = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return json_str.into_owned(),
    };

    if results.is_empty() {
        eprintln!("[ai-summary] No results found.");
        std::process::exit(2);
    }
    eprintln!("Found {} results", results.len());

    let mut ctx = String::new();
    for (i, entry) in results.iter().enumerate() {
        let repo_name = entry["repository"]["nameWithOwner"].as_str().unwrap_or("?");
        let path = entry["path"].as_str().unwrap_or("?");
        ctx.push_str(&format!("### [{}] {}: {}\n", i + 1, repo_name, path));
        if let Some(matches) = entry["textMatches"].as_array() {
            for m in matches {
                if let Some(frag) = m["fragment"].as_str() {
                    ctx.push_str(frag);
                    ctx.push('\n');
                }
            }
        }
        ctx.push('\n');
    }
    ctx
}

/// Read a file or directory from GitHub via `gh api`.
fn gh_read(repo: &str, path: &str) -> String {
    let endpoint = if path.is_empty() {
        format!("repos/{repo}/contents")
    } else {
        format!("repos/{repo}/contents/{path}")
    };
    let output = match Command::new("gh").args(["api", &endpoint]).output() {
        Ok(o) if o.status.success() => o,
        Ok(o) => {
            eprintln!(
                "[ai-summary] gh api failed: {}",
                String::from_utf8_lossy(&o.stderr)
            );
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("[ai-summary] gh exec error: {e}");
            std::process::exit(1);
        }
    };

    let json_str = String::from_utf8_lossy(&output.stdout);
    let value: Value = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[ai-summary] JSON parse error: {e}");
            std::process::exit(1);
        }
    };

    if value.is_array() {
        // Directory listing
        let entries = value.as_array().unwrap();
        let mut result = format!("Directory: {repo}/{path}\n\n");
        for entry in entries {
            let name = entry["name"].as_str().unwrap_or("?");
            let type_ = entry["type"].as_str().unwrap_or("?");
            let size = entry["size"].as_u64().unwrap_or(0);
            if type_ == "dir" {
                result.push_str(&format!("  {name}/\n"));
            } else {
                result.push_str(&format!("  {name}  ({size} bytes)\n"));
            }
        }
        result
    } else {
        // File: fetch raw content
        let raw_output = match Command::new("gh")
            .args(["api", &endpoint, "-H", "Accept: application/vnd.github.raw+json"])
            .output()
        {
            Ok(o) if o.status.success() => o,
            Ok(o) => {
                eprintln!(
                    "[ai-summary] gh api raw failed: {}",
                    String::from_utf8_lossy(&o.stderr)
                );
                std::process::exit(1);
            }
            Err(e) => {
                eprintln!("[ai-summary] gh exec error: {e}");
                std::process::exit(1);
            }
        };
        let file_path = value["path"].as_str().unwrap_or(path);
        let size = value["size"].as_u64().unwrap_or(0);
        let content = String::from_utf8_lossy(&raw_output.stdout);
        format!("{repo}: {file_path} ({size} bytes)\n\n{content}")
    }
}

pub fn run_repo(
    cli: &Cli,
    cfg: &Config,
    client: &Client,
    repo: &str,
    prompt: &Option<String>,
    include: Option<&str>,
) {
    let t0 = Instant::now();

    let repomix_cmd = if Command::new("repomix")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        "repomix"
    } else if Command::new("npx")
        .args(["repomix", "--version"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        "npx"
    } else {
        eprintln!("[ai-summary] repomix not found. Install: npm install -g repomix");
        std::process::exit(1);
    };

    if repomix_cmd == "npx" {
        eprintln!("Packing remote repo via npx: {repo}");
        eprintln!("(Tip: npm i -g repomix for faster startup)");
    } else {
        eprintln!("Packing remote repo: {repo}");
    }
    let mut cmd = Command::new(repomix_cmd);
    if repomix_cmd == "npx" {
        cmd.arg("repomix");
    }
    cmd.args(["--remote", repo, "--compress", "--stdout"]);
    if let Some(patterns) = include {
        cmd.args(["--include", patterns]);
    }
    cmd.stderr(std::process::Stdio::inherit());

    let output = match cmd.output() {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
        Ok(o) => {
            eprintln!("[ai-summary] repomix failed (exit {})", o.status);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("[ai-summary] repomix exec error: {e}");
            std::process::exit(1);
        }
    };

    let raw_chars = output.len() as u64;
    eprintln!("Packed {} chars ({:.1}s)", fmtn(raw_chars), t0.elapsed().as_secs_f64());

    if cli.raw && !cli.json {
        println!("{output}");
        return;
    }

    let q = prompt.as_deref().unwrap_or(
        "Summarize this repository: its purpose, architecture, key modules, and tech stack",
    );
    let result = llm_summarize(client, cfg, q, &output);
    let dur = t0.elapsed().as_secs_f64();
    eprintln!("Done ({dur:.1}s total)");

    record_search(q, "repo", 1, raw_chars, result.summary_chars, result.usage.as_ref(), dur);
    let tokens_saved = (raw_chars / 4).saturating_sub(result.summary_chars / 4);

    if cli.json {
        let json_val = json!({
            "repo": repo,
            "summary": result.text,
            "raw_chars": raw_chars,
            "tokens_saved": tokens_saved,
            "duration_secs": dur,
        });
        println!("{}", serde_json::to_string(&json_val).unwrap());
    } else {
        println!("{}", result.text);
    }
}
