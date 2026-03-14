// Cloudflare Browser Rendering crawl flow and summarization orchestration.
// Exports: run_crawl().
// Deps: crate::config::Config, crate::llm::llm_summarize, crate::stats::record_search, reqwest blocking client.

use reqwest::blocking::Client;
use std::thread;
use std::time::Instant;

use crate::config::Config;
use crate::llm::llm_summarize;
use crate::stats::record_search;
use crate::Cli;

pub fn run_crawl(
    cli: &Cli,
    cfg: &Config,
    client: &Client,
    url: &str,
    prompt: &Option<String>,
    limit: u32,
    depth: u32,
) {
    if cfg.cf_account_id.is_empty() || cfg.cf_api_token.is_empty() {
        eprintln!("Error: Cloudflare crawl requires cf_account_id and cf_api_token in config");
        eprintln!("Set via config.toml or env vars CF_ACCOUNT_ID / CF_API_TOKEN");
        std::process::exit(1);
    }

    let t0 = Instant::now();
    let api_url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/browser-rendering/crawl",
        cfg.cf_account_id
    );

    eprintln!("Starting crawl: {url} (limit={limit}, depth={depth})");
    let payload = serde_json::json!({
        "url": url,
        "limit": limit,
        "depth": depth,
        "formats": ["markdown"],
        "render": true,
        "rejectResourceTypes": ["image", "media", "font", "stylesheet"]
    });

    let resp = client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", cfg.cf_api_token))
        .header("Content-Type", "application/json")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(30))
        .send();

    let resp = match resp {
        Ok(response) => response,
        Err(error) => {
            eprintln!("[ai-summary] Crawl request failed: {error}");
            return;
        }
    };

    let body: serde_json::Value = match resp.json() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("[ai-summary] Failed to parse crawl response: {error}");
            return;
        }
    };

    if body.get("success").and_then(|value| value.as_bool()) != Some(true) {
        eprintln!(
            "[ai-summary] Crawl API error: {}",
            serde_json::to_string_pretty(&body).unwrap_or_default()
        );
        return;
    }

    let job_id = match body.get("result").and_then(|value| value.as_str()) {
        Some(id) => id.to_string(),
        None => {
            eprintln!("[ai-summary] No job ID in response: {body}");
            return;
        }
    };
    eprintln!("Crawl job started: {job_id}");

    let poll_url = format!("{}/{}?limit={}", api_url, job_id, limit);
    let mut attempts = 0;
    let max_attempts = 60;
    let result_body = loop {
        attempts += 1;
        if attempts > max_attempts {
            eprintln!("[ai-summary] Crawl timed out after {}s", attempts * 5);
            return;
        }
        thread::sleep(std::time::Duration::from_secs(5));

        let poll = client
            .get(&poll_url)
            .header("Authorization", format!("Bearer {}", cfg.cf_api_token))
            .timeout(std::time::Duration::from_secs(15))
            .send();

        let poll_body: serde_json::Value = match poll.ok().and_then(|response| response.json().ok())
        {
            Some(value) => value,
            None => continue,
        };

        let result = poll_body.get("result").cloned().unwrap_or_default();
        let status = result
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        let finished = result
            .get("finished")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);
        let total = result
            .get("total")
            .and_then(|value| value.as_u64())
            .unwrap_or(0);

        eprintln!("[ai-summary] Crawl status: {status} ({finished}/{total} pages)");

        match status {
            "completed" | "cancelled_due_to_limits" => break result,
            "errored" | "cancelled_due_to_timeout" | "cancelled_by_user" => {
                eprintln!("[ai-summary] Crawl ended with status: {status}");
                if finished > 0 {
                    break result;
                } else {
                    return;
                }
            }
            _ => continue,
        }
    };

    let records = result_body
        .get("records")
        .and_then(|value| value.as_array());
    let records = match records {
        Some(records) if !records.is_empty() => records,
        _ => {
            eprintln!("[ai-summary] No pages crawled.");
            return;
        }
    };

    let mut ctx = String::new();
    let mut raw_total: u64 = 0;
    let mut page_count = 0u32;
    for (index, record) in records.iter().enumerate() {
        let record_status = record
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if record_status != "completed" {
            continue;
        }
        let record_url = record
            .get("url")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        let markdown = record
            .get("markdown")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        if markdown.is_empty() {
            continue;
        }
        let text = if markdown.len() > cfg.max_page_chars {
            &markdown[..cfg.max_page_chars]
        } else {
            markdown
        };
        ctx.push_str(&format!(
            "### Source {}: {}\n{}\n\n",
            index + 1,
            record_url,
            text
        ));
        raw_total += text.len() as u64;
        page_count += 1;
    }

    if page_count == 0 {
        eprintln!("[ai-summary] No content extracted from crawled pages.");
        return;
    }

    let t1 = Instant::now();
    eprintln!(
        "Crawled {} pages with content ({:.1}s)",
        page_count,
        t1.duration_since(t0).as_secs_f64()
    );

    if cli.raw {
        print!("{ctx}");
        return;
    }

    let q = prompt
        .as_deref()
        .unwrap_or("Summarize the following crawled website content");
    eprintln!("Summarizing with {} ({})...", cfg.model, cfg.api_url);
    let result = llm_summarize(client, cfg, q, &ctx);
    let dur = t0.elapsed().as_secs_f64();
    eprintln!("Done ({dur:.1}s total)");

    record_search(
        q,
        "crawl",
        page_count,
        raw_total,
        result.summary_chars,
        result.usage.as_ref(),
        dur,
    );
    let tokens_saved = (raw_total / 4).saturating_sub(result.summary_chars / 4);
    if cli.json {
        let json_val = serde_json::json!({
            "url": url,
            "summary": result.text,
            "pages_crawled": page_count,
            "tokens_saved": tokens_saved,
            "duration_secs": dur,
        });
        println!("{}", serde_json::to_string(&json_val).unwrap());
    } else {
        println!("{}", result.text);
    }
}
