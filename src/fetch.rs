//! Acquire ordered URL outcomes and retain source provenance.
//! HTTP and Cloudflare requests run in a bounded worker pool.

mod browser;
mod documents;
pub(crate) mod http;
mod passage;
mod section;
#[cfg(test)]
mod tests;

pub use crate::fetch_utils::strip_html;
use crate::{config::Config, types::FetchedPage};
use reqwest::blocking::Client;
use serde::Serialize;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

const MIN_BROWSER_TEXT_CHARS: usize = 200;
const WORKERS: usize = 4;

#[derive(Serialize)]
pub struct FetchFailure {
    pub url: String,
    pub reason: String,
}

#[derive(Default)]
pub struct FetchBatch {
    pub pages: Vec<FetchedPage>,
    pub failed: Vec<FetchFailure>,
}

impl FetchBatch {
    pub fn status(&self) -> &'static str {
        match (self.pages.is_empty(), self.failed.is_empty()) {
            (true, _) => "failed",
            (false, false) => "partial",
            (false, true) => "ok",
        }
    }
}

pub fn fetch_page(client: &Client, url: &str, max_chars: usize) -> Option<FetchedPage> {
    http::direct(client, url, max_chars).ok()
}

pub fn fetch_page_jina(url: &str, max_chars: usize, key: &str) -> Option<FetchedPage> {
    http::jina(url, max_chars, key).ok()
}

pub fn fetch_pages_parallel(
    client: &Client,
    cfg: &Config,
    urls: &[String],
    max_pages: usize,
    use_cf: bool,
    use_browser: bool,
) -> FetchBatch {
    fetch_batch(client, cfg, urls, max_pages, (use_cf, use_browser), None)
}

pub(crate) fn fetch_search_evidence(
    client: &Client,
    cfg: &Config,
    urls: &[String],
    renderers: (bool, bool),
    query: &str,
) -> FetchBatch {
    fetch_batch(client, cfg, urls, urls.len(), renderers, Some(query))
}

fn fetch_batch(
    client: &Client,
    cfg: &Config,
    urls: &[String],
    max_pages: usize,
    (use_cf, use_browser): (bool, bool),
    focus: Option<&str>,
) -> FetchBatch {
    if max_pages == 0 {
        return FetchBatch::default();
    }
    let candidates = &urls[..urls.len().min(max_pages.saturating_add(2))];
    let max_chars = cfg.max_page_chars.min(16_000);
    let browser_available = !use_cf && browser::has_agent_browser();
    let direct = |url: &str| {
        if use_cf {
            http::cloudflare(cfg, url, max_chars)
        } else {
            http::direct_with_focus(client, url, max_chars, focus)
        }
    };
    if use_browser {
        return collect(
            candidates,
            candidates.iter().map(|url| browser_result(url, max_chars)),
            max_pages,
        );
    }
    let results = ordered_workers(candidates, direct);
    let retry = |url: &str| {
        if browser_available {
            browser_result(url, max_chars)
        } else {
            http::jina(url, max_chars, &cfg.jina_api_key)
        }
    };
    let retry = (!use_cf && (browser_available || !cfg.jina_api_key.is_empty())).then_some(retry);
    collect_with_retry(candidates, results, max_pages, retry)
}

fn collect_with_retry(
    candidates: &[String],
    results: Vec<Result<FetchedPage, String>>,
    max_pages: usize,
    retry: Option<impl Fn(&str) -> Result<FetchedPage, String>>,
) -> FetchBatch {
    let Some(retry) = retry else {
        return collect(candidates, results, max_pages);
    };
    let strong = results
        .iter()
        .filter(|result| {
            result
                .as_ref()
                .is_ok_and(|page| page.text.len() >= MIN_BROWSER_TEXT_CHARS)
        })
        .count();
    let mut remaining = max_pages.saturating_sub(strong).min(WORKERS * 2);
    let results = candidates.iter().zip(results).map(|(url, result)| {
        let weak = result
            .as_ref()
            .map_or(true, |page| page.text.len() < MIN_BROWSER_TEXT_CHARS);
        if weak && remaining > 0 {
            remaining -= 1;
            return retry(url).or(result);
        }
        result
    });
    collect(candidates, results, max_pages)
}

fn browser_result(url: &str, max_chars: usize) -> Result<FetchedPage, String> {
    http::validate_url(url)?;
    browser::fetch_page_browser(url, max_chars)
}

fn ordered_workers<T: Send>(urls: &[String], fetch: impl Fn(&str) -> T + Sync) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let (send, receive) = mpsc::channel();
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(urls.len()) {
            let send = send.clone();
            let (next, fetch) = (&next, &fetch);
            scope.spawn(move || loop {
                let index = next.fetch_add(1, Ordering::Relaxed);
                let Some(url) = urls.get(index) else { break };
                if send.send((index, fetch(url))).is_err() {
                    break;
                }
            });
        }
    });
    drop(send);
    let mut records = receive.into_iter().collect::<Vec<_>>();
    records.sort_unstable_by_key(|(index, _)| *index);
    records.into_iter().map(|(_, record)| record).collect()
}

fn collect(
    urls: &[String],
    results: impl IntoIterator<Item = Result<FetchedPage, String>>,
    limit: usize,
) -> FetchBatch {
    let mut batch = FetchBatch::default();
    for (url, result) in urls.iter().zip(results) {
        match result {
            Ok(page) if batch.pages.len() < limit => batch.pages.push(page),
            Ok(_) => {}
            Err(reason) => batch.failed.push(FetchFailure {
                url: url.clone(),
                reason,
            }),
        }
    }
    batch
}

fn page(
    url: &str,
    final_url: &str,
    extractor: &'static str,
    media_type: &str,
    text: String,
    max_chars: usize,
) -> Result<FetchedPage, String> {
    if text.trim().is_empty() || max_chars == 0 {
        return Err("no extractable text".into());
    }
    let end = text
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(text.len());
    Ok(FetchedPage {
        url: url.into(),
        final_url: final_url.into(),
        extractor,
        media_type: media_type.into(),
        truncated: end < text.len(),
        text: text[..end].into(),
    })
}
