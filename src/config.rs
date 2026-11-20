//! Load and resolve configuration without moving credentials between origins.
//! Exports Config, resolve_config, local paths, and configuration commands.

mod commands;
#[cfg(test)]
mod tests;

pub use commands::{print_config, run_setup};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, path::PathBuf};

use crate::Cli;
#[cfg(test)]
use crate::Commands;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    #[serde(skip)]
    pub explicit_summary_backend: bool,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub max_pages: usize,
    pub max_page_chars: usize,
    pub max_summary_tokens: u32,
    pub summary_timeout_secs: u64,
    pub opencode_timeout_secs: u64,
    pub brave_api_key: String,
    pub gemini_api_key: String,
    pub gemini_model: String,
    pub cf_account_id: String,
    pub cf_api_token: String,
    pub jina_api_key: String,
    pub openrouter_api_key: String,
    pub openrouter_model: String,
}

fn default_openrouter_model() -> String {
    "xiaomi/mimo-v2.5".to_string()
}

fn default_gemini_model() -> String {
    "gemini-2.5-flash".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            explicit_summary_backend: false,
            api_url: "http://127.0.0.1:8000".to_string(),
            api_key: String::new(),
            model: "Qwen3.5-9B-MLX-4bit".to_string(),
            max_pages: 3,
            max_page_chars: 4000,
            max_summary_tokens: 1024,
            summary_timeout_secs: 60,
            opencode_timeout_secs: 25,
            brave_api_key: String::new(),
            gemini_api_key: String::new(),
            gemini_model: default_gemini_model(),
            cf_account_id: String::new(),
            cf_api_token: String::new(),
            jina_api_key: String::new(),
            openrouter_api_key: String::new(),
            openrouter_model: default_openrouter_model(),
        }
    }
}

pub fn config_path() -> PathBuf {
    dirs_home().join(".ai-summary/config.toml")
}

pub fn load_config() -> Result<Config, String> {
    let source = match fs::read_to_string(config_path()) {
        Ok(source) => source,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Config::default()),
        Err(error) => return Err(format!("cannot read configuration: {}", error.kind())),
    };
    toml::from_str(&source).map_err(|_| "invalid TOML configuration".into())
}

pub fn resolve_config(cli: &Cli) -> Result<Config, String> {
    let mut cfg = load_config()?;
    cfg.explicit_summary_backend =
        cli.api_url.is_some() || cli.api_key.is_some() || cli.model.is_some();
    if let Some(url) = &cli.api_url {
        if !same_origin(&cfg.api_url, url) {
            cfg.api_key.clear();
        }
        cfg.api_url = url.clone();
    }
    if let Some(key) = &cli.api_key {
        cfg.api_key = key.clone();
    } else if cfg.api_key.is_empty() && is_default_omlx_origin(&cfg.api_url) {
        cfg.api_key = load_omlx_api_key();
    }
    if let Some(model) = &cli.model {
        cfg.model = model.clone();
    }
    if cli.deep {
        cfg.max_pages = 5;
    }
    for (field, variable) in [
        (&mut cfg.brave_api_key, "BRAVE_API_KEY"),
        (&mut cfg.gemini_api_key, "GEMINI_API_KEY"),
        (&mut cfg.cf_account_id, "CF_ACCOUNT_ID"),
        (&mut cfg.cf_api_token, "CF_API_TOKEN"),
        (&mut cfg.jina_api_key, "JINA_API_KEY"),
        (&mut cfg.openrouter_api_key, "OPENROUTER_API_KEY"),
    ] {
        if field.is_empty() {
            *field = std::env::var(variable).unwrap_or_default();
        }
    }
    Ok(cfg)
}

fn same_origin(left: &str, right: &str) -> bool {
    match (Url::parse(left), Url::parse(right)) {
        (Ok(left), Ok(right)) => left.origin() == right.origin(),
        _ => false,
    }
}

fn is_default_omlx_origin(url: &str) -> bool {
    [
        "http://127.0.0.1:8000",
        "http://localhost:8000",
        "http://[::1]:8000",
    ]
    .iter()
    .any(|origin| same_origin(url, origin))
}

pub fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}

pub fn load_omlx_api_key() -> String {
    let path = dirs_home().join(".omlx/settings.json");
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|value| {
            value
                .get("auth")?
                .get("api_key")?
                .as_str()
                .map(String::from)
        })
        .unwrap_or_default()
}
