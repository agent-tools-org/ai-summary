//! Serialize and render completed summaries at the CLI boundary.
//! Keeps provenance, diagnostics, and display formatting out of providers.

use crate::llm::{Summary, SummaryError};
use crate::{stats::fmtn, terminal, Cli};
use serde_json::{json, Value};

pub fn structured(cli: &Cli) -> bool {
    cli.json || cli.metadata
}

pub fn use_sse(cli: &Cli) -> bool {
    !structured(cli) && terminal::is_stdout_terminal()
}

pub fn require_summary(result: Result<Summary, SummaryError>) -> Summary {
    result.unwrap_or_else(|error| {
        eprintln!("[ai-summary] Summary failed: {error}");
        std::process::exit(2);
    })
}

pub fn emit_json(mut fields: Value) {
    fields["schema_version"] = json!(1);
    println!("{fields}");
}

pub fn emit(cli: &Cli, summary: &Summary, raw_bytes: u64, fields: Value) {
    if structured(cli) {
        let mut fields = fields.as_object().cloned().unwrap_or_default();
        fields.insert("summary".into(), json!(summary.text));
        if cli.metadata {
            fields.insert("provider".into(), json!(summary.provider));
            fields.insert("model".into(), json!(summary.model));
        }
        emit_json(Value::Object(fields));
        return;
    }
    let usage = summary
        .usage
        .as_ref()
        .map(|usage| {
            format!(
                " | tokens: {}+{}",
                usage.prompt_tokens, usage.completion_tokens
            )
        })
        .unwrap_or_default();
    println!(
        "{}\n\n---\n[ai-summary] {}{} | {} -> {}",
        summary.text,
        summary.model,
        usage,
        fmtn(raw_bytes),
        fmtn(summary.text.len() as u64)
    );
}
