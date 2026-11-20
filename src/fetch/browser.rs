//! Render a page in a temporary agent-browser session.
//! Probe installed options before starting a browser and suppress inherited config.

use super::{FetchedPage, MIN_BROWSER_TEXT_CHARS};
use crate::fetch_utils::{collapse_ws, strip_ansi};
use crate::{subprocess, types::error_detail};
use std::{path::Path, process::Command, sync::OnceLock, time::Duration};

fn capability() -> Result<(), String> {
    static DETECTED: OnceLock<Result<(), String>> = OnceLock::new();
    DETECTED
        .get_or_init(|| {
            let output = subprocess::capture(
                Command::new("agent-browser").arg("--help"),
                None,
                Duration::from_secs(3),
                32 * 1024,
            )?;
            let help = String::from_utf8_lossy(&output.stdout);
            if !output.status.success() || !help.contains("--config") || !help.contains("--session")
            {
                return Err(
                    "installed agent-browser lacks --config or --session; upgrade it".into(),
                );
            }
            Ok(())
        })
        .clone()
}

pub fn has_agent_browser() -> bool {
    capability().is_ok()
}

pub fn fetch_page_browser(url: &str, max_chars: usize) -> Result<FetchedPage, String> {
    capability()?;
    let session = BrowserSession::new()?;
    session.run(&["open", url], Duration::from_secs(13))?;
    let _ = session.run(&["wait", "--load", "networkidle"], Duration::from_secs(8));
    let mut text = session.text("main").unwrap_or_default();
    if text.len() < MIN_BROWSER_TEXT_CHARS {
        text = session.text("body").or_else(|error| {
            if text.is_empty() {
                Err(error)
            } else {
                Ok(text)
            }
        })?;
    }
    let final_url = session
        .run(&["get", "url"], Duration::from_secs(3))
        .ok()
        .and_then(|value| super::http::validate_url(value.trim()).ok());
    super::page(
        url,
        final_url.as_ref().map_or(url, reqwest::Url::as_str),
        "browser",
        "text/plain",
        text,
        max_chars,
    )
}

struct BrowserSession {
    directory: tempfile::TempDir,
    id: String,
}

impl BrowserSession {
    fn new() -> Result<Self, String> {
        let directory =
            tempfile::tempdir().map_err(|_| "cannot create isolated browser session")?;
        std::fs::write(directory.path().join("browser.json"), "{}")
            .map_err(|_| "cannot create isolated browser config")?;
        let id = format!(
            "ai-summary-{}-{}",
            std::process::id(),
            directory
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("browser")
                .trim_start_matches('.')
        );
        Ok(Self { directory, id })
    }

    fn run(&self, args: &[&str], timeout: Duration) -> Result<String, String> {
        let mut command = Command::new("agent-browser");
        isolate(&mut command, self.directory.path());
        command.args(["--session", &self.id]).args(args);
        let output = subprocess::capture(&mut command, None, timeout, 400_000)
            .map_err(|error| format!("agent-browser {}: {error}", args[0]))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let diagnostic = if stderr.trim().is_empty() {
                &stdout
            } else {
                &stderr
            };
            return Err(format!(
                "agent-browser {}: {}",
                args[0],
                error_detail(&strip_ansi(diagnostic), "")
            ));
        }
        Ok(strip_ansi(&stdout))
    }

    fn text(&self, selector: &str) -> Result<String, String> {
        self.run(&["get", "text", selector], Duration::from_secs(5))
            .map(|text| collapse_ws(text.trim()))
    }
}

fn isolate(command: &mut Command, directory: &Path) {
    command
        .current_dir(directory)
        .arg("--config")
        .arg(directory.join("browser.json"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("AGENT_BROWSER_") {
            command.env_remove(name);
        }
    }
}

impl Drop for BrowserSession {
    fn drop(&mut self) {
        let _ = self.run(&["close"], Duration::from_secs(3));
    }
}
