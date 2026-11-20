//! Pack a remote repository through repomix and render or summarize it.
//! Caps captured output and avoids installing a runtime dependency implicitly.

use super::{emit_raw, fail, output, summary};
use crate::{config::Config, subprocess, types::cli_failure, Cli};
use serde_json::json;
use std::{
    process::Command,
    time::{Duration, Instant},
};

pub fn run_repo(
    cli: &Cli,
    cfg: &Config,
    repo: &str,
    prompt: &Option<String>,
    include: Option<&str>,
) {
    let started = Instant::now();
    if repo.starts_with('-') || repo.trim().is_empty() {
        fail(1, "expected a GitHub repository name or URL");
    }
    let packed = pack(repo, include).unwrap_or_else(|error| fail(2, &error));
    if cli.raw {
        return emit_raw(cli, &packed);
    }
    let question = prompt.as_deref().unwrap_or(
        "Summarize this repository: its purpose, architecture, key modules, and tech stack",
    );
    let (answer, _) = summary::summarize(cli, cfg, "repo", question, &packed, false);
    let bytes = packed.len() as u64;
    let (tokens, duration) = summary::record(&answer, question, "repo", 1, bytes, started, false);
    output::emit(
        cli,
        &answer,
        bytes,
        json!({
            "repo": repo, "raw_chars": bytes, "tokens_saved": tokens, "duration_secs": duration,
        }),
    );
}

fn pack(repo: &str, include: Option<&str>) -> Result<String, String> {
    let mut command = Command::new("repomix");
    command.args(["--remote", repo, "--compress", "--stdout"]);
    if let Some(patterns) = include {
        command.args(["--include", patterns]);
    }
    let output = subprocess::capture(
        &mut command,
        None,
        Duration::from_secs(90),
        subprocess::CONTENT_LIMIT,
    )
    .map_err(|error| format!("repomix: {error} (install with npm install -g repomix)"))?;
    if !output.status.success() {
        return Err(format!(
            "repomix: {}",
            cli_failure(&output.status, &String::from_utf8_lossy(&output.stderr), "")
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        return Err("repomix returned no content".into());
    }
    Ok(text)
}
