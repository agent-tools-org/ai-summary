//! Report installed tools and provider readiness without sending a prompt.
//! Hosted authentication checks are explicit and print no keys or endpoint URLs.

use crate::{commands::output, config::Config, llm::OPENROUTER_API_URL, subprocess, Cli};
use reqwest::{
    blocking::{Client, RequestBuilder},
    Url,
};
use serde::Serialize;
use serde_json::json;
use std::{process::Command, time::Duration};

#[derive(Serialize)]
struct Status {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

pub fn run(cli: &Cli, cfg: &Config, check_hosted: bool) {
    let client =
        crate::credential_client().unwrap_or_else(|error| crate::commands::fail(2, &error));
    let tools = ["defuddle", "agent-browser", "gh", "repomix", "opencode"]
        .into_iter()
        .map(|name| (name, identify(name)))
        .collect::<std::collections::BTreeMap<_, _>>();
    let http = http_status(&client, cfg);
    let openrouter = openrouter_status(&client, cfg, check_hosted);
    let search = [
        ("gemini", configured(&cfg.gemini_api_key)),
        ("brave", configured(&cfg.brave_api_key)),
    ];
    if output::structured(cli) {
        output::emit_json(json!({
            "tools":tools, "summary":{"http":http,"openrouter":openrouter},
            "search":search.iter().map(|(name, status)| (name, status)).collect::<std::collections::BTreeMap<_, _>>(),
            "summary_timeout_secs":cfg.summary_timeout_secs.clamp(1,600),
            "opencode_timeout_secs":cfg.opencode_timeout_secs.clamp(1,600),
        }));
    } else {
        for (name, status) in [("HTTP summary", &http), ("OpenRouter", &openrouter)] {
            display(name, status);
        }
        for (name, status) in search {
            display(name, &status);
        }
        for (name, status) in tools {
            display(name, &status);
        }
    }
}

fn display(name: &str, status: &Status) {
    println!(
        "{name}: {}{}",
        status.status,
        status
            .detail
            .as_ref()
            .map(|value| format!(" — {value}"))
            .unwrap_or_default()
    );
}

fn identify(name: &str) -> Status {
    let result = subprocess::capture(
        Command::new(name).arg("--version"),
        None,
        Duration::from_secs(2),
        4096,
    );
    match result {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            Status {
                status: "installed",
                detail: version
                    .lines()
                    .next()
                    .map(|line| crate::text::prefix(line, 120).into()),
            }
        }
        Ok(_) => state("version_failed"),
        Err(error) if error.contains("No such file") => state("missing"),
        Err(_) => state("unavailable"),
    }
}

fn http_status(client: &Client, cfg: &Config) -> Status {
    let Ok(mut url) = Url::parse(&cfg.api_url) else {
        return state("invalid_endpoint");
    };
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return state("invalid_endpoint");
    }
    if !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")) {
        return state("external_not_checked");
    }
    let base = url.path().trim_end_matches('/');
    let path = if base.ends_with("/v1") {
        format!("{base}/models")
    } else {
        format!("{base}/v1/models")
    };
    url.set_path(&path);
    let mut request = client.get(url);
    if !cfg.api_key.is_empty() {
        request = request.bearer_auth(&cfg.api_key);
    }
    probe(request)
}

fn openrouter_status(client: &Client, cfg: &Config, check: bool) -> Status {
    if cfg.openrouter_api_key.is_empty() {
        return state("not_configured");
    }
    if !check {
        return state("configured_not_checked");
    }
    let url = format!("{OPENROUTER_API_URL}/v1/key");
    let mut status = probe(client.get(url).bearer_auth(&cfg.openrouter_api_key));
    if status.status == "reachable" {
        status.status = "key_valid";
        status.detail = Some("chat availability and account credits were not checked".into());
    }
    status
}

fn probe(request: RequestBuilder) -> Status {
    let response = match request.timeout(Duration::from_secs(3)).send() {
        Ok(response) => response,
        Err(error) if error.is_timeout() => return state("timed_out"),
        Err(error) if error.is_connect() => return state("unreachable"),
        Err(_) => return state("request_failed"),
    };
    match response.status().as_u16() {
        200..=299 => state("reachable"),
        401 | 403 => state("authentication_failed"),
        402 => state("credits_required"),
        429 => state("rate_limited"),
        status => Status {
            status: "http_error",
            detail: Some(format!("HTTP {status}")),
        },
    }
}

fn configured(key: &str) -> Status {
    if key.is_empty() {
        state("not_configured")
    } else {
        state("configured_not_checked")
    }
}

fn state(status: &'static str) -> Status {
    Status {
        status,
        detail: None,
    }
}
