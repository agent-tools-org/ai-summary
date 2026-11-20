//! Read and summarize stdin using the shared output contract.
//! Raw mode serializes original input without invoking a provider.

use super::{output, summary};
use crate::{config::Config, terminal, Cli};
use serde_json::json;
use std::{io, time::Instant};

pub fn run_summarize(cli: &Cli, cfg: &Config, prompt: &str) {
    let started = Instant::now();
    if terminal::is_stdin_terminal() {
        eprintln!("Reading from stdin (Ctrl+D to end)...");
    }
    let bytes = crate::read_limit(io::stdin().lock(), crate::subprocess::CONTENT_LIMIT)
        .unwrap_or_else(|error| super::fail(1, &error));
    let input = String::from_utf8(bytes).unwrap_or_else(|_| super::fail(1, "stdin is not UTF-8"));
    if input.trim().is_empty() {
        eprintln!("Error: No input received on stdin.");
        std::process::exit(1);
    }
    if cli.raw {
        if output::structured(cli) {
            output::emit_json(json!({"prompt": prompt, "text": input}));
        } else {
            println!("{input}");
        }
        return;
    }
    let (result, _) = summary::summarize(cli, cfg, "stdin", prompt, &input, false);
    let raw_bytes = input.len() as u64;
    let label = if prompt.is_empty() { "(stdin)" } else { prompt };
    let (tokens, duration) = summary::record(&result, label, "stdin", 1, raw_bytes, started, false);
    output::emit(
        cli,
        &result,
        raw_bytes,
        json!({
            "prompt": prompt, "raw_chars": raw_bytes, "summary_chars": result.text.len(),
            "tokens_saved": tokens, "duration_secs": duration,
        }),
    );
}
