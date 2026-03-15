// LLM summarization via opencode CLI or OpenAI-compatible APIs.
// Exports: OPENCODE_MODEL, has_opencode(), llm_summarize_opencode(), llm_summarize(), strip_thinking().
// Deps: crate::config::Config, crate::stats::fmtn, crate::types, reqwest blocking client.

use reqwest::blocking::Client;
use std::io::{BufRead, BufReader, Write};

use crate::config::Config;
use crate::stats::fmtn;
use crate::types::{ChatResponse, SummarizeResult, Usage};

pub const OPENCODE_MODEL: &str = "opencode/nemotron-3-super-free";

pub fn has_opencode() -> bool {
    std::process::Command::new("opencode")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn llm_summarize_opencode(prompt: &str, content: &str, streaming: bool) -> Option<SummarizeResult> {
    let raw_chars = content.len() as u64;
    let full_prompt = if prompt.is_empty() {
        format!(
            "You are a research assistant. Summarize the provided content concisely and accurately.\n\
             Rules: Use bullet points. Include specific facts, numbers, code examples if relevant.\n\
             Keep under 500 words. Output ONLY the summary, no preamble. Do not use any tools.\n\n\
             ---\n\n{content}"
        )
    } else {
        format!(
            "You are a research assistant. Answer the question based on the provided content.\n\
             Rules: Use bullet points. Include specific facts, numbers, code examples if relevant.\n\
             Cite source numbers [1], [2] when multiple sources are given.\n\
             Keep under 500 words. Respond in the same language as the query.\n\
             Output ONLY the summary, no preamble. Do not use any tools.\n\n\
             Question: {prompt}\n\nContent:\n\n---\n\n{content}\n\n---"
        )
    };
    let mut text_parts = Vec::new();
    let mut usage = None;

    if streaming {
        let mut child = std::process::Command::new("opencode")
            .args([
                "run",
                "-m",
                OPENCODE_MODEL,
                "--dir",
                "/tmp",
                "--format",
                "json",
                &full_prompt,
            ])
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .ok()?;

        let stdout = child.stdout.take()?;
        let mut reader = BufReader::new(stdout);
        let out = std::io::stdout();
        let mut out = out.lock();
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = line.trim_end();
                    if trimmed.is_empty() {
                        continue;
                    }
                    let json: serde_json::Value = match serde_json::from_str(trimmed) {
                        Ok(value) => value,
                        Err(_) => continue,
                    };
                    match json.get("type").and_then(|value| value.as_str()) {
                        Some("text") => {
                            if let Some(text) = json
                                .get("part")
                                .and_then(|part| part.get("text"))
                                .and_then(|value| value.as_str())
                            {
                                text_parts.push(text.to_string());
                                write!(out, "{text}").ok();
                                out.flush().ok();
                            }
                        }
                        Some("step_finish") => {
                            if let Some(tokens) = json.get("part").and_then(|part| part.get("tokens")) {
                                let input = tokens
                                    .get("input")
                                    .and_then(|value| value.as_u64())
                                    .unwrap_or(0);
                                let output = tokens
                                    .get("output")
                                    .and_then(|value| value.as_u64())
                                    .unwrap_or(0);
                                usage = Some(Usage {
                                    prompt_tokens: input as u32,
                                    completion_tokens: output as u32,
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Err(_) => break,
            }
        }

        let status = child.wait().ok()?;
        if !status.success() {
            return None;
        }
        let response = text_parts.join("");
        if response.is_empty() {
            return None;
        }
        let content = strip_thinking(&response);
        let summary_chars = content.len() as u64;
        let model_short = OPENCODE_MODEL
            .strip_prefix("opencode/")
            .unwrap_or(OPENCODE_MODEL);
        let usage_info = usage
            .as_ref()
            .map(|usage| {
                format!(
                    "tokens: {}+{} (free)",
                    usage.prompt_tokens, usage.completion_tokens
                )
            })
            .unwrap_or_else(|| "free".to_string());
        let footer = format!(
            "\n\n---\n[ai-summary] {model_short} | {usage_info} | {raw_chars_fmt} -> {summary_chars_fmt}",
            raw_chars_fmt = fmtn(raw_chars),
            summary_chars_fmt = fmtn(summary_chars),
        );
        write!(out, "{footer}").ok();
        out.flush().ok();
        return Some(SummarizeResult {
            text: String::new(),
            usage,
            raw_chars,
            summary_chars,
        });
    }

    let output = std::process::Command::new("opencode")
        .args([
            "run",
            "-m",
            OPENCODE_MODEL,
            "--dir",
            "/tmp",
            "--format",
            "json",
            &full_prompt,
        ])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let json: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        match json.get("type").and_then(|value| value.as_str()) {
            Some("text") => {
                if let Some(text) = json
                    .get("part")
                    .and_then(|part| part.get("text"))
                    .and_then(|value| value.as_str())
                {
                    text_parts.push(text.to_string());
                }
            }
            Some("step_finish") => {
                if let Some(tokens) = json.get("part").and_then(|part| part.get("tokens")) {
                    let input = tokens
                        .get("input")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0);
                    let output = tokens
                        .get("output")
                        .and_then(|value| value.as_u64())
                        .unwrap_or(0);
                    usage = Some(Usage {
                        prompt_tokens: input as u32,
                        completion_tokens: output as u32,
                    });
                }
            }
            _ => {}
        }
    }

    let response = text_parts.join("");
    if response.is_empty() {
        return None;
    }
    let response = strip_thinking(&response);
    let summary_chars = response.len() as u64;
    let model_short = OPENCODE_MODEL
        .strip_prefix("opencode/")
        .unwrap_or(OPENCODE_MODEL);
    let usage_info = usage
        .as_ref()
        .map(|usage| {
            format!(
                "tokens: {}+{} (free)",
                usage.prompt_tokens, usage.completion_tokens
            )
        })
        .unwrap_or_else(|| "free".to_string());

    Some(SummarizeResult {
        text: format!(
            "{response}\n\n---\n[ai-summary] {model_short} | {usage_info} | {raw_chars_fmt} -> {summary_chars_fmt}",
            raw_chars_fmt = fmtn(raw_chars),
            summary_chars_fmt = fmtn(summary_chars),
        ),
        usage,
        raw_chars,
        summary_chars,
    })
}

pub fn llm_summarize(
    client: &Client,
    cfg: &Config,
    prompt: &str,
    content: &str,
    streaming: bool,
) -> SummarizeResult {
    let raw_chars = content.len() as u64;

    if has_opencode() {
        let model_short = OPENCODE_MODEL
            .strip_prefix("opencode/")
            .unwrap_or(OPENCODE_MODEL);
        eprintln!("Summarizing with {model_short} (opencode, free)...");
        if let Some(result) = llm_summarize_opencode(prompt, content, streaming) {
            return result;
        }
        eprintln!(
            "[ai-summary] opencode failed, falling back to {}...",
            cfg.model
        );
    }

    let is_local = cfg.api_url.contains("127.0.0.1") || cfg.api_url.contains("localhost");
    eprintln!("Summarizing with {} ({})...", cfg.model, cfg.api_url);

    let system =
        "You are a research assistant. Summarize the provided content concisely and accurately.\n\
        Rules:\n\
        - Answer the user's question directly if one is provided\n\
        - Include specific facts, numbers, code examples if relevant\n\
        - Cite source numbers [1], [2] when multiple sources are given\n\
        - Use bullet points for clarity\n\
        - Keep under 500 words\n\
        - Respond in the same language as the query\n\
        - Output ONLY the final summary, no preamble.";

    let user_msg = if prompt.is_empty() {
        format!("Summarize the following content:\n\n---\n\n{content}")
    } else {
        format!(
            "Question: {prompt}\n\nContent:\n\n---\n\n{content}\n\n---\n\nProvide a concise summary."
        )
    };

    let payload = serde_json::json!({
        "model": cfg.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user_msg}
        ],
        "max_tokens": cfg.max_summary_tokens,
        "temperature": 0.3,
        "chat_template_kwargs": {"enable_thinking": false},
        "stream": streaming
    });

    let mut req = client
        .post(format!("{}/v1/chat/completions", cfg.api_url))
        .json(&payload);
    if !cfg.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", cfg.api_key));
    }

    match req.timeout(std::time::Duration::from_secs(120)).send() {
        Ok(resp) => {
            if streaming {
                let mut reader = BufReader::new(resp);
                let out = std::io::stdout();
                let mut out = out.lock();
                let mut text_parts = Vec::new();
                let mut usage = None;
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break,
                        Ok(_) => {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                continue;
                            }
                            if let Some(payload_line) = trimmed.strip_prefix("data: ") {
                                if payload_line == "[DONE]" {
                                    break;
                                }
                                let json: serde_json::Value =
                                    match serde_json::from_str(payload_line) {
                                        Ok(value) => value,
                                        Err(_) => continue,
                                    };
                                if let Some(choices) = json.get("choices").and_then(|value| {
                                    value.as_array()
                                }) {
                                    if let Some(choice) = choices.first() {
                                        let mut chunk_text = None;
                                        if let Some(delta) = choice.get("delta") {
                                            if let Some(content) = delta
                                                .get("content")
                                                .and_then(|value| value.as_str())
                                            {
                                                chunk_text = Some(content.to_string());
                                            }
                                        }
                                        if chunk_text.is_none() {
                                            if let Some(message) = choice.get("message") {
                                                if let Some(content) = message
                                                    .get("content")
                                                    .and_then(|value| value.as_str())
                                                {
                                                    chunk_text = Some(content.to_string());
                                                }
                                            }
                                        }
                                        if let Some(text) = chunk_text {
                                            text_parts.push(text.clone());
                                            write!(out, "{text}").ok();
                                            out.flush().ok();
                                        }
                                    }
                                }
                                if usage.is_none() {
                                    if let Some(usage_val) = json.get("usage") {
                                        if let Some(prompt_tokens) = usage_val
                                            .get("prompt_tokens")
                                            .and_then(|value| value.as_u64())
                                        {
                                            if let Some(completion_tokens) = usage_val
                                                .get("completion_tokens")
                                                .and_then(|value| value.as_u64())
                                            {
                                                usage = Some(Usage {
                                                    prompt_tokens: prompt_tokens as u32,
                                                    completion_tokens: completion_tokens as u32,
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(_) => break,
                    }
                }
                let response = text_parts.join("");
                if response.is_empty() {
                    return SummarizeResult {
                        text: String::new(),
                        usage,
                        raw_chars,
                        summary_chars: 0,
                    };
                }
                let content = strip_thinking(&response);
                let summary_chars = content.len() as u64;
                let loc = if is_local { "free, local" } else { "API" };
                let usage_info = usage
                    .as_ref()
                    .map(|usage| {
                        format!(
                            "tokens: {}+{} ({})",
                            usage.prompt_tokens, usage.completion_tokens, loc
                        )
                    })
                    .unwrap_or_default();
                let footer = format!(
                    "\n\n---\n[ai-summary] {model} | {usage_info} | {raw_chars_fmt} -> {summary_chars_fmt}",
                    model = cfg.model,
                    raw_chars_fmt = fmtn(raw_chars),
                    summary_chars_fmt = fmtn(summary_chars),
                );
                write!(out, "{footer}").ok();
                out.flush().ok();
                SummarizeResult {
                    summary_chars,
                    text: String::new(),
                    usage,
                    raw_chars,
                }
            } else {
                #[allow(unused_mut)]
                let mut resp = resp;
                match resp.json::<ChatResponse>() {
                    Ok(response) => {
                        let raw = response
                            .choices
                            .first()
                            .map(|choice| choice.message.content.clone())
                            .unwrap_or_default();
                        let content = strip_thinking(&raw);
                        let loc = if is_local { "free, local" } else { "API" };
                        let usage_info = response
                            .usage
                            .as_ref()
                            .map(|usage| {
                                format!(
                                    "tokens: {}+{} ({})",
                                    usage.prompt_tokens, usage.completion_tokens, loc
                                )
                            })
                            .unwrap_or_default();
                        SummarizeResult {
                            summary_chars: content.len() as u64,
                            text: format!(
                                "{content}\n\n---\n[ai-summary] {model} | {usage_info} | {raw_chars_fmt} -> {summary_chars_fmt}",
                                model = cfg.model,
                                raw_chars_fmt = fmtn(raw_chars),
                                summary_chars_fmt = fmtn(content.len() as u64),
                            ),
                            usage: response.usage,
                            raw_chars,
                        }
                    }
                    Err(error) => SummarizeResult {
                        text: format!("[ai-summary] Parse error: {error}"),
                        usage: None,
                        raw_chars,
                        summary_chars: 0,
                    },
                }
            }
        }
        Err(error) => SummarizeResult {
            text: format!("[ai-summary] Connection error: {error}"),
            usage: None,
            raw_chars,
            summary_chars: 0,
        },
    }
}

pub fn strip_thinking(s: &str) -> String {
    let s = s.trim();
    if let Some(index) = s.find("</think>") {
        return s[index + 8..].trim().to_string();
    }
    let lines: Vec<&str> = s.lines().collect();
    let starters = [
        "Thinking Process",
        "**Thinking",
        "Let me think",
        "I'll analyze",
        "Let me analyze",
        "First, let me",
    ];
    let mut thinking = false;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if index < 3 && starters.iter().any(|starter| trimmed.starts_with(starter)) {
            thinking = true;
            continue;
        }
        if thinking
            && (trimmed.starts_with("## ")
                || trimmed.starts_with("# ")
                || trimmed == "---"
                || trimmed.starts_with("- **")
                || trimmed.starts_with("- [")
                || trimmed.starts_with("1.")
                || trimmed.starts_with("Here")
                || trimmed.starts_with("Based on")
                || (trimmed.starts_with("**")
                    && !starters.iter().any(|starter| trimmed.starts_with(starter))))
        {
            return lines[index..].join("\n").trim().to_string();
        }
    }
    s.to_string()
}
