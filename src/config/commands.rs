//! Implement redacted config display and atomic setup writes.
//! Stores TOML in the existing per-user configuration location.

use super::{config_path, load_config};
use std::{fs, io, path::Path};

const TEMPLATE: &str = include_str!("default.toml");

pub fn print_config() -> Result<(), String> {
    let path = config_path();
    if !path.exists() {
        save_config(&path, TEMPLATE).map_err(write_error)?;
    }
    let mut cfg = load_config()?;
    for field in [
        &mut cfg.api_key,
        &mut cfg.brave_api_key,
        &mut cfg.gemini_api_key,
        &mut cfg.cf_api_token,
        &mut cfg.jina_api_key,
        &mut cfg.openrouter_api_key,
    ] {
        if !field.is_empty() {
            *field = "[redacted]".into();
        }
    }
    if let Ok(mut url) = reqwest::Url::parse(&cfg.api_url) {
        let _ = url.set_username("");
        let _ = url.set_password(None);
        url.set_query(None);
        url.set_fragment(None);
        cfg.api_url = url.to_string();
    } else {
        cfg.api_url = "[invalid URL]".into();
    }
    let text = toml::to_string_pretty(&cfg).map_err(|_| "cannot serialize configuration")?;
    println!("Config file: {}\n\n{text}", path.display());
    Ok(())
}

pub fn run_setup(key: Option<&str>) -> Result<(), String> {
    if let Some(key) = key {
        save_openrouter_key(key)?;
        println!("OpenRouter API key saved to {}", config_path().display());
        return Ok(());
    }
    let cfg = load_config()?;
    let configured = !cfg.openrouter_api_key.is_empty()
        || std::env::var("OPENROUTER_API_KEY").is_ok_and(|value| !value.is_empty());
    println!("ai-summary setup\n\nLLM backend priority:");
    if configured {
        println!(
            "  1. OpenRouter API ({}) — configured",
            cfg.openrouter_model
        );
    } else {
        println!("  1. OpenRouter API (direct) — not configured");
    }
    let opencode = if crate::llm::has_opencode() {
        "installed"
    } else {
        "not found"
    };
    println!("  2. opencode CLI (subprocess) — {opencode}\n  3. Custom HTTP API");
    println!("\nConfig: {}", config_path().display());
    Ok(())
}

fn save_openrouter_key(key: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("OpenRouter API key is empty".into());
    }
    let path = config_path();
    let source = match fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => TEMPLATE.into(),
        Err(error) => return Err(format!("cannot read configuration: {}", error.kind())),
    };
    let mut table: toml::Table =
        toml::from_str(&source).map_err(|_| "invalid TOML configuration")?;
    table.insert("openrouter_api_key".into(), key.into());
    let text = toml::to_string_pretty(&table).map_err(|_| "cannot serialize configuration")?;
    toml::from_str::<super::Config>(&text).map_err(|_| "invalid configuration values")?;
    save_config(&path, &text).map_err(write_error)
}

fn save_config(path: &Path, text: &str) -> io::Result<()> {
    crate::storage::write_atomic(path, text.as_bytes(), 0o600)
}

fn write_error(error: io::Error) -> String {
    format!("cannot save configuration: {}", error.kind())
}
