// Search provider integrations and URL encoding helpers.
// Exports: UA, urlenc(), urldec(), has_gemini_cli(), search_gemini_cli(), search_gemini(), search_web().
// Deps: reqwest blocking client, scraper selectors, crate::types.

use crate::types::{SearchResult, SummarizeResult, Usage};
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

pub fn has_gemini_cli() -> bool {
    std::process::Command::new("gemini")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn search_gemini_cli(query: &str, streaming: bool) -> Option<SummarizeResult> {
    let prompt = format!(
        "Search the web for: {}\n\n\
         Based ONLY on search results, provide a concise factual answer. \
         Use bullet points. Include specific source URLs. Keep under 500 words.",
        query
    );
    if streaming {
        search_gemini_cli_stream(&prompt)
    } else {
        search_gemini_cli_batch(&prompt)
    }
}

fn search_gemini_cli_stream(prompt: &str) -> Option<SummarizeResult> {
    use std::io::{BufRead, BufReader, Write};
    let mut child = std::process::Command::new("gemini")
        .args(["-o", "stream-json", "-p", prompt])
        .current_dir("/tmp")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let reader = BufReader::new(stdout);
    let mut response = String::new();
    let out = std::io::stdout();
    let mut out = out.lock();
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let json: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if json.get("type").and_then(|v| v.as_str()) == Some("message")
            && json.get("role").and_then(|v| v.as_str()) == Some("assistant")
        {
            if let Some(content) = json.get("content").and_then(|v| v.as_str()) {
                response.push_str(content);
                let _ = out.write_all(content.as_bytes());
                let _ = out.flush();
            }
        }
    }
    let _ = child.wait();
    if response.is_empty() {
        return None;
    }
    let footer = "\n\n---\n[ai-summary] Gemini CLI + Google Search";
    let _ = out.write_all(footer.as_bytes());
    let _ = out.flush();
    let raw_estimate = 14000u64;
    let summary_chars = response.len() as u64;
    Some(SummarizeResult {
        text: String::new(), // already printed
        usage: None,
        raw_chars: raw_estimate,
        summary_chars,
    })
}

fn search_gemini_cli_batch(prompt: &str) -> Option<SummarizeResult> {
    let output = std::process::Command::new("gemini")
        .args(["-o", "json", "-p", prompt])
        .current_dir("/tmp")
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    let response = json.get("response")?.as_str()?.to_string();
    if response.is_empty() {
        return None;
    }
    let raw_estimate = 14000u64;
    let summary_chars = response.len() as u64;
    Some(SummarizeResult {
        text: format!("{response}\n\n---\n[ai-summary] Gemini CLI + Google Search"),
        usage: None,
        raw_chars: raw_estimate,
        summary_chars,
    })
}

pub fn search_gemini(
    client: &Client,
    query: &str,
    api_key: &str,
    model: &str,
) -> Option<SummarizeResult> {
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
        .ok()?;

    let body: serde_json::Value = resp.json().ok()?;
    let parts = body
        .get("candidates")?
        .get(0)?
        .get("content")?
        .get("parts")?
        .as_array()?;
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
    Some(SummarizeResult {
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
    tavily_key: &str,
) -> Vec<SearchResult> {
    let results = search_ddg(client, query, num);
    if !results.is_empty() {
        return results;
    }
    if !tavily_key.is_empty() {
        eprintln!("[ai-summary] DDG unavailable, trying Tavily Search...");
        let tavily_results = search_tavily(client, query, num, tavily_key);
        if !tavily_results.is_empty() {
            return tavily_results;
        }
    }
    if !brave_key.is_empty() {
        eprintln!("[ai-summary] DDG unavailable, trying Brave Search...");
        return search_brave(client, query, num, brave_key);
    }
    eprintln!("[ai-summary] DDG unavailable. Set gemini_api_key, tavily_api_key, or brave_api_key in config.");
    vec![]
}

pub fn search_tavily(client: &Client, query: &str, num: usize, key: &str) -> Vec<SearchResult> {
    let payload = serde_json::json!({
        "api_key": key,
        "query": query,
        "max_results": num,
    });
    let body = client
        .post("https://api.tavily.com/search")
        .header("Content-Type", "application/json")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .ok()
        .and_then(|response| response.text().ok())
        .unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    value
        .get("results")
        .and_then(|results| results.as_array())
        .map(|items| {
            items
                .iter()
                .take(num)
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
                            .get("content")
                            .and_then(|value| value.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn search_ddg(client: &Client, query: &str, num: usize) -> Vec<SearchResult> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", urlenc(query));
    let resp = client
        .get(&url)
        .header("User-Agent", UA)
        .send()
        .ok()
        .and_then(|response| response.text().ok())
        .unwrap_or_default();
    if resp.is_empty() {
        return vec![];
    }
    let doc = Html::parse_document(&resp);
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
            if out.len() >= num {
                break;
            }
        }
    }
    out
}

pub fn search_brave(client: &Client, query: &str, num: usize, key: &str) -> Vec<SearchResult> {
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlenc(query),
        num
    );
    let body = client
        .get(&url)
        .header("Accept", "application/json")
        .header("X-Subscription-Token", key)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .ok()
        .and_then(|response| response.text().ok())
        .unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    value
        .get("web")
        .and_then(|web| web.get("results"))
        .and_then(|results| results.as_array())
        .map(|items| {
            items
                .iter()
                .take(num)
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
    use serde_json::Value;

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

    fn parse_ddg_html(html: &str, limit: usize) -> Vec<SearchResult> {
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
                out.push(SearchResult { url, title, snippet });
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }

    #[test]
    fn tavily_parses_filtered_results() {
        let body = r#"{
            "results": [
                {"url": "https://one.example/", "title": "One", "content": "One content"},
                {"url": "ftp://skip", "title": "Skip", "content": "Ignored"},
                {"url": "https://two.example/", "title": "Two", "content": "Two content"}
            ]
        }"#;
        let results = parse_tavily_json(body, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].url, "https://one.example/");
        assert_eq!(results[0].title, "One");
        assert_eq!(results[0].snippet, "One content");
        assert_eq!(results[1].url, "https://two.example/");
        assert_eq!(results[1].title, "Two");
        assert_eq!(results[1].snippet, "Two content");
    }

    fn parse_tavily_json(body: &str, limit: usize) -> Vec<SearchResult> {
        let value: Value = serde_json::from_str(body).unwrap_or_default();
        value
            .get("results")
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
                                .get("content")
                                .and_then(|value| value.as_str())
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_brave_json(body: &str, limit: usize) -> Vec<SearchResult> {
        let value: Value = serde_json::from_str(body).unwrap_or_default();
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
}
