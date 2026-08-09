//! opencode CLI summarization backend.
//! Exports: has_opencode(), summarize_opencode().
//! Deps: opencode executable, stats formatting, response cleanup.

use std::io::{BufRead, BufReader, Read, Write};

use crate::stats::fmtn;
use crate::types::{cli_failure, SummarizeResult, Usage};

pub(crate) fn has_opencode() -> bool {
    std::process::Command::new("opencode")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub(super) fn summarize_opencode(
    model: &str,
    prompt: &str,
    content: &str,
    streaming: bool,
) -> Result<SummarizeResult, String> {
    let raw_chars = content.len() as u64;
    let full_prompt = opencode_prompt(prompt, content);
    if streaming {
        stream_opencode(model, &full_prompt, raw_chars)
    } else {
        batch_opencode(model, &full_prompt, raw_chars)
    }
}

fn opencode_prompt(prompt: &str, content: &str) -> String {
    if prompt.is_empty() {
        return format!(
            "You are a research assistant. Summarize the provided content concisely and accurately.\n\
             Rules: Use bullet points. Include specific facts, numbers, code examples if relevant.\n\
             Keep under 500 words. Output ONLY the summary, no preamble. Do not use any tools.\n\n\
             ---\n\n{content}"
        );
    }
    format!(
        "You are a research assistant. Answer the question based on the provided content.\n\
         Rules: Use bullet points. Include specific facts, numbers, code examples if relevant.\n\
         Cite source numbers [1], [2] when multiple sources are given.\n\
         Keep under 500 words. Respond in the same language as the query.\n\
         Output ONLY the summary, no preamble. Do not use any tools.\n\n\
         Question: {prompt}\n\nContent:\n\n---\n\n{content}\n\n---"
    )
}

fn stream_opencode(model: &str, prompt: &str, raw_chars: u64) -> Result<SummarizeResult, String> {
    let mut child = std::process::Command::new("opencode")
        .args(["run", "-m", model, "--dir", "/tmp", "--format", "json", prompt])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start: {error}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture stdout".to_string())?;
    let mut reader = BufReader::new(stdout);
    let out = std::io::stdout();
    let mut out = out.lock();
    let mut text_parts = Vec::new();
    let mut usage = None;
    let mut line = String::new();
    while read_opencode_line(&mut reader, &mut line, &mut text_parts, &mut usage, &mut out) {}

    let stderr = child
        .stderr
        .take()
        .map(|mut stream| {
            let mut stderr = String::new();
            let _ = stream.read_to_string(&mut stderr);
            stderr
        })
        .unwrap_or_default();
    let status = child
        .wait()
        .map_err(|error| format!("failed to wait: {error}"))?;
    if !status.success() {
        return Err(cli_failure(&status, &stderr, ""));
    }
    let result = build_result(model, text_parts.join(""), usage, raw_chars, true)
        .ok_or_else(|| "completed successfully but returned no response".to_string())?;
    write!(out, "{}", result.text).ok();
    out.flush().ok();
    Ok(SummarizeResult {
        text: String::new(),
        ..result
    })
}

fn batch_opencode(model: &str, prompt: &str, raw_chars: u64) -> Result<SummarizeResult, String> {
    let output = std::process::Command::new("opencode")
        .args(["run", "-m", model, "--dir", "/tmp", "--format", "json", prompt])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("failed to start: {error}"))?;
    if !output.status.success() {
        return Err(cli_failure(
            &output.status,
            &String::from_utf8_lossy(&output.stderr),
            "",
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (response, usage) = parse_opencode_json_lines(&stdout);
    build_result(model, response, usage, raw_chars, false)
        .ok_or_else(|| "completed successfully but returned no response".to_string())
}

fn read_opencode_line<R: BufRead, W: Write>(
    reader: &mut R,
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
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return true;
    }
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        handle_opencode_event(&json, text_parts, usage, Some(out));
    }
    true
}

fn parse_opencode_json_lines(stdout: &str) -> (String, Option<Usage>) {
    let mut text_parts = Vec::new();
    let mut usage = None;
    for line in stdout.lines() {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        handle_opencode_event::<Vec<u8>>(&json, &mut text_parts, &mut usage, None);
    }
    (text_parts.join(""), usage)
}

fn handle_opencode_event<W: Write>(
    json: &serde_json::Value,
    text_parts: &mut Vec<String>,
    usage: &mut Option<Usage>,
    out: Option<&mut W>,
) {
    match json.get("type").and_then(|value| value.as_str()) {
        Some("text") => push_opencode_text(json, text_parts, out),
        Some("step_finish") => *usage = opencode_usage(json),
        _ => {}
    }
}

fn push_opencode_text<W: Write>(
    json: &serde_json::Value,
    text_parts: &mut Vec<String>,
    out: Option<&mut W>,
) {
    let Some(text) = json
        .get("part")
        .and_then(|part| part.get("text"))
        .and_then(|value| value.as_str())
    else {
        return;
    };
    text_parts.push(text.to_string());
    if let Some(out) = out {
        write!(out, "{text}").ok();
        out.flush().ok();
    }
}

fn opencode_usage(json: &serde_json::Value) -> Option<Usage> {
    let tokens = json.get("part").and_then(|part| part.get("tokens"))?;
    Some(Usage {
        prompt_tokens: tokens.get("input")?.as_u64().unwrap_or(0) as u32,
        completion_tokens: tokens.get("output")?.as_u64().unwrap_or(0) as u32,
    })
}

fn build_result(
    model: &str,
    response: String,
    usage: Option<Usage>,
    raw_chars: u64,
    streaming: bool,
) -> Option<SummarizeResult> {
    if response.is_empty() {
        return None;
    }
    let response = super::clean::strip_thinking(&response);
    let summary_chars = response.len() as u64;
    let footer = footer(model, usage.as_ref(), raw_chars, summary_chars);
    let text = if streaming {
        footer
    } else {
        format!("{response}{footer}")
    };
    Some(SummarizeResult {
        text,
        usage,
        raw_chars,
        summary_chars,
    })
}

fn footer(model: &str, usage: Option<&Usage>, raw_chars: u64, summary_chars: u64) -> String {
    let model_short = model.strip_prefix("opencode/").unwrap_or(model);
    let usage_info = usage
        .map(|usage| {
            format!(
                "tokens: {}+{} (free)",
                usage.prompt_tokens, usage.completion_tokens
            )
        })
        .unwrap_or_else(|| "free".to_string());
    format!(
        "\n\n---\n[ai-summary] {model_short} | {usage_info} | {raw_chars_fmt} -> {summary_chars_fmt}",
        raw_chars_fmt = fmtn(raw_chars),
        summary_chars_fmt = fmtn(summary_chars),
    )
}
