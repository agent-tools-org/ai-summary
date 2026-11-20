//! Issue OpenAI-compatible summary requests and decode complete responses.
//! Uses the summary client's no-redirect policy and SSE response parser.

use reqwest::{blocking::Client, Url};

use super::{Provider, Summary};
use crate::types::{error_detail, ChatResponse};

pub(super) struct HttpSummaryRequest<'a> {
    pub provider: Provider,
    pub url: &'a str,
    pub key: &'a str,
    pub model: &'a str,
    pub max_tokens: u32,
    pub prompt: &'a str,
    pub content: &'a str,
    pub streaming: bool,
    pub timeout: std::time::Duration,
}

pub(super) fn summarize_http(
    client: &Client,
    input: HttpSummaryRequest<'_>,
) -> Result<Summary, String> {
    let url = endpoint(input.url)?;
    let payload = serde_json::json!({
        "model": input.model,
        "messages": [
            {"role": "system", "content": system_prompt()},
            {"role": "user", "content": user_message(input.prompt, input.content)}
        ],
        "max_tokens": input.max_tokens,
        "temperature": 0.3,
        "chat_template_kwargs": {"enable_thinking": false},
        "stream": input.streaming
    });
    let mut request = client.post(url).json(&payload);
    if !input.key.is_empty() {
        request = request.bearer_auth(input.key);
    }
    let response = request
        .timeout(input.timeout)
        .send()
        .map_err(|error| format!("connection error: {}", error.without_url()))?;
    let status = response.status();
    if !status.is_success() {
        let body =
            String::from_utf8_lossy(&crate::read_limit(response, 64 * 1024).unwrap_or_default())
                .into_owned();
        return Err(format!("HTTP {status}: {}", error_detail(&body, input.key)));
    }
    if input.streaming {
        let (text, usage) =
            super::stream::parse_streaming_response(&*crate::read_limit(response, 1024 * 1024)?)
                .map_err(|error| error_detail(&error, input.key))?;
        return Summary::new(&text, usage, input.provider, input.model);
    }
    let response: ChatResponse = serde_json::from_slice(&crate::read_limit(response, 1024 * 1024)?)
        .map_err(|_| "invalid summary JSON")?;
    let text = response
        .choices
        .first()
        .map(|choice| choice.message.content.as_str())
        .unwrap_or_default();
    Summary::new(text, response.usage, input.provider, input.model)
}

fn endpoint(base: &str) -> Result<Url, String> {
    let mut url = Url::parse(base).map_err(|_| "invalid summary API URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("invalid summary API URL".into());
    }
    let path = url.path().trim_end_matches('/');
    let endpoint = if path.ends_with("/v1") {
        format!("{path}/chat/completions")
    } else {
        format!("{path}/v1/chat/completions")
    };
    url.set_path(&endpoint);
    Ok(url)
}

fn system_prompt() -> &'static str {
    "You are a research assistant. Summarize the provided content concisely and accurately.\n\
    Rules:\n\
    - Answer the user's question directly if one is provided\n\
    - Include specific facts, numbers, code examples if relevant\n\
    - Cite source numbers [1], [2] when multiple sources are given\n\
    - Use bullet points for clarity\n\
    - Keep under 500 words\n\
    - Respond in the same language as the query\n\
    - Output ONLY the final summary, no preamble."
}

fn user_message(prompt: &str, content: &str) -> String {
    if prompt.is_empty() {
        return format!("Summarize the following content:\n\n---\n\n{content}");
    }
    format!(
        "Question: {prompt}\n\nContent:\n\n---\n\n{content}\n\n---\n\nProvide a concise summary."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_nested_provider_error_message() {
        let body = r#"{"error":{"message":"model not found"}}"#;
        assert_eq!(error_detail(body, ""), "model not found");
    }

    #[test]
    fn truncates_and_redacts_http_error_detail() {
        let body = format!("{{\"message\":\"{} secret-key\"}}", "x".repeat(300));
        let detail = error_detail(&body, "secret-key");
        assert!(detail.len() <= 240);
        assert!(!detail.contains("secret-key"));
    }
}
