// Application configuration loading, resolution, and printing.
// Exports: Config, load_config(), resolve_config(), print_config(), config_path().
// Deps: crate::Cli, serde, toml, std::fs, environment variables.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::Cli;
#[cfg(test)]
use crate::Commands;

#[derive(Serialize, Deserialize, Clone)]
pub struct Config {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    #[serde(default = "default_max_pages")]
    pub max_pages: usize,
    #[serde(default = "default_max_page_chars")]
    pub max_page_chars: usize,
    #[serde(default = "default_max_summary_tokens")]
    pub max_summary_tokens: u32,
    #[serde(default)]
    pub brave_api_key: String,
    #[serde(default)]
    pub gemini_api_key: String,
    #[serde(default = "default_gemini_model")]
    pub gemini_model: String,
    #[serde(default)]
    pub cf_account_id: String,
    #[serde(default)]
    pub cf_api_token: String,
    #[serde(default)]
    pub jina_api_key: String,
    #[serde(default)]
    pub openrouter_api_key: String,
    #[serde(default = "default_openrouter_model")]
    pub openrouter_model: String,
}

fn default_openrouter_model() -> String {
    "xiaomi/mimo-v2-flash".to_string()
}

fn default_gemini_model() -> String {
    "gemini-2.0-flash".to_string()
}

fn default_max_pages() -> usize {
    3
}

fn default_max_page_chars() -> usize {
    4000
}

fn default_max_summary_tokens() -> u32 {
    1024
}

impl Default for Config {
    fn default() -> Self {
        Self {
            api_url: "http://127.0.0.1:8000".to_string(),
            api_key: String::new(),
            model: "Qwen3.5-9B-MLX-4bit".to_string(),
            max_pages: 3,
            max_page_chars: 4000,
            max_summary_tokens: 1024,
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

pub fn load_config() -> Config {
    let path = config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        toml::from_str(&content).unwrap_or_default()
    } else {
        Config::default()
    }
}

pub fn resolve_config(cli: &Cli) -> Config {
    let mut cfg = load_config();
    if let Some(ref url) = cli.api_url {
        cfg.api_url = url.clone();
    }
    if let Some(ref key) = cli.api_key {
        cfg.api_key = key.clone();
    }
    if let Some(ref model) = cli.model {
        cfg.model = model.clone();
    }
    if cli.deep {
        cfg.max_pages = 5;
    }
    if cfg.api_key.is_empty() {
        cfg.api_key = load_omlx_api_key();
    }
    if cfg.brave_api_key.is_empty() {
        if let Ok(key) = std::env::var("BRAVE_API_KEY") {
            cfg.brave_api_key = key;
        }
    }
    if cfg.gemini_api_key.is_empty() {
        if let Ok(key) = std::env::var("GEMINI_API_KEY") {
            cfg.gemini_api_key = key;
        }
    }
    if cfg.cf_account_id.is_empty() {
        if let Ok(value) = std::env::var("CF_ACCOUNT_ID") {
            cfg.cf_account_id = value;
        }
    }
    if cfg.cf_api_token.is_empty() {
        if let Ok(value) = std::env::var("CF_API_TOKEN") {
            cfg.cf_api_token = value;
        }
    }
    if cfg.jina_api_key.is_empty() {
        if let Ok(value) = std::env::var("JINA_API_KEY") {
            cfg.jina_api_key = value;
        }
    }
    if cfg.openrouter_api_key.is_empty() {
        if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
            cfg.openrouter_api_key = key;
        }
    }
    cfg
}

pub fn print_config() {
    let path = config_path();
    if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            println!("Config file: {}", path.display());
            println!();
            println!("{content}");
        }
    } else {
        let cfg = Config::default();
        let toml_str = format!(
            r#"# ai-summary configuration
# Supports any OpenAI-compatible API endpoint

# Local oMLX (recommended for Apple Silicon)
api_url = "{}"
api_key = ""  # Leave empty for oMLX auto-detection
model = "{}"

# --- Alternative providers (uncomment one) ---
# api_url = "https://api.openai.com"
# api_key = "sk-..."
# model = "gpt-4o-mini"

# api_url = "https://api.groq.com/openai"
# api_key = "gsk_..."
# model = "llama-3.3-70b-versatile"

# api_url = "https://api.deepseek.com"
# api_key = "sk-..."
# model = "deepseek-chat"

# --- Search provider ---
# Gemini + Google Search grounding (recommended, best quality)
# Get free key at https://aistudio.google.com/apikey
gemini_api_key = ""
gemini_model = "gemini-2.0-flash"

# Brave Search API fallback (free: https://brave.com/search/api/)
brave_api_key = ""

# Search priority: Gemini (if key set) > DuckDuckGo > Brave (if key set)

max_pages = {}
max_page_chars = {}
max_summary_tokens = {}

# --- OpenRouter (direct API, fast + cheap) ---
# Get key at: https://openrouter.ai/keys
# Or run: ai-summary setup --openrouter-key YOUR_KEY
# openrouter_api_key = ""
# openrouter_model = "xiaomi/mimo-v2-flash"  # ~$0.05/1M tokens

# --- Fetch fallback ---
# Jina Reader API (free tier available: https://jina.ai/reader/)
# jina_api_key = ""

# --- Cloudflare Browser Rendering (crawl command) ---
# cf_account_id = ""
# cf_api_token = ""
"#,
            cfg.api_url, cfg.model, cfg.max_pages, cfg.max_page_chars, cfg.max_summary_tokens
        );
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, &toml_str);
        println!("Created config file: {}", path.display());
        println!();
        println!("{toml_str}");
    }
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

pub fn set_openrouter_key(key: &str) {
    let path = config_path();
    let mut content = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        String::new()
    };
    if let Some(start) = content.find("openrouter_api_key") {
        if let Some(nl) = content[start..].find('\n') {
            content.replace_range(start..start + nl, &format!("openrouter_api_key = \"{key}\""));
        } else {
            content.replace_range(start.., &format!("openrouter_api_key = \"{key}\""));
        }
    } else {
        if !content.is_empty() && !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!("\nopenrouter_api_key = \"{key}\"\n"));
    }
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, content);
}

pub fn run_setup(openrouter_key: Option<&str>) {
    if let Some(key) = openrouter_key {
        set_openrouter_key(key);
        println!("OpenRouter API key saved to {}", config_path().display());
        println!("LLM calls will now use OpenRouter directly (no subprocess overhead).");
        return;
    }
    let cfg = resolve_config(&crate::Cli {
        command: crate::Commands::Stats,
        deep: false,
        raw: false,
        cf: false,
        browser: false,
        api_url: None,
        api_key: None,
        model: None,
        json: false,
        no_cache: false,
        doc: false,
        metadata: false,
    });
    println!("ai-summary setup\n");
    println!("LLM backend priority:");
    if !cfg.openrouter_api_key.is_empty() {
        println!("  1. OpenRouter API ({}) — configured", cfg.openrouter_model);
    } else {
        println!("  1. OpenRouter API (direct) — not configured");
    }
    if crate::llm::has_opencode() {
        println!("  2. opencode CLI (subprocess) — installed");
    } else {
        println!("  2. opencode CLI (subprocess) — not found");
    }
    println!("  3. Custom API — {} ({})", cfg.model, cfg.api_url);
    println!();
    println!("To enable direct OpenRouter calls (faster, no subprocess):");
    println!("  ai-summary setup --openrouter-key YOUR_KEY");
    println!();
    println!("Or set env: export OPENROUTER_API_KEY=sk-or-...");
    println!("Get a free key at: https://openrouter.ai/keys");
    println!();
    println!("Config: {}", config_path().display());
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::{SystemTime, UNIX_EPOCH};
    use std::ffi::OsString;

    static HOME_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_home<R, F: FnOnce(&PathBuf) -> R>(f: F) -> R {
        let _guard = HOME_LOCK.lock().unwrap();
        let prev_home = env::var_os("HOME");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let temp_home = env::temp_dir().join(format!("ai_summary_home_{now}"));
        let _ = fs::remove_dir_all(&temp_home);
        env::set_var("HOME", &temp_home);
        let result = f(&temp_home);
        if let Some(home) = prev_home {
            env::set_var("HOME", home);
        } else {
            env::remove_var("HOME");
        }
        let _ = fs::remove_dir_all(&temp_home);
        result
    }

    struct EnvGuard {
        key: String,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &str, value: &str) -> Self {
            let previous = env::var_os(key);
            env::set_var(key, value);
            EnvGuard {
                key: key.to_string(),
                previous,
            }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                env::set_var(&self.key, previous);
            } else {
                env::remove_var(&self.key);
            }
        }
    }

    #[test]
    fn default_config_when_no_file_exists() {
        with_temp_home(|_| {
            let cfg = load_config();
            let baseline = Config::default();
            assert_eq!(cfg.api_url, baseline.api_url);
            assert_eq!(cfg.model, baseline.model);
            assert_eq!(cfg.max_pages, baseline.max_pages);
            assert_eq!(cfg.max_page_chars, baseline.max_page_chars);
        });
    }

    #[test]
    fn dirs_home_returns_path() {
        with_temp_home(|home| {
            assert_eq!(dirs_home(), home.clone());
        });
    }

    #[test]
    fn config_path_includes_home_directory() {
        with_temp_home(|home| {
            let expected = home.join(".ai-summary/config.toml");
            assert_eq!(config_path(), expected);
        });
    }

    #[test]
    fn load_config_reads_existing_file() {
        with_temp_home(|home| {
            let config_dir = home.join(".ai-summary");
            fs::create_dir_all(&config_dir).unwrap();
            let content = r#"
api_url = "https://custom.url"
api_key = "file-key"
model = "custom-model"
max_pages = 7
max_page_chars = 1234
brave_api_key = "file-brave"
"#;
            fs::write(config_dir.join("config.toml"), content).unwrap();
            let cfg = load_config();
            assert_eq!(cfg.api_url, "https://custom.url");
            assert_eq!(cfg.model, "custom-model");
            assert_eq!(cfg.max_pages, 7);
            assert_eq!(cfg.max_page_chars, 1234);
            assert_eq!(cfg.brave_api_key, "file-brave");
        });
    }

    #[test]
    fn resolve_config_merges_cli_overrides_and_env_vars() {
        with_temp_home(|home| {
            let config_dir = home.join(".ai-summary");
            fs::create_dir_all(&config_dir).unwrap();
            let content = r#"
api_url = "https://file.url"
api_key = ""
model = "file-model"
max_pages = 2
max_page_chars = 1111
max_summary_tokens = 888
gemini_api_key = ""
brave_api_key = ""
cf_account_id = ""
cf_api_token = ""
jina_api_key = ""
"#;
            fs::write(config_dir.join("config.toml"), content).unwrap();
            let omlx_dir = home.join(".omlx");
            fs::create_dir_all(&omlx_dir).unwrap();
            fs::write(
                omlx_dir.join("settings.json"),
                r#"{"auth":{"api_key":"omlx-key"}}"#,
            )
            .unwrap();

            let _guards = vec![
                EnvGuard::set("BRAVE_API_KEY", "env-brave"),
                EnvGuard::set("GEMINI_API_KEY", "env-gemini"),
                EnvGuard::set("CF_ACCOUNT_ID", "env-cf-id"),
                EnvGuard::set("CF_API_TOKEN", "env-cf-token"),
                EnvGuard::set("JINA_API_KEY", "env-jina"),
            ];

            let cli = Cli {
                command: Commands::Stats,
                deep: true,
                raw: false,
                cf: false,
                browser: false,
                api_url: Some("https://cli.url".to_string()),
                api_key: None,
                model: Some("cli-model".to_string()),
                json: false,
                no_cache: false,
                doc: false,
                metadata: false,
            };
            let cfg = resolve_config(&cli);
            assert_eq!(cfg.api_url, "https://cli.url");
            assert_eq!(cfg.api_key, "omlx-key");
            assert_eq!(cfg.model, "cli-model");
            assert_eq!(cfg.max_pages, 5);
            assert_eq!(cfg.max_page_chars, 1111);
            assert_eq!(cfg.max_summary_tokens, 888);
            assert_eq!(cfg.brave_api_key, "env-brave");
            assert_eq!(cfg.gemini_api_key, "env-gemini");
            assert_eq!(cfg.cf_account_id, "env-cf-id");
            assert_eq!(cfg.cf_api_token, "env-cf-token");
            assert_eq!(cfg.jina_api_key, "env-jina");
        });
    }
}
