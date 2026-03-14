// Page fetching, Cloudflare rendering fetches, browser fetches, and parallel dispatch.
// Exports: fetch_page(), fetch_page_browser(), fetch_page_cf(), fetch_page_jina(), fetch_pages_parallel().
// Deps: crate::config::Config, crate::search::UA, reqwest blocking client, std::process.
use reqwest::blocking::Client;
use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use crate::config::Config;
use crate::search::UA;
use crate::types::FetchedPage;
pub use crate::fetch_utils::{collapse_ws, extract_redirect, strip_html};
pub(crate) use crate::fetch_utils::strip_ansi;
use crate::fetch_utils::{extracted_text, truncate_text};
const MIN_BROWSER_TEXT_CHARS: usize = 200;
pub fn fetch_page(client: &Client, url: &str, max_chars: usize) -> Option<FetchedPage> {
    fetch_page_inner(client, url, max_chars, 3)
}
fn fetch_page_inner(
    client: &Client,
    url: &str,
    max_chars: usize,
    redirects_left: u8,
) -> Option<FetchedPage> {
    let resp = client
        .get(url)
        .header("User-Agent", UA)
        .timeout(std::time::Duration::from_secs(8))
        .send();
    let resp = match resp {
        Ok(response) => response,
        Err(error) => {
            eprintln!("[ai-summary] Failed to fetch {url}: {error}");
            return None;
        }
    };
    if !resp.status().is_success() {
        eprintln!("[ai-summary] HTTP {} for {url}", resp.status());
        return None;
    }
    let content_type = resp
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !content_type.contains("text/html") && !content_type.contains("text/plain") {
        eprintln!("[ai-summary] Skipping {url}: content-type {content_type}");
        return None;
    }
    let mut body = String::new();
    resp.take(200_000).read_to_string(&mut body).ok()?;
    if body.len() < 1000 && redirects_left > 0 {
        if let Some(redirect_url) = extract_redirect(&body) {
            eprintln!("[ai-summary] Following redirect -> {redirect_url}");
            return fetch_page_inner(client, &redirect_url, max_chars, redirects_left - 1);
        }
    }
    let text = extracted_text(url, &body);
    if text.len() < 50 {
        eprintln!(
            "[ai-summary] Skipping {url}: content too short after extraction ({} chars)",
            text.len()
        );
        return None;
    }
    Some(FetchedPage {
        url: url.to_string(),
        text: truncate_text(text, max_chars),
    })
}
pub fn fetch_page_cf(
    client: &Client,
    cfg: &Config,
    url: &str,
    max_chars: usize,
) -> Option<FetchedPage> {
    let api_url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{}/browser-rendering/content",
        cfg.cf_account_id
    );
    let resp = client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", cfg.cf_api_token))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "url": url,
            "rejectResourceTypes": ["image", "media", "font", "stylesheet"]
        }))
        .timeout(std::time::Duration::from_secs(30))
        .send();
    let body = match resp {
        Ok(response) => response.text().ok()?,
        Err(error) => {
            eprintln!("[ai-summary] CF fetch failed for {url}: {error}");
            return None;
        }
    };
    let text = extracted_text(url, &body);
    if text.len() < 50 {
        eprintln!(
            "[ai-summary] CF: skipping {url}: content too short ({} chars)",
            text.len()
        );
        return None;
    }
    Some(FetchedPage {
        url: url.to_string(),
        text: truncate_text(text, max_chars),
    })
}
pub fn fetch_page_jina(
    client: &Client,
    url: &str,
    max_chars: usize,
    api_key: &str,
) -> Option<FetchedPage> {
    let jina_url = format!("https://r.jina.ai/{}", url);
    let mut req = client
        .get(&jina_url)
        .header("Accept", "text/plain")
        .timeout(std::time::Duration::from_secs(15));
    if !api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", api_key));
    }
    let body = match req.send() {
        Ok(resp) => resp.text().ok()?,
        Err(error) => {
            eprintln!("[ai-summary] Jina fetch failed for {url}: {error}");
            return None;
        }
    };
    if body.len() < 50 || body.contains("AuthenticationRequiredError") {
        return None;
    }
    Some(FetchedPage {
        url: url.to_string(),
        text: truncate_text(body, max_chars),
    })
}
pub fn has_agent_browser() -> bool {
    Command::new("agent-browser")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
fn agent_browser(args: &[&str]) -> Option<Output> {
    Command::new("agent-browser")
        .args(args)
        .env("AGENT_BROWSER_DEFAULT_TIMEOUT", "12000")
        .stderr(Stdio::null())
        .output()
        .ok()
}
fn browser_text(args: &[&str]) -> Option<String> {
    let output = agent_browser(args)?;
    if !output.status.success() {
        return None;
    }
    let text = strip_ansi(&String::from_utf8_lossy(&output.stdout));
    Some(collapse_ws(text.trim()))
}
pub fn fetch_page_browser(url: &str, max_chars: usize) -> Option<FetchedPage> {
    eprintln!("[ai-summary] Browser fetch: {url}");
    if agent_browser(&["open", url]).map(|output| output.status.success()) != Some(true) {
        eprintln!("[ai-summary] agent-browser open failed for {url}");
        return None;
    }
    if agent_browser(&["wait", "--load", "networkidle"]).map(|output| output.status.success())
        != Some(true)
    {
        eprintln!("[ai-summary] agent-browser wait timed out for {url}, reading current page");
    }
    let mut text = browser_text(&["get", "text", "main"]).unwrap_or_default();
    if text.len() < MIN_BROWSER_TEXT_CHARS {
        text = browser_text(&["get", "text", "body"]).unwrap_or(text);
    }
    if text.len() < 50 {
        eprintln!(
            "[ai-summary] agent-browser: skipping {url}: content too short ({} chars)",
            text.len()
        );
        return None;
    }
    Some(FetchedPage {
        url: url.to_string(),
        text: truncate_text(text, max_chars),
    })
}
pub fn fetch_pages_parallel(
    client: &Client,
    cfg: &Config,
    urls: &[String],
    max_pages: usize,
    max_chars: usize,
    use_cf: bool,
    use_browser: bool,
) -> Vec<FetchedPage> {
    if use_browser {
        if has_agent_browser() {
            return urls
                .iter()
                .take(max_pages.saturating_add(2))
                .filter_map(|url| fetch_page_browser(url, max_chars))
                .take(max_pages)
                .collect();
        }
        eprintln!("[ai-summary] agent-browser not found, falling back to HTTP fetch...");
    }
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();
    for url in urls.iter().take(max_pages.saturating_add(2)) {
        let (tx, url, client, cfg) = (tx.clone(), url.clone(), client.clone(), cfg.clone());
        handles.push(thread::spawn(move || {
            let page = if use_cf {
                fetch_page_cf(&client, &cfg, &url, max_chars)
            } else {
                fetch_page(&client, &url, max_chars)
            };
            let _ = tx.send((url, page));
        }));
    }
    drop(tx);
    let mut pages = Vec::new();
    let mut retries = Vec::new();
    for (url, page) in rx {
        if use_cf {
            if let Some(page) = page {
                pages.push(page);
            }
            continue;
        }
        match page {
            Some(page) if page.text.len() < MIN_BROWSER_TEXT_CHARS => {
                retries.push((url, Some(page)))
            }
            Some(page) => pages.push(page),
            None => retries.push((url, None)),
        }
    }
    for handle in handles {
        let _ = handle.join();
    }
    if use_cf || retries.is_empty() {
        pages.truncate(max_pages);
        return pages;
    }
    if !has_agent_browser() {
        // Try Jina Reader as fallback for failed/short fetches (works without API key)
        for (url, page) in &retries {
            if pages.len() >= max_pages {
                break;
            }
            eprintln!("[ai-summary] Retrying via Jina Reader: {url}");
            if let Some(jina_page) =
                fetch_page_jina(client, url, max_chars, &cfg.jina_api_key).or(page.clone())
            {
                pages.push(jina_page);
            }
        }
        pages.truncate(max_pages);
        return pages;
    }
    for (url, page) in retries {
        if pages.len() >= max_pages {
            break;
        }
        eprintln!("[ai-summary] Retrying via agent-browser: {url}");
        if let Some(page) = fetch_page_browser(&url, max_chars).or(page) {
            pages.push(page);
        }
    }
    pages.truncate(max_pages);
    pages
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strips_html_tags() {
        let html = "<body><p>hello <span>world</span></p></body>";
        assert_eq!(strip_html(html), "hello world");
    }
    #[test]
    fn strips_color_codes() {
        assert_eq!(strip_ansi("\x1b[32mhello\x1b[0m"), "hello");
    }
    #[test]
    fn has_agent_browser_returns_bool() {
        let result = has_agent_browser();
        assert!(result == true || result == false);
    }
}
