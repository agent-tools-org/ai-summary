//! Fetch and render source pages or summarize their extracted content.
//! Raw output bypasses the summary and cache layers.

use super::{output, summary};
use crate::{cache, config::Config, fetch::fetch_pages_parallel, fetch::FetchBatch, Cli};
use reqwest::blocking::Client;
use serde_json::json;
use std::time::Instant;

pub fn run_fetch(cli: &Cli, cfg: &Config, client: &Client, urls: &[String], prompt: Option<&str>) {
    let started = Instant::now();
    let (use_cf, use_browser) = super::renderers(cli, cfg);
    eprintln!("Fetching {} URLs...", urls.len());
    let batch = fetch_pages_parallel(client, cfg, urls, urls.len(), use_cf, use_browser);
    let pages = &batch.pages;
    if pages.is_empty() {
        if output::structured(cli) {
            emit_raw(cli, urls, &batch);
        }
        eprintln!("Could not fetch any pages.");
        std::process::exit(2);
    }
    for failure in &batch.failed {
        eprintln!("Fetch failed for {}: {}", failure.url, failure.reason);
    }
    if cli.raw {
        return emit_raw(cli, urls, &batch);
    }
    let timestamp = cache::current_timestamp();
    let mut content = String::new();
    for (index, page) in pages.iter().enumerate() {
        content.push_str(&format!(
            "### Source {}: {}\n{}\n\n",
            index + 1,
            page.url,
            page.text
        ));
    }
    let prompt = prompt.unwrap_or("Summarize the following content");
    let raw_bytes = pages.iter().map(|page| page.text.len() as u64).sum();
    let (result, cached) = summary::summarize(cli, cfg, "fetch", prompt, &content, urls.len() == 1);
    let (tokens, duration) = summary::record(
        &result,
        prompt,
        "fetch",
        pages.len() as u32,
        raw_bytes,
        started,
        cached,
    );
    let mut fields = json!({
        "urls": urls, "pages_fetched": pages.len(), "tokens_saved": tokens, "duration_secs": duration,
        "status": batch.status(), "failed": batch.failed,
    });
    if cli.metadata {
        fields["source_urls"] = json!(pages.iter().map(|page| &page.url).collect::<Vec<_>>());
        fields["fetch_timestamp"] = json!(timestamp);
        fields["cached"] = json!(cached);
    }
    output::emit(cli, &result, raw_bytes, fields);
}

fn emit_raw(cli: &Cli, urls: &[String], batch: &FetchBatch) {
    let pages = &batch.pages;
    if output::structured(cli) {
        output::emit_json(json!({
            "urls": urls, "pages_fetched": pages.len(), "pages": pages,
            "status": batch.status(), "failed": batch.failed,
        }));
        return;
    }
    for (index, page) in pages.iter().enumerate() {
        let text: String = page.text.chars().take(3000).collect();
        println!("\n### [{}] {}\n{}", index + 1, page.url, text);
    }
}
