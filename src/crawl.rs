//! Run and render Cloudflare crawl outcomes.
//! Preserves partial results and returns nonzero for empty or failed jobs.

mod api;
#[cfg(test)]
mod tests;

use crate::{cloudflare, commands, config::Config, fetch::http::validate_url, Cli};
use api::{CrawlApi, Record, Status};
use serde_json::json;
use std::time::{Duration, Instant};

const MAX_PAGES: u32 = 128;
const JOB_DEADLINE: Duration = Duration::from_secs(300);

pub fn run_crawl(cli: &Cli, cfg: &Config, url: &str, prompt: Option<&str>, limit: u32, depth: u32) {
    if cfg.cf_api_token.is_empty() || cfg.cf_account_id.is_empty() {
        commands::fail(
            1,
            "Cloudflare crawl requires CF_ACCOUNT_ID and CF_API_TOKEN",
        );
    }
    if limit == 0 || limit > MAX_PAGES {
        commands::fail(1, &format!("crawl limit must be 1–{MAX_PAGES}"));
    }
    validate_url(url).unwrap_or_else(|error| commands::fail(1, &error));
    let started = Instant::now();
    let client = crate::credential_client().unwrap_or_else(|error| commands::fail(2, &error));
    let endpoint = cloudflare::endpoint(&cfg.cf_account_id, "crawl")
        .unwrap_or_else(|error| commands::fail(1, &error));
    let api = CrawlApi {
        client: &client,
        endpoint,
        token: &cfg.cf_api_token,
        deadline: started + JOB_DEADLINE,
    };
    let (status, records) = api
        .run(url, limit, depth)
        .unwrap_or_else(|error| commands::fail(2, &error));
    render(cli, cfg, url, prompt, status, records, started);
}

fn render(
    cli: &Cli,
    cfg: &Config,
    url: &str,
    prompt: Option<&str>,
    status: Status,
    records: Vec<Record>,
    started: Instant,
) {
    let mut pages = Vec::new();
    let mut failed = Vec::new();
    for record in records {
        let text = record.markdown.unwrap_or_default();
        if record.status == "completed" && !text.trim().is_empty() {
            let clipped = text
                .chars()
                .take(cfg.max_page_chars.min(16_000))
                .collect::<String>();
            pages.push(
                json!({"url":record.url,"text":clipped,"truncated":clipped.len() < text.len()}),
            );
        } else {
            failed
                .push(json!({"url":record.url, "status":record.status, "outcome":record.outcome}));
        }
    }
    let state = if pages.is_empty() {
        "failed"
    } else if status != Status::Completed || !failed.is_empty() {
        "partial"
    } else {
        "ok"
    };
    let mut fields = json!({"url":url,"pages_crawled":pages.len(),"status":state,"job_status":status,"failed":failed});
    if pages.is_empty() {
        if commands::output::structured(cli) {
            commands::output::emit_json(fields);
        }
        commands::fail(2, "no content extracted from crawled pages");
    }
    if cli.raw && commands::output::structured(cli) {
        fields["pages"] = json!(pages);
        commands::output::emit_json(fields);
        return;
    }
    let content = pages
        .iter()
        .enumerate()
        .map(|(index, page)| {
            format!(
                "### Source {}: {}\n{}\n\n",
                index + 1,
                page["url"].as_str().unwrap_or_default(),
                page["text"].as_str().unwrap_or_default()
            )
        })
        .collect::<String>();
    if cli.raw {
        print!("{content}");
        return;
    }
    let prompt = prompt.unwrap_or("Summarize the following crawled website content");
    let (answer, _) = commands::summary::summarize(cli, cfg, "crawl", prompt, &content, false);
    let (tokens, duration) = commands::summary::record(
        &answer,
        prompt,
        "crawl",
        pages.len() as u32,
        content.len() as u64,
        started,
        false,
    );
    fields["tokens_saved"] = json!(tokens);
    fields["duration_secs"] = json!(duration);
    commands::output::emit(cli, &answer, content.len() as u64, fields);
}
