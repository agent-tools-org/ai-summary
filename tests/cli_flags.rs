// Integration tests for CLI argument parsing and subcommand routing.
// Validates that clap parses all subcommands, flags, aliases, and the
// bare-word fallback correctly -- without making any network calls.

use assert_cmd::Command;
use predicates::prelude::*;

fn cmd() -> Command {
    static HOME_DIR: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    let path = HOME_DIR.get_or_init(|| tempfile::tempdir().unwrap()).path();
    let mut command = Command::cargo_bin("ai-summary").unwrap();
    command
        .env_clear()
        .env("HOME", path)
        .env("PATH", "/nonexistent")
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9");
    command
}

// ---------------------------------------------------------------------------
// Global flags work with subcommands
// ---------------------------------------------------------------------------

#[test]
fn global_raw_with_search() {
    cmd().args(["search", "--help", "--raw"]).assert().success();
}

#[test]
fn global_json_with_stats() {
    // stats --json should run and succeed (no network needed)
    cmd().args(["stats", "--json"]).assert().success();
}

#[test]
fn global_deep_with_github() {
    cmd()
        .args(["github", "--help", "--deep"])
        .assert()
        .success();
}

#[test]
fn global_flags_before_subcommand() {
    // Global flags should work when placed before the subcommand
    cmd().args(["--raw", "search", "--help"]).assert().success();
}

#[test]
fn global_flags_mixed_positions() {
    cmd()
        .args(["--deep", "github", "--help", "--raw"])
        .assert()
        .success();
}

#[test]
fn global_api_url_env_var() {
    // --api-url should accept a value (testing via --help to avoid network)
    cmd()
        .args(["--api-url", "http://localhost:8080", "search", "--help"])
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Subcommand-specific flag parsing
// ---------------------------------------------------------------------------

#[test]
fn fetch_with_prompt_flag() {
    cmd()
        .args(["fetch", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-p, --prompt"));
}

#[test]
fn compress_with_max_chars_flag() {
    cmd()
        .args(["compress", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-m, --max-chars"));
}

#[test]
fn compress_with_source_flag() {
    cmd()
        .args(["compress", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-s, --source"));
}

#[test]
fn crawl_defaults_in_help() {
    cmd().args(["crawl", "--help"]).assert().success().stdout(
        predicate::str::contains("[default: 10]").and(predicate::str::contains("[default: 2]")),
    );
}

#[test]
fn repo_include_flag() {
    cmd()
        .args(["repo", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("-I, --include"));
}

// ---------------------------------------------------------------------------
// Error cases
// ---------------------------------------------------------------------------

#[test]
fn unknown_global_flag_errors() {
    cmd()
        .arg("--nonexistent")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn unknown_subcommand_flag_errors() {
    cmd()
        .args(["search", "--nonexistent"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("unexpected argument"));
}

#[test]
fn search_empty_query_exits_1() {
    // `search` with no query words should print usage and exit 1
    cmd()
        .args(["search"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("Usage: ai-summary <query>"));
}

#[test]
fn repo_requires_argument() {
    // `repo` without the required <REPO> argument should fail
    cmd()
        .args(["repo"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("<REPO>"));
}

#[test]
fn crawl_requires_url_argument() {
    cmd()
        .args(["crawl"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("<URL>"));
}

// ---------------------------------------------------------------------------
// Dash-prefixed args should NOT trigger bare-word fallback
// ---------------------------------------------------------------------------

#[test]
fn dash_prefixed_unknown_does_not_fallback() {
    // "--foo" starts with '-', so the fallback should NOT kick in.
    // clap should report it as an unexpected argument (exit 2).
    cmd().arg("--foo").assert().code(2);
}

// ---------------------------------------------------------------------------
// Subcommands that run without network (smoke tests)
// ---------------------------------------------------------------------------

#[test]
fn stats_runs_successfully() {
    cmd().args(["stats"]).assert().success();
}

#[test]
fn config_runs_successfully() {
    cmd().args(["config"]).assert().success();
}

#[test]
fn stats_json_outputs_valid_json() {
    let output = cmd().args(["stats", "--json"]).assert().success();
    let out = output.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should be valid JSON
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
        "stats --json should output valid JSON, got: {}",
        stdout,
    );
}
