//! OpenAI-compatible HTTP summarization client.
//! Exports: summarize_http().
//! Deps: reqwest blocking client, Config-derived params, stats formatting, response types.

use reqwest::blocking::Client;

use crate::stats::fmtn;
use crate::types::{error_detail, ChatResponse, SummarizeResult, Usage};

pub(super) struct HttpSummaryRequest<'a> {
    pub api_url: &'a str,
    pub api_key: &'a str,
    pub model: &'a str,
    pub model_display: &'a str,
    pub max_tokens: u32,
    pub cost_label: &'a str,
    pub prompt: &'a str,
    pub content: &'a str,
    pub streaming: bool,
}

pub(super) fn summarize_http(client: &Client, input: HttpSummaryRequest<'_>) -> SummarizeResult {
    let raw_chars = input.content.len() as u64;
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

    let mut req = client
        .post(format!("{}/v1/chat/completions", input.api_url))
        .json(&payload);
    if !input.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", input.api_key));
    }

    match req.timeout(std::time::Duration::from_secs(120)).send() {
        Ok(resp) => parse_response(resp, raw_chars, &input),
        Err(error) => SummarizeResult {
            text: format!(
                "[ai-summary] Connection error: {}",
                error_detail(&error.to_string(), input.api_key)
            ),
            usage: None,
            raw_chars,
            summary_chars: 0,
        },
    }
}

fn parse_response(
    resp: reqwest::blocking::Response,
    raw_chars: u64,
    input: &HttpSummaryRequest<'_>,
) -> SummarizeResult {
    if !resp.status().is_success() {
        return parse_http_error(resp, raw_chars, input.api_key);
    }
    if input.streaming {
        super::stream::parse_streaming_response(
            resp,
            raw_chars,
            input.model_display,
            input.cost_label,
        )
    } else {
        parse_json_response(resp, raw_chars, input.model_display, input.cost_label)
    }
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

fn parse_http_error(
    resp: reqwest::blocking::Response,
    raw_chars: u64,
    api_key: &str,
) -> SummarizeResult {
    let status = resp.status();
    let body = resp.text().unwrap_or_default();
    let detail = error_detail(&body, api_key);
    SummarizeResult {
        text: format!("[ai-summary] HTTP {status}: {detail}"),
        usage: None,
        raw_chars,
        summary_chars: 0,
    }
}

fn parse_json_response(
    resp: reqwest::blocking::Response,
    raw_chars: u64,
    model_display: &str,
    cost_label: &str,
) -> SummarizeResult {
    match resp.json::<ChatResponse>() {
        Ok(response) => json_summary(response, raw_chars, model_display, cost_label),
        Err(error) => SummarizeResult {
            text: format!("[ai-summary] Parse error: {error}"),
            usage: None,
            raw_chars,
            summary_chars: 0,
        },
    }
}

fn json_summary(
    response: ChatResponse,
    raw_chars: u64,
    model_display: &str,
    cost_label: &str,
) -> SummarizeResult {
    let raw = response
        .choices
        .first()
        .map(|choice| choice.message.content.clone())
        .unwrap_or_default();
    let content = super::clean::strip_thinking(&raw);
    let summary_chars = content.len() as u64;
    let footer = summary_footer(
        model_display,
        cost_label,
        raw_chars,
        summary_chars,
        response.usage.as_ref(),
    );
    SummarizeResult {
        text: format!("{content}{footer}"),
        usage: response.usage,
        raw_chars,
        summary_chars,
    }
}

pub(super) fn summary_footer(
    model_display: &str,
    cost_label: &str,
    raw_chars: u64,
    summary_chars: u64,
    usage: Option<&Usage>,
) -> String {
    let usage_info = usage
        .map(|usage| {
            format!(
                "tokens: {}+{} ({})",
                usage.prompt_tokens, usage.completion_tokens, cost_label
            )
        })
        .unwrap_or_default();
    format!(
        "\n\n---\n[ai-summary] {model_display} | {usage_info} | {raw_chars_fmt} -> {summary_chars_fmt}",
        raw_chars_fmt = fmtn(raw_chars),
        summary_chars_fmt = fmtn(summary_chars),
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
