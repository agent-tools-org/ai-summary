// Shared data types and redacted backend diagnostics.
// Exports: response types, error_detail(), and cli_failure().
// Deps: serde::Deserialize for API response decoding.

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub url: String,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Clone)]
pub struct FetchedPage {
    pub url: String,
    pub text: String,
}

#[derive(Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

#[derive(Deserialize)]
pub struct Choice {
    pub message: MessageContent,
}

#[derive(Deserialize)]
pub struct MessageContent {
    pub content: String,
}

#[derive(Deserialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

pub struct SummarizeResult {
    pub text: String,
    pub usage: Option<Usage>,
    pub raw_chars: u64,
    pub summary_chars: u64,
}

pub(crate) fn error_detail(body: &str, secret: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(|message| message.as_str())
                .or_else(|| value.get("message").and_then(|message| message.as_str()))
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.trim().to_string());
    let detail = if detail.is_empty() {
        "empty response body".to_string()
    } else {
        detail
    };
    truncate_error(&redact_sensitive(&detail, secret))
}

pub(crate) fn cli_failure(status: &std::process::ExitStatus, stderr: &str, secret: &str) -> String {
    let code = status
        .code()
        .map(|value| value.to_string())
        .unwrap_or_else(|| "signal".to_string());
    format!(
        "exit code {code}; stderr: {}",
        error_detail(stderr, secret)
    )
}

fn redact_sensitive(text: &str, secret: &str) -> String {
    let text = if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "[redacted]")
    };
    text.lines()
        .map(redact_authorization_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_authorization_line(line: &str) -> String {
    let lowercase = line.to_ascii_lowercase();
    let Some(start) = lowercase.find("authorization") else {
        return line.to_string();
    };
    let suffix = &lowercase[start..];
    let delimiter = suffix.find(':').or_else(|| suffix.find('='));
    let Some(delimiter) = delimiter else {
        return line.to_string();
    };
    format!("{} [redacted]", &line[..start + delimiter + 1])
}

fn truncate_error(message: &str) -> String {
    const MAX_ERROR_CHARS: usize = 240;
    message.chars().take(MAX_ERROR_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_detail_redacts_api_keys_and_authorization_headers() {
        let detail = error_detail(
            "Authorization: Bearer hidden-header\nprovider key=hidden-key",
            "hidden-key",
        );
        assert!(!detail.contains("hidden-header"));
        assert!(!detail.contains("hidden-key"));
        assert!(detail.contains("Authorization: [redacted]"));
    }

    #[cfg(unix)]
    #[test]
    fn cli_failure_includes_exit_code_and_truncated_stderr() {
        use std::os::unix::process::ExitStatusExt;

        let status = std::process::ExitStatus::from_raw(41 << 8);
        let stderr = "gemini authentication failed: ".to_string() + &"x".repeat(300);
        let reason = cli_failure(&status, &stderr, "");
        assert!(reason.starts_with("exit code 41; stderr: gemini authentication failed:"));
        assert!(reason.len() <= 270);
    }
}
