//! Apply shared summary cache, accounting, and execution policy.
//! Used by the fetch, search, stdin, and repository commands.

use super::output;
use crate::llm::Summary;
use crate::{cache, config::Config, llm, stats::record_search, Cli};
use std::time::Instant;

pub fn summarize(
    cli: &Cli,
    cfg: &Config,
    mode: &str,
    prompt: &str,
    content: &str,
    cacheable: bool,
) -> (Summary, bool) {
    const CONTEXT_BYTES: usize = 256 * 1024;
    const PROMPT_BYTES: usize = 32 * 1024;
    if prompt.len() > PROMPT_BYTES {
        super::fail(1, "summary prompt exceeds 32 KiB");
    }
    let content = if content.len() > CONTEXT_BYTES {
        eprintln!("[ai-summary] summary context limited to 256 KiB");
        crate::text::prefix(content, CONTEXT_BYTES)
    } else {
        content
    };
    let scope = if cacheable && !cli.no_cache {
        llm::cache_scope(cfg)
    } else {
        None
    };
    let key = scope
        .as_ref()
        .map(|(_, scope)| cache::CacheKey::new(mode, prompt, content, scope));
    if let Some(entry) = key.as_ref().and_then(cache::cache_get) {
        eprintln!("[ai-summary] cache hit");
        return (entry.summary, true);
    }
    let result = llm::llm_summarize(cfg, prompt, content, output::use_sse(cli));
    let summary = output::require_summary(result);
    if let (Some(key), Some((preferred, _))) = (key, scope) {
        if preferred == summary.provider {
            cache::cache_put(&key, &summary);
        }
    }
    (summary, false)
}

pub fn record(
    summary: &Summary,
    prompt: &str,
    mode: &str,
    sources: u32,
    raw_bytes: u64,
    started: Instant,
    cached: bool,
) -> (u64, f64) {
    let duration = started.elapsed().as_secs_f64();
    let summary_bytes = summary.text.len() as u64;
    let usage = if cached { None } else { summary.usage.as_ref() };
    record_search(
        prompt,
        mode,
        sources,
        raw_bytes,
        summary_bytes,
        usage,
        duration,
    );
    ((raw_bytes / 4).saturating_sub(summary_bytes / 4), duration)
}
