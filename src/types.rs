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

#[derive(Debug, Clone, serde::Serialize)]
pub struct FetchedPage {
    pub url: String,
    pub final_url: String,
    pub extractor: &'static str,
    pub media_type: String,
    pub truncated: bool,
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

#[derive(serde::Serialize, Deserialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
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
    format!("exit code {code}; stderr: {}", error_detail(stderr, secret))
}

fn redact_sensitive(text: &str, secret: &str) -> String {
    let text = if secret.is_empty() {
        text.to_string()
    } else {
        text.replace(secret, "[redacted]")
    };
    text.lines()
        .map(redact_sensitive_line)
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_sensitive_line(line: &str) -> String {
    let line = redact_after_marker(line, "bearer", b"");
    let line = redact_after_marker(&line, "authorization", b":=");
    let line = redact_after_marker(&line, "x-api-key", b":=");
    let line = redact_after_marker(&line, "?key=", b"");
    redact_after_marker(&line, "api_key=", b"")
}

fn redact_after_marker(line: &str, marker: &str, delimiters: &[u8]) -> String {
    let lowercase = line.to_ascii_lowercase();
    let Some(start) = lowercase.find(marker) else {
        return line.to_string();
    };
    let mut value_start = start + marker.len();
    if !delimiters.is_empty() {
        let Some(delimiter) = lowercase[value_start..]
            .bytes()
            .position(|byte| delimiters.contains(&byte))
        else {
            return line.to_string();
        };
        value_start += delimiter + 1;
    };
    value_start += line[value_start..]
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map(|(index, _)| index)
        .unwrap_or(line[value_start..].len());
    let value_end = line[value_start..]
        .char_indices()
        .find(|(_, character)| {
            character.is_whitespace() || matches!(character, '&' | '"' | '\'' | ',' | ';')
        })
        .map(|(index, _)| value_start + index)
        .unwrap_or(line.len());
    if value_start == value_end {
        return line.to_string();
    }
    format!("{}[redacted]{}", &line[..value_start], &line[value_end..])
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

    #[test]
    fn error_detail_redacts_secret_patterns_without_a_secret_argument() {
        let detail = error_detail(
            "?key=query-secret&next=1 api_key=api-secret X-Api-Key: header-secret Bearer token-secret",
            "",
        );
        assert!(!detail.contains("query-secret"));
        assert!(!detail.contains("api-secret"));
        assert!(!detail.contains("header-secret"));
        assert!(!detail.contains("token-secret"));
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
