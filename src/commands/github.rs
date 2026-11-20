//! Acquire GitHub file, directory, or code-search content via gh.
//! All subprocesses share a deadline and bounded capture.

use super::{emit_raw, fail, output, summary};
use crate::{config::Config, subprocess, types::cli_failure, Cli};
use serde_json::{json, Value};
use std::{
    process::Command,
    time::{Duration, Instant},
};

const DEADLINE: Duration = Duration::from_secs(45);

pub fn run_github(
    cli: &Cli,
    cfg: &Config,
    args: &[String],
    repo_flag: &Option<String>,
    language: &Option<String>,
    prompt: &Option<String>,
) {
    let started = Instant::now();
    let gh = Github {
        deadline: started + DEADLINE,
    };
    let read = if repo_flag.is_none() && language.is_none() {
        parse_repo_path(args)
    } else {
        None
    };
    let (content, mode, question) = if let Some((repo, path)) = read {
        (
            gh.read(&repo, &path),
            "github-read",
            "Explain this code and its key functions",
        )
    } else {
        (
            gh.search(&args.join(" "), repo_flag.as_deref(), language.as_deref()),
            "github-search",
            "Summarize the key patterns in these code search results",
        )
    };
    let content = content.unwrap_or_else(|error| fail(2, &error));
    if cli.raw {
        return emit_raw(cli, &content);
    }
    let question = prompt.as_deref().unwrap_or(question);
    let (answer, _) = summary::summarize(cli, cfg, mode, question, &content, false);
    let bytes = content.len() as u64;
    let (tokens, duration) = summary::record(&answer, question, mode, 1, bytes, started, false);
    output::emit(
        cli,
        &answer,
        bytes,
        json!({
            "mode": mode, "raw_chars": bytes, "tokens_saved": tokens, "duration_secs": duration,
        }),
    );
}

fn parse_repo_path(args: &[String]) -> Option<(String, String)> {
    let first = args.first()?;
    let mut parts = first.splitn(3, '/');
    let (owner, repo) = (parts.next()?, parts.next()?);
    let valid = |part: &str| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
    };
    if !valid(owner) || !valid(repo) {
        return None;
    }
    let path = parts
        .next()
        .into_iter()
        .chain(args[1..].iter().map(String::as_str))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    Some((format!("{owner}/{repo}"), path))
}

struct Github {
    deadline: Instant,
}

impl Github {
    fn search(
        &self,
        query: &str,
        repo: Option<&str>,
        language: Option<&str>,
    ) -> Result<String, String> {
        let mut command = Command::new("gh");
        command.args([
            "search",
            "code",
            "--limit",
            "20",
            "--json",
            "path,repository,textMatches",
        ]);
        if let Some(repo) = repo {
            command.args(["--repo", repo]);
        }
        if let Some(language) = language {
            command.args(["--language", language]);
        }
        let bytes = self.capture(command.args(["--", query]))?;
        let results: Vec<Value> =
            serde_json::from_slice(&bytes).map_err(|_| "invalid GitHub search response")?;
        if results.is_empty() {
            return Err("no GitHub code search results".into());
        }
        let mut text = String::new();
        for (index, result) in results.iter().enumerate() {
            let repo = result["repository"]["nameWithOwner"]
                .as_str()
                .unwrap_or("?");
            let path = result["path"].as_str().unwrap_or("?");
            text.push_str(&format!("### [{}] {repo}: {path}\n", index + 1));
            for item in result["textMatches"].as_array().into_iter().flatten() {
                if let Some(fragment) = item["fragment"].as_str() {
                    text.push_str(fragment);
                    text.push('\n');
                }
            }
            text.push('\n');
        }
        Ok(text)
    }

    fn read(&self, repo: &str, path: &str) -> Result<String, String> {
        let endpoint = contents_endpoint(repo, path)?;
        let metadata = self.capture(Command::new("gh").args(["api", &endpoint]))?;
        let value: Value =
            serde_json::from_slice(&metadata).map_err(|_| "invalid GitHub contents response")?;
        if let Some(entries) = value.as_array() {
            return Ok(directory(repo, path, entries));
        }
        let body = self.capture(Command::new("gh").args([
            "api",
            &endpoint,
            "-H",
            "Accept: application/vnd.github.raw+json",
        ]))?;
        let file_path = value["path"].as_str().unwrap_or(path);
        let size = value["size"].as_u64().unwrap_or(body.len() as u64);
        Ok(format!(
            "{repo}: {file_path} ({size} bytes)\n\n{}",
            String::from_utf8_lossy(&body)
        ))
    }

    fn capture(&self, command: &mut Command) -> Result<Vec<u8>, String> {
        let timeout = self
            .deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| "GitHub command deadline exceeded".to_string())?;
        let output = subprocess::capture(
            command.env("GH_PROMPT_DISABLED", "1"),
            None,
            timeout,
            subprocess::CONTENT_LIMIT,
        )?;
        if !output.status.success() {
            return Err(format!(
                "gh: {}",
                cli_failure(&output.status, &String::from_utf8_lossy(&output.stderr), "")
            ));
        }
        Ok(output.stdout)
    }
}

fn contents_endpoint(repo: &str, path: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse("https://api.github.com/").expect("fixed GitHub origin");
    let mut segments = url.path_segments_mut().expect("hierarchical GitHub URL");
    segments
        .push("repos")
        .extend(repo.split('/'))
        .push("contents");
    for segment in path.split('/').filter(|part| !part.is_empty()) {
        if matches!(segment, "." | "..") {
            return Err("invalid GitHub path component".into());
        }
        segments.push(segment);
    }
    drop(segments);
    Ok(url.path().trim_start_matches('/').to_owned())
}

fn directory(repo: &str, path: &str, entries: &[Value]) -> String {
    let mut text = format!("Directory: {repo}/{path}\n\n");
    for entry in entries {
        let name = entry["name"].as_str().unwrap_or("?");
        if entry["type"] == "dir" {
            text.push_str(&format!("  {name}/\n"));
        } else {
            let size = entry["size"].as_u64().unwrap_or(0);
            text.push_str(&format!("  {name}  ({size} bytes)\n"));
        }
    }
    text
}
