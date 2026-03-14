// LLM summarization via opencode CLI or OpenAI-compatible APIs.
// Exports: OPENCODE_MODEL, has_opencode(), llm_summarize_opencode(), llm_summarize(), strip_thinking().
// Deps: crate::config::Config, crate::stats::fmtn, crate::types, reqwest blocking client.

use reqwest::blocking::Client;

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

pub fn llm_summarize_opencode(prompt: &str, content: &str) -> Option<SummarizeResult> {
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
    let mut text_parts = Vec::new();
    let mut usage = None;

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
) -> SummarizeResult {
    let raw_chars = content.len() as u64;

    if has_opencode() {
        let model_short = OPENCODE_MODEL
            .strip_prefix("opencode/")
            .unwrap_or(OPENCODE_MODEL);
        eprintln!("Summarizing with {model_short} (opencode, free)...");
        if let Some(result) = llm_summarize_opencode(prompt, content) {
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
        "chat_template_kwargs": {"enable_thinking": false}
    });

    let mut req = client
        .post(format!("{}/v1/chat/completions", cfg.api_url))
        .json(&payload);
    if !cfg.api_key.is_empty() {
        req = req.header("Authorization", format!("Bearer {}", cfg.api_key));
    }

    match req.timeout(std::time::Duration::from_secs(120)).send() {
        Ok(resp) => match resp.json::<ChatResponse>() {
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
        },
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
