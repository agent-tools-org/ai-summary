//! Run opencode as a subprocess and decode its JSON-lines summary.
//! Returns a completed result and leaves rendering to the caller.

use super::{Provider, Summary};
use crate::types::{cli_failure, Usage};

use crate::subprocess;
use std::{process::Command, time::Duration};

pub(crate) fn has_opencode() -> bool {
    subprocess::available(Command::new("opencode").arg("--version"))
}

pub(super) fn summarize_opencode(
    model: &str,
    prompt: &str,
    content: &str,
    timeout: Duration,
) -> Result<Summary, String> {
    let full_prompt = opencode_prompt(prompt, content);
    let workdir = tempfile::tempdir().map_err(|_| "cannot create opencode working directory")?;
    let mut command = Command::new("opencode");
    command
        .args(["run", "-m", model, "--format", "json", "--dir"])
        .arg(workdir.path())
        .env("OPENCODE_PERMISSION", r#"{"*":"deny"}"#);
    let output = subprocess::capture(
        &mut command,
        Some(full_prompt.as_bytes()),
        timeout,
        subprocess::CONTENT_LIMIT,
    )?;
    if !output.status.success() {
        return Err(cli_failure(
            &output.status,
            &String::from_utf8_lossy(&output.stderr),
            "",
        ));
    }
    let (text, usage) = parse_opencode_json_lines(&String::from_utf8_lossy(&output.stdout))?;
    Summary::new(&text, usage, Provider::Opencode, model)
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

fn parse_opencode_json_lines(stdout: &str) -> Result<(String, Option<Usage>), String> {
    let mut text = String::new();
    let mut usage = None;
    for line in stdout.lines() {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match event["type"].as_str() {
            Some("text") => text.push_str(event["part"]["text"].as_str().unwrap_or_default()),
            Some("step_finish") => usage = opencode_usage(&event).or(usage),
            Some("error") => return Err("opencode emitted an error event".into()),
            _ => {}
        }
    }
    Ok((text, usage))
}

fn opencode_usage(json: &serde_json::Value) -> Option<Usage> {
    let tokens = json.get("part").and_then(|part| part.get("tokens"))?;
    Some(Usage {
        prompt_tokens: tokens.get("input")?.as_u64().unwrap_or(0) as u32,
        completion_tokens: tokens.get("output")?.as_u64().unwrap_or(0) as u32,
    })
}
