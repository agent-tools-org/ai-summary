// Test config resolution using isolated temporary configuration.
// Exercises precedence, defaults, and endpoint ownership.

use super::*;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

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
        let cfg = load_config().unwrap();
        let baseline = Config::default();
        assert_eq!(cfg.api_url, baseline.api_url);
        assert_eq!(cfg.model, baseline.model);
        assert_eq!(cfg.max_pages, baseline.max_pages);
        assert_eq!(cfg.max_page_chars, baseline.max_page_chars);
        assert_eq!(cfg.gemini_model, "gemini-2.5-flash");
        assert_eq!(cfg.openrouter_model, "xiaomi/mimo-v2.5");
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
        let cfg = load_config().unwrap();
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

        let _guards = [
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
            metadata: false,
        };
        let cfg = resolve_config(&cli).unwrap();
        assert_eq!(cfg.api_url, "https://cli.url");
        assert!(cfg.api_key.is_empty());
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
