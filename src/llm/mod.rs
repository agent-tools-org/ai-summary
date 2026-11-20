//! Select summary backends and return completed, provider-attributed results.
//! HTTP and subprocess adapters return data without writing to stdout.

mod clean;
mod http;
mod opencode;
mod stream;

use crate::config::Config;
use crate::types::Usage;
use reqwest::blocking::Client;
use std::{
    fmt,
    time::{Duration, Instant},
};

const BACKEND_TIMEOUT: Duration = Duration::from_secs(120);

pub(crate) const OPENROUTER_API_URL: &str = "https://openrouter.ai/api";
const OPENCODE_MODEL: &str = "opencode/nemotron-3-ultra-free";

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Http,
    OpenRouter,
    Opencode,
    Gemini,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Summary {
    pub text: String,
    pub usage: Option<Usage>,
    pub provider: Provider,
    pub model: String,
}

impl Summary {
    pub(crate) fn new(
        text: &str,
        usage: Option<Usage>,
        provider: Provider,
        model: &str,
    ) -> Result<Self, String> {
        let text = clean::strip_thinking(text);
        if text.is_empty() {
            return Err("backend returned no summary".into());
        }
        Ok(Self {
            text,
            usage,
            provider,
            model: model.into(),
        })
    }
}

#[derive(Debug)]
pub struct SummaryError(Vec<String>);

impl fmt::Display for SummaryError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(output, "{}", self.0.join("; "))
    }
}

pub(crate) fn has_opencode() -> bool {
    static DETECTED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *DETECTED.get_or_init(opencode::has_opencode)
}

pub fn llm_summarize(
    cfg: &Config,
    prompt: &str,
    content: &str,
    streaming: bool,
) -> Result<Summary, SummaryError> {
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| SummaryError(vec!["failed to initialize HTTP summary client".into()]))?;
    let deadline = Instant::now() + Duration::from_secs(cfg.summary_timeout_secs.clamp(1, 600));
    let mut failures = Vec::new();
    let mut plan = Backend::plan(cfg).into_iter().peekable();
    while let Some(backend) = plan.next() {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            failures.push("overall summary deadline exceeded".into());
            break;
        };
        let cap = if matches!(backend, Backend::Opencode) {
            Duration::from_secs(cfg.opencode_timeout_secs.clamp(1, 600))
        } else {
            BACKEND_TIMEOUT
        };
        let result = backend.summarize(
            &client,
            prompt,
            content,
            cfg.max_summary_tokens,
            streaming,
            remaining.min(cap),
        );
        match result {
            Ok(summary) => return Ok(summary),
            Err(reason) => {
                let diagnostic = format!("{}: {reason}", backend.label());
                if plan.peek().is_some() {
                    eprintln!("[ai-summary] {diagnostic}; trying fallback");
                }
                failures.push(diagnostic);
            }
        }
    }
    Err(SummaryError(failures))
}

enum Backend<'a> {
    Http(Provider, &'a str, &'a str, &'a str),
    Opencode,
}

impl<'a> Backend<'a> {
    fn plan(cfg: &'a Config) -> Vec<Self> {
        let mut plan = Vec::new();
        if !cfg.explicit_summary_backend {
            if !cfg.openrouter_api_key.is_empty() {
                plan.push(Self::Http(
                    Provider::OpenRouter,
                    OPENROUTER_API_URL,
                    &cfg.openrouter_api_key,
                    &cfg.openrouter_model,
                ));
            }
            if has_opencode() {
                plan.push(Self::Opencode);
            }
        }
        plan.push(Self::Http(
            Provider::Http,
            &cfg.api_url,
            &cfg.api_key,
            &cfg.model,
        ));
        plan
    }

    fn label(&self) -> &str {
        match self {
            Self::Http(Provider::OpenRouter, ..) => "OpenRouter",
            Self::Http(..) => "configured HTTP backend",
            Self::Opencode => "opencode",
        }
    }

    fn summarize(
        &self,
        client: &Client,
        prompt: &str,
        content: &str,
        max_tokens: u32,
        streaming: bool,
        timeout: Duration,
    ) -> Result<Summary, String> {
        match self {
            Self::Http(provider, url, key, model) => http::summarize_http(
                client,
                http::HttpSummaryRequest {
                    provider: *provider,
                    url,
                    key,
                    model,
                    max_tokens,
                    prompt,
                    content,
                    streaming,
                    timeout,
                },
            ),
            Self::Opencode => {
                opencode::summarize_opencode(OPENCODE_MODEL, prompt, content, timeout)
            }
        }
    }
}

pub fn cache_scope(cfg: &Config) -> Option<(Provider, String)> {
    let plan = Backend::plan(cfg);
    let max_tokens = cfg.max_summary_tokens;
    match plan.first()? {
        Backend::Http(Provider::Http, _, key, _) if !key.is_empty() => None,
        Backend::Http(provider, url, _, model) => Some((
            *provider,
            serde_json::json!([provider, url, model, max_tokens]).to_string(),
        )),
        Backend::Opencode => Some((Provider::Opencode, OPENCODE_MODEL.into())),
    }
}
