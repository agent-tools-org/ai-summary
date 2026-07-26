//! LLM backend selection and public summarization API.
//! Exports: has_opencode(), llm_summarize(), strip_thinking().
//! Deps: Config, reqwest client, HTTP backend, opencode backend.

mod clean;
mod http;
mod opencode;
mod stream;

use reqwest::blocking::Client;

use crate::config::Config;
use crate::types::SummarizeResult;
use http::HttpSummaryRequest;

const OPENROUTER_API_URL: &str = "https://openrouter.ai/api";
const OPENCODE_MODEL: &str = "opencode/nemotron-3-super-free";

pub(crate) fn has_opencode() -> bool {
    opencode::has_opencode()
}

/// Main entry: tries OpenRouter direct -> opencode CLI -> config API.
pub fn llm_summarize(
    client: &Client,
    cfg: &Config,
    prompt: &str,
    content: &str,
    streaming: bool,
) -> SummarizeResult {
    if let Some(result) = try_openrouter(client, cfg, prompt, content, streaming) {
        return result;
    }
    if let Some(result) = try_opencode(prompt, content, streaming, &cfg.model) {
        return result;
    }
    summarize_config_api(client, cfg, prompt, content, streaming)
}

fn try_openrouter(
    client: &Client,
    cfg: &Config,
    prompt: &str,
    content: &str,
    streaming: bool,
) -> Option<SummarizeResult> {
    if cfg.openrouter_api_key.is_empty() {
        return None;
    }
    let model = &cfg.openrouter_model;
    let model_short = model.split('/').next_back().unwrap_or(model);
    let is_free = model.ends_with(":free");
    let cost_label = if is_free { "OpenRouter, free" } else { "OpenRouter" };
    eprintln!("Summarizing with {model_short} ({cost_label})...");
    let result = http::summarize_http(
        client,
        HttpSummaryRequest {
            api_url: OPENROUTER_API_URL,
            api_key: &cfg.openrouter_api_key,
            model,
            model_display: model_short,
            max_tokens: cfg.max_summary_tokens,
            cost_label,
            prompt,
            content,
            streaming,
        },
    );
    if result.summary_chars > 0 {
        return Some(result);
    }
    eprintln!("[ai-summary] OpenRouter failed, trying fallback...");
    None
}

fn try_opencode(
    prompt: &str,
    content: &str,
    streaming: bool,
    fallback_model: &str,
) -> Option<SummarizeResult> {
    if !has_opencode() {
        return None;
    }
    let model_short = OPENCODE_MODEL
        .strip_prefix("opencode/")
        .unwrap_or(OPENCODE_MODEL);
    eprintln!("Summarizing with {model_short} (opencode, free)...");
    if let Some(result) = opencode::summarize_opencode(OPENCODE_MODEL, prompt, content, streaming) {
        return Some(result);
    }
    eprintln!("[ai-summary] opencode failed, falling back to {fallback_model}...");
    None
}

fn summarize_config_api(
    client: &Client,
    cfg: &Config,
    prompt: &str,
    content: &str,
    streaming: bool,
) -> SummarizeResult {
    let is_local = cfg.api_url.contains("127.0.0.1") || cfg.api_url.contains("localhost");
    let cost_label = if is_local { "free, local" } else { "API" };
    eprintln!("Summarizing with {} ({})...", cfg.model, cfg.api_url);
    http::summarize_http(
        client,
        HttpSummaryRequest {
            api_url: &cfg.api_url,
            api_key: &cfg.api_key,
            model: &cfg.model,
            model_display: &cfg.model,
            max_tokens: cfg.max_summary_tokens,
            cost_label,
            prompt,
            content,
            streaming,
        },
    )
}
