//! Orchestrate grounded search and web search with fetched context.
//! Grounded answers keep their distinct source and usage contract.

use super::{output, summary};
use crate::{
    cache,
    config::Config,
    fetch::fetch_pages_parallel,
    search::{search_gemini, search_web, GroundedAnswer},
    stats::record_search,
    types::{FetchedPage, SearchResult},
    Cli,
};
use reqwest::blocking::Client;
use serde_json::json;
use std::time::Instant;

pub fn run_search(cli: &Cli, cfg: &Config, client: &Client, query: &str) {
    if query.len() > 8192 {
        super::fail(1, "search query exceeds 8 KiB");
    }
    if cli.deep {
        return super::deep::run(cli, cfg, client, query);
    }
    let started = Instant::now();
    if !cfg.gemini_api_key.is_empty() && !cli.raw {
        match search_gemini(query, &cfg.gemini_api_key, &cfg.gemini_model) {
            Ok(answer) => return emit_grounded(cli, query, answer, started),
            Err(reason) => {
                eprintln!("[ai-summary] Gemini API failed ({reason}), falling back to DDG...")
            }
        }
    }
    eprintln!("Searching: {query}");
    let results = search_web(client, query, 6, &cfg.brave_api_key).unwrap_or_else(|reason| {
        eprintln!("No search results found: {reason}");
        std::process::exit(2);
    });
    let urls: Vec<String> = results.iter().map(|result| result.url.clone()).collect();
    let (cf, browser) = super::renderers(cli, cfg);
    let batch = fetch_pages_parallel(client, cfg, &urls, cfg.max_pages, cf, browser);
    let pages = &batch.pages;
    if cli.raw {
        return emit_raw(cli, query, &results, &batch);
    }
    let timestamp = cache::current_timestamp();
    let (content, raw_bytes) = search_context(&results, pages);
    let (answer, cached) = summary::summarize(cli, cfg, "search", query, &content, true);
    let (tokens, duration) = summary::record(
        &answer,
        query,
        "search",
        pages.len() as u32,
        raw_bytes,
        started,
        cached,
    );
    let mut fields = json!({
        "query": query, "mode": "search", "sources": results.len(),
        "status": if batch.failed.is_empty() { "ok" } else { "partial" },
        "tokens_saved": tokens, "duration_secs": duration, "failed": batch.failed,
    });
    if cli.metadata {
        fields["source_urls"] = json!(urls);
        fields["fetch_timestamp"] = json!(timestamp);
        fields["cached"] = json!(cached);
    }
    output::emit(cli, &answer, raw_bytes, fields);
}

fn search_context(results: &[SearchResult], pages: &[FetchedPage]) -> (String, u64) {
    let mut content = String::new();
    let mut raw_bytes = 0;
    for (index, result) in results.iter().enumerate() {
        content.push_str(&format!(
            "### Source {}: {}\nURL: {}\nSnippet: {}\n",
            index + 1,
            result.title,
            result.url,
            result.snippet
        ));
        raw_bytes += result.snippet.len() as u64;
        if let Some(page) = pages.iter().find(|page| page.url == result.url) {
            content.push_str(&format!("Content:\n{}\n\n", page.text));
            raw_bytes += page.text.len() as u64;
        }
    }
    (content, raw_bytes)
}

fn emit_raw(cli: &Cli, query: &str, results: &[SearchResult], batch: &crate::fetch::FetchBatch) {
    let pages = &batch.pages;
    if output::structured(cli) {
        let sources: Vec<_> = results.iter().map(|source| {
            let text = pages.iter().find(|page| page.url == source.url).map(|page| &page.text);
            json!({"url": source.url, "title": source.title, "snippet": source.snippet, "text": text})
        }).collect();
        println!(
            "{}",
            json!({"schema_version": 1, "query": query, "sources": sources, "failed": batch.failed})
        );
        return;
    }
    for (index, result) in results.iter().enumerate() {
        println!(
            "\n### [{}] {}\nURL: {}\nSnippet: {}",
            index + 1,
            result.title,
            result.url,
            result.snippet
        );
        if let Some(page) = pages.iter().find(|page| page.url == result.url) {
            println!(
                "Content: {}",
                page.text.chars().take(2000).collect::<String>()
            );
        }
    }
}

fn emit_grounded(cli: &Cli, query: &str, answer: GroundedAnswer, started: Instant) {
    let duration = started.elapsed().as_secs_f64();
    let bytes = answer.text.len() as u64;
    // Grounded search does not report source input bytes. Account for the
    // request and usage without inventing savings.
    record_search(
        query,
        "gemini",
        answer.sources.len() as u32,
        bytes,
        bytes,
        answer.usage.as_ref(),
        duration,
    );
    if output::structured(cli) {
        println!(
            "{}",
            json!({
                "schema_version": 2, "query":query, "mode":"gemini", "summary":answer.text,
                "sources":answer.sources.len(), "citations":answer.citations,
                "grounding_sources":answer.sources, "tokens_saved":null, "duration_secs":duration,
                "provider":"gemini", "model":answer.model, "cached":false,
            })
        );
    } else {
        println!(
            "{}

Sources:",
            answer.text
        );
        for source in answer.sources {
            println!("- {}: {}", source.title, source.url);
        }
    }
}
