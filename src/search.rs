// Search provider integrations and URL encoding helpers.
// Exports: UA, urlenc(), urldec(), search_gemini(), search_web().
// Deps: reqwest blocking client, scraper selectors, crate::types.

pub mod deep;
mod gemini;
pub use gemini::{search_gemini, summarize_evidence, GroundedAnswer};

use crate::types::{error_detail, SearchResult};
use reqwest::blocking::Client;
use scraper::{Html, Selector};
use std::collections::HashSet;

pub const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
fn urlenc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{:02X}", b),
        })
        .collect()
}

fn urldec(s: &str) -> String {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(value) =
                u8::from_str_radix(&String::from_utf8_lossy(&bytes[index + 1..index + 3]), 16)
            {
                out.push(value);
                index += 3;
                continue;
            }
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

pub fn search_web(
    client: &Client,
    query: &str,
    num: usize,
    brave_key: &str,
) -> Result<Vec<SearchResult>, String> {
    let ddg_reason = match search_ddg(client, query, num) {
        Ok(results) if !results.is_empty() => return Ok(results),
        Ok(_) => "returned no results".to_string(),
        Err(reason) => reason,
    };
    if brave_key.is_empty() {
        return Err(format!(
            "DDG failed: {ddg_reason}; Brave Search is not configured"
        ));
    }
    eprintln!("[ai-summary] DDG unavailable ({ddg_reason}), trying Brave Search...");
    search_brave(query, num, brave_key)
        .map_err(|reason| format!("DDG failed: {ddg_reason}; Brave failed: {reason}"))
}

fn search_ddg(client: &Client, query: &str, num: usize) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlenc(query));
    let response = client
        .get(&url)
        .header("User-Agent", UA)
        .send()
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let bytes = crate::read_limit(response, 1024 * 1024)?;
    let body = String::from_utf8_lossy(&bytes);
    let results = parse_ddg_html(&body, num);
    if results.is_empty() {
        return Err(if body.contains("anomaly-modal") {
            "DuckDuckGo requested a human verification"
        } else {
            "response contained no results"
        }
        .into());
    }
    Ok(results)
}

pub(crate) fn parse_ddg_html(html: &str, limit: usize) -> Vec<SearchResult> {
    if limit == 0 {
        return Vec::new();
    }
    let doc = Html::parse_document(html);
    let result_selector = Selector::parse(".result").unwrap();
    let link_selector = Selector::parse("a.result__a").unwrap();
    let snippet_selector = Selector::parse("a.result__snippet").unwrap();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for el in doc.select(&result_selector) {
        if let Some(link) = el.select(&link_selector).next() {
            let title = link.text().collect::<String>().trim().to_string();
            let href = link.value().attr("href").unwrap_or("");
            let url = if href.contains("uddg=") {
                href.split("uddg=")
                    .nth(1)
                    .map(|segment| urldec(segment.split('&').next().unwrap_or(segment)))
                    .unwrap_or_default()
            } else {
                href.to_string()
            };
            if crate::fetch::http::validate_url(&url).is_err() || !seen.insert(url.clone()) {
                continue;
            }
            let snippet = el
                .select(&snippet_selector)
                .next()
                .map(|item| item.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            out.push(SearchResult {
                url,
                title,
                snippet,
            });
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

fn search_brave(query: &str, num: usize, key: &str) -> Result<Vec<SearchResult>, String> {
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlenc(query),
        num
    );
    let response = crate::credential_client()?
        .get(&url)
        .header("Accept", "application/json")
        .header("X-Subscription-Token", key)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .map_err(|error| format!("request failed: {error}"))?;
    let status = response.status();
    let bytes = crate::read_limit(response, 1024 * 1024)?;
    let body = String::from_utf8_lossy(&bytes);
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", error_detail(&body, key)));
    }
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| format!("invalid JSON response: {error}"))?;
    let results = parse_brave_value(&value, num);
    if results.is_empty() {
        return Err("response contained no web results".to_string());
    }
    Ok(results)
}

#[cfg(test)]
pub(crate) fn parse_brave_json(body: &str, limit: usize) -> Vec<SearchResult> {
    let Ok(value) = serde_json::from_str(body) else {
        return Vec::new();
    };
    parse_brave_value(&value, limit)
}

fn parse_brave_value(value: &serde_json::Value, limit: usize) -> Vec<SearchResult> {
    value
        .get("web")
        .and_then(|web| web.get("results"))
        .and_then(|results| results.as_array())
        .map(|items| {
            let mut seen = HashSet::new();
            items
                .iter()
                .filter_map(|item| {
                    let url = item.get("url")?.as_str()?.to_string();
                    if crate::fetch::http::validate_url(&url).is_err() || !seen.insert(url.clone())
                    {
                        return None;
                    }
                    Some(SearchResult {
                        url,
                        title: item
                            .get("title")
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .to_string(),
                        snippet: item
                            .get("description")
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .take(limit)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urlenc_handles_spaces_and_ampersands() {
        let encoded = urlenc("rust & code");
        assert_eq!(encoded, "rust+%26+code");
    }

    #[test]
    fn urldec_percent_sequences() {
        assert_eq!(urldec("%41%42%20x"), "AB x");
    }

    #[test]
    fn urldec_plus_means_space() {
        assert_eq!(urldec("a+b+c"), "a b c");
    }

    #[test]
    fn ddg_parses_results_from_html() {
        let html = r#"
            <div class="result">
                <a class="result__a" href="/l/?uddg=https%3A%2F%2Fprimary.example%2F">Primary</a>
                <a class="result__snippet">Primary summary</a>
            </div>
            <div class="result">
                <a class="result__a" href="https://secondary.example/">Secondary</a>
                <a class="result__snippet">Secondary summary</a>
            </div>
        "#;
        let results = parse_ddg_html(html, 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].url, "https://primary.example/");
        assert_eq!(results[0].title, "Primary");
        assert_eq!(results[0].snippet, "Primary summary");
        assert_eq!(results[1].url, "https://secondary.example/");
        assert_eq!(results[1].snippet, "Secondary summary");
    }

    #[test]
    fn brave_parses_filtered_web_results() {
        let body = r#"{
            "web": {
                "results": [
                    {"url": "https://alpha.example/", "title": "Alpha", "description": "Alpha desc"},
                    {"url": "ftp://ignore", "title": "Skip", "description": "Ignored"},
                    {"url": "https://beta.example/"}
                ]
            }
        }"#;
        let results = parse_brave_json(body, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].url, "https://alpha.example/");
        assert_eq!(results[0].title, "Alpha");
        assert_eq!(results[0].snippet, "Alpha desc");
        assert_eq!(results[1].url, "https://beta.example/");
        assert_eq!(results[1].title, "");
        assert_eq!(results[1].snippet, "");
    }
}
