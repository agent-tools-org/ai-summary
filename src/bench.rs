// Benchmark helpers for ai-summary fetch & summarization flows.
// Exports: run_bench().
// Deps: config::Config, fetch::{fetch_page, fetch_page_jina}, llm::llm_summarize, reqwest blocking Client.

use reqwest::blocking::Client;

use crate::config::Config;
use crate::fetch::{fetch_page, fetch_page_jina};
use crate::llm::llm_summarize;

const BENCH_URLS: [&str; 5] = [
    "https://docs.rs/reqwest/latest/reqwest/",
    "https://react.dev/learn",
    "https://en.wikipedia.org/wiki/Rust_(programming_language)",
    "https://news.ycombinator.com",
    "https://github.com/nickel-org/nickel.rs",
];

struct BenchRow {
    url: String,
    source: &'static str,
    raw_tokens: u64,
    compressed_tokens: u64,
    ratio_pct: f64,
    tokens_saved: u64,
}

pub fn run_bench(cfg: &Config, client: &Client) {
    let rows: Vec<BenchRow> = BENCH_URLS
        .iter()
        .filter_map(|url| bench_row(client, cfg, url))
        .collect();
    if rows.is_empty() {
        eprintln!("[ai-summary] Bench failed to capture any content.");
    }
    print_table(&rows);
}

fn bench_row(client: &Client, cfg: &Config, url: &str) -> Option<BenchRow> {
    eprintln!("[bench] Fetching {url}...");
    let (source, text) = fetch_content(client, cfg, url)?;
    let raw_chars = text.len() as u64;
    eprintln!("[bench] Summarizing {url} ({raw_chars} chars)...");
    let summary = llm_summarize(client, cfg, "", &text, false);
    let raw_tokens = raw_chars / 4;
    let compressed_tokens = summary.summary_chars / 4;
    let ratio_pct = if raw_tokens == 0 {
        0.0
    } else {
        (compressed_tokens as f64 / raw_tokens as f64) * 100.0
    };
    let tokens_saved = raw_tokens.saturating_sub(compressed_tokens);
    Some(BenchRow {
        url: url.to_string(),
        source,
        raw_tokens,
        compressed_tokens,
        ratio_pct,
        tokens_saved,
    })
}

fn fetch_content(client: &Client, cfg: &Config, url: &str) -> Option<(&'static str, String)> {
    if let Some(page) = fetch_page(client, url, cfg.max_page_chars) {
        if page.text.len() >= 200 {
            return Some(("http", page.text));
        }
        eprintln!(
            "[bench] {url}: HTTP fetch produced {} chars, trying Jina fallback",
            page.text.len()
        );
    }
    if let Some(page) = fetch_page_jina(client, url, cfg.max_page_chars, &cfg.jina_api_key) {
        return Some(("jina", page.text));
    }
    eprintln!("[bench] {url}: Failed to fetch content");
    None
}

fn print_table(rows: &[BenchRow]) {
    println!("ai-summary benchmark");
    println!("====================\n");
    println!(
        "{:<30} {:<6} {:>10} {:>12} {:>7} {:>12}",
        "URL", "Source", "Raw Tokens", "Compressed", "Ratio", "Saved"
    );
    println!("{}", "-".repeat(81));
    let mut total_raw: u64 = 0;
    let mut total_compressed: u64 = 0;
    for row in rows {
        total_raw += row.raw_tokens;
        total_compressed += row.compressed_tokens;
        println!(
            "{:<30} {:<6} {:>10} {:>12} {:>6.1}% {:>12}",
            truncate_url(&row.url),
            row.source,
            row.raw_tokens,
            row.compressed_tokens,
            row.ratio_pct,
            row.tokens_saved
        );
    }
    println!("{}", "-".repeat(81));
    let total_ratio = if total_raw == 0 {
        0.0
    } else {
        (total_compressed as f64 / total_raw as f64) * 100.0
    };
    let total_saved = total_raw.saturating_sub(total_compressed);
    println!(
        "{:<30} {:<6} {:>10} {:>12} {:>6.1}% {:>12}",
        "TOTAL", "", total_raw, total_compressed, total_ratio, total_saved
    );
}

fn truncate_url(url: &str) -> String {
    let url = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    if url.len() <= 30 {
        url.to_string()
    } else {
        format!("{}...", &url[..27])
    }
}
