// Search provider integrations and URL encoding helpers.
// Exports: UA, urlenc(), urldec(), search_gemini(), search_web().
// Deps: reqwest blocking client, scraper selectors, crate::types.

use crate::types::{error_detail, SearchResult, SummarizeResult, Usage};
use reqwest::blocking::Client;
use scraper::{Html, Selector};

pub const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
pub fn urlenc(s: &str) -> String {
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

pub fn urldec(s: &str) -> String {
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

pub fn search_gemini(
    client: &Client,
    query: &str,
    api_key: &str,
    model: &str,
) -> Result<SummarizeResult, String> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model, api_key
    );

    let payload = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": format!(
                    "{}\n\nProvide a concise, factual answer. Use bullet points. Include specific facts, numbers, code examples if relevant. Keep under 500 words.",
                    query
                )
            }]
        }],
        "tools": [{"google_search": {}}]
    });
    let resp = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .map_err(|error| format!("connection error: {}", error_detail(&error.to_string(), api_key)))?;

    let status = resp.status();
    let body = resp
        .text()
        .map_err(|error| {
            format!(
                "failed to read response: {}",
                error_detail(&error.to_string(), api_key)
            )
        })?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {}", error_detail(&body, api_key)));
    }
    let body: serde_json::Value = serde_json::from_str(&body)
        .map_err(|error| format!("invalid JSON response: {error}"))?;
    let parts = body
        .get("candidates")
        .and_then(|candidates| candidates.get(0))
        .and_then(|candidate| candidate.get("content"))
        .and_then(|content| content.get("parts"))
        .and_then(|parts| parts.as_array())
        .ok_or_else(|| "response did not contain candidate content".to_string())?;
    let text: String = parts
        .iter()
        .filter_map(|part| part.get("text").and_then(|value| value.as_str()))
        .collect::<Vec<_>>()
        .join("");

    let mut sources = Vec::new();
    if let Some(meta) = body
        .get("candidates")
        .and_then(|candidates| candidates.get(0))
        .and_then(|candidate| candidate.get("groundingMetadata"))
    {
        if let Some(chunks) = meta
            .get("groundingChunks")
            .and_then(|chunks| chunks.as_array())
        {
            for chunk in chunks {
                if let Some(web) = chunk.get("web") {
                    let uri = web
                        .get("uri")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    let title = web
                        .get("title")
                        .and_then(|value| value.as_str())
                        .unwrap_or("");
                    if !uri.is_empty() {
                        sources.push(format!("- [{}]({})", title, uri));
                    }
                }
            }
        }
    }
    let usage = body.get("usageMetadata").and_then(|usage| {
        Some(Usage {
            prompt_tokens: usage.get("promptTokenCount")?.as_u64()? as u32,
            completion_tokens: usage.get("candidatesTokenCount")?.as_u64()? as u32,
        })
    });

    let sources_str = if sources.is_empty() {
        String::new()
    } else {
        format!("\n\nSources:\n{}", sources.join("\n"))
    };
    let raw_estimate = 14000u64;
    Ok(SummarizeResult {
        text: format!(
            "{text}{sources_str}\n\n---\n[ai-summary] Gemini ({model}) + Google Search | {usage_info}",
            usage_info = usage
                .as_ref()
                .map(|usage| format!(
                    "tokens: {}+{} (Gemini API)",
                    usage.prompt_tokens, usage.completion_tokens
                ))
                .unwrap_or_default(),
        ),
        usage,
        raw_chars: raw_estimate,
        summary_chars: text.len() as u64,
    })
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
    search_brave(client, query, num, brave_key)
        .map_err(|reason| format!("DDG failed: {ddg_reason}; Brave failed: {reason}"))
}

pub fn search_ddg(client: &Client, query: &str, num: usize) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlenc(query));
    let response = client
        .get(&url)
        .header("User-Agent", UA)
        .send()
        .map_err(|error| format!("request failed: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }
    let body = response
        .text()
        .map_err(|error| format!("failed to read response: {error}"))?;
    let results = parse_ddg_html(&body, num);
    if results.is_empty() {
        return Err("response contained no results".to_string());
    }
    Ok(results)
}

pub(crate) fn parse_ddg_html(html: &str, limit: usize) -> Vec<SearchResult> {
    let doc = Html::parse_document(html);
    let result_selector = Selector::parse(".result").unwrap();
    let link_selector = Selector::parse("a.result__a").unwrap();
    let snippet_selector = Selector::parse("a.result__snippet").unwrap();
    let mut out = Vec::new();
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
            if !url.starts_with("http") {
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

pub fn search_brave(
    client: &Client,
    query: &str,
    num: usize,
    key: &str,
) -> Result<Vec<SearchResult>, String> {
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlenc(query),
        num
    );
    let response = client
        .get(&url)
        .header("Accept", "application/json")
        .header("X-Subscription-Token", key)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .map_err(|error| format!("request failed: {error}"))?;
    let status = response.status();
    let body = response
        .text()
        .map_err(|error| format!("failed to read response: {error}"))?;
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
            items
                .iter()
                .take(limit)
                .filter_map(|item| {
                    let url = item.get("url")?.as_str()?.to_string();
                    if !url.starts_with("http") {
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
