//! Streaming response parser for OpenAI-compatible chat completions.
//! Exports: parse_streaming_response().
//! Deps: reqwest blocking response, Usage type, cleanup and HTTP footer helpers.

use std::io::{BufRead, BufReader, Write};

use crate::types::{SummarizeResult, Usage};

pub(super) fn parse_streaming_response(
    resp: reqwest::blocking::Response,
    raw_chars: u64,
    model_display: &str,
    cost_label: &str,
) -> SummarizeResult {
    let mut reader = BufReader::new(resp);
    let out = std::io::stdout();
    let mut out = out.lock();
    let mut text_parts = Vec::new();
    let mut usage = None;
    let mut line = String::new();
    while read_stream_line(&mut reader, &mut line, &mut text_parts, &mut usage, &mut out) {}
    finalize_stream(text_parts, usage, raw_chars, model_display, cost_label, out)
}

fn read_stream_line<W: Write>(
    reader: &mut BufReader<reqwest::blocking::Response>,
    line: &mut String,
    text_parts: &mut Vec<String>,
    usage: &mut Option<Usage>,
    out: &mut W,
) -> bool {
    line.clear();
    let Ok(bytes) = reader.read_line(line) else {
        return false;
    };
    if bytes == 0 {
        return false;
    }
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    parse_stream_payload(trimmed, text_parts, usage, out)
}

fn parse_stream_payload<W: Write>(
    trimmed: &str,
    text_parts: &mut Vec<String>,
    usage: &mut Option<Usage>,
    out: &mut W,
) -> bool {
    let Some(payload_line) = trimmed.strip_prefix("data: ") else {
        return true;
    };
    if payload_line == "[DONE]" {
        return false;
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(payload_line) else {
        return true;
    };
    if let Some(text) = stream_text(&json) {
        text_parts.push(text.to_string());
        write!(out, "{text}").ok();
        out.flush().ok();
    }
    if usage.is_none() {
        *usage = parse_usage(&json);
    }
    true
}

fn stream_text(json: &serde_json::Value) -> Option<&str> {
    let choice = json
        .get("choices")
        .and_then(|value| value.as_array())
        .and_then(|choices| choices.first())?;
    choice
        .get("delta")
        .and_then(|delta| delta.get("content"))
        .and_then(|value| value.as_str())
        .or_else(|| {
            choice
                .get("message")
                .and_then(|message| message.get("content"))
                .and_then(|value| value.as_str())
        })
}

fn parse_usage(json: &serde_json::Value) -> Option<Usage> {
    let usage_val = json.get("usage")?;
    Some(Usage {
        prompt_tokens: usage_val.get("prompt_tokens")?.as_u64()? as u32,
        completion_tokens: usage_val.get("completion_tokens")?.as_u64()? as u32,
    })
}

fn finalize_stream<W: Write>(
    text_parts: Vec<String>,
    usage: Option<Usage>,
    raw_chars: u64,
    model_display: &str,
    cost_label: &str,
    mut out: W,
) -> SummarizeResult {
    let response = text_parts.join("");
    if response.is_empty() {
        return empty_summary(raw_chars, usage);
    }
    let content = super::clean::strip_thinking(&response);
    let summary_chars = content.len() as u64;
    let footer = super::http::summary_footer(
        model_display,
        cost_label,
        raw_chars,
        summary_chars,
        usage.as_ref(),
    );
    write!(out, "{footer}").ok();
    out.flush().ok();
    SummarizeResult {
        summary_chars,
        text: String::new(),
        usage,
        raw_chars,
    }
}

fn empty_summary(raw_chars: u64, usage: Option<Usage>) -> SummarizeResult {
    SummarizeResult {
        text: String::new(),
        usage,
        raw_chars,
        summary_chars: 0,
    }
}
