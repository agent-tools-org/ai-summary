//! Expose command workflows and shared summary output to the CLI.
//! Each command handles its own acquisition and delegates summary execution.

mod deep;
mod fetch;
mod github;
pub(crate) mod output;
mod repo;
mod search;
mod stdin;
pub(crate) mod summary;

pub use fetch::run_fetch;
pub use github::run_github;
pub use repo::run_repo;
pub use search::run_search;
pub use stdin::run_summarize;

use crate::{config::Config, Cli};

fn renderers(cli: &Cli, cfg: &Config) -> (bool, bool) {
    if cli.cf && (cfg.cf_account_id.is_empty() || cfg.cf_api_token.is_empty()) {
        fail(1, "--cf requires CF_ACCOUNT_ID and CF_API_TOKEN");
    }
    let cf = cli.cf;
    (cf, cli.browser && !cf)
}

fn emit_raw(cli: &Cli, content: &str) {
    if output::structured(cli) {
        output::emit_json(serde_json::json!({"text": content}));
    } else {
        println!("{content}");
    }
}

pub(crate) fn fail(code: i32, message: &str) -> ! {
    eprintln!("[ai-summary] {message}");
    std::process::exit(code)
}
