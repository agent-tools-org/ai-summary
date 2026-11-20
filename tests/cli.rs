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
// Basic binary sanity
// ---------------------------------------------------------------------------

#[test]
fn version_flag() {
    cmd()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::starts_with("ai-summary "));
}

#[test]
fn help_flag_short() {
    cmd()
        .arg("-h")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary"));
}

#[test]
fn help_flag_long() {
    cmd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Search the web, fetch pages, or pipe text",
        ));
}

#[test]
fn no_args_shows_usage_and_exits_2() {
    cmd()
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Usage: ai-summary"));
}

// ---------------------------------------------------------------------------
// Subcommand routing: every subcommand's --help succeeds
// ---------------------------------------------------------------------------

#[test]
fn search_help() {
    cmd()
        .args(["search", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Search the web and summarize results",
        ));
}

#[test]
fn fetch_help() {
    cmd()
        .args(["fetch", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary fetch"));
}

#[test]
fn github_help() {
    cmd().args(["github", "--help"]).assert().success().stdout(
        predicate::str::contains("Search GitHub code or read files from repos")
            .and(predicate::str::contains("-r, --repo"))
            .and(predicate::str::contains("-l, --language"))
            .and(predicate::str::contains("-p, --prompt")),
    );
}

#[test]
fn repo_help() {
    cmd().args(["repo", "--help"]).assert().success().stdout(
        predicate::str::contains("Pack a remote GitHub repo with repomix")
            .and(predicate::str::contains("-p, --prompt"))
            .and(predicate::str::contains("-I, --include")),
    );
}

#[test]
fn stats_help() {
    cmd()
        .args(["stats", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary stats"));
}

#[test]
fn reset_stats_help() {
    cmd()
        .args(["reset-stats", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary reset-stats"));
}

#[test]
fn config_help() {
    cmd()
        .args(["config", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary config"));
}

#[test]
fn bench_help() {
    cmd()
        .args(["bench", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark fetch + summarization"));
}

#[test]
fn crawl_help() {
    cmd().args(["crawl", "--help"]).assert().success().stdout(
        predicate::str::contains("Usage: ai-summary crawl")
            .and(predicate::str::contains("-l, --limit"))
            .and(predicate::str::contains("-d, --depth")),
    );
}

#[test]
fn compress_help() {
    cmd()
        .args(["compress", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary compress"));
}

#[test]
fn wrap_help() {
    cmd()
        .args(["wrap", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Run a command, compress large passing test output",
        ));
}

#[test]
fn summarize_help() {
    cmd()
        .args(["summarize", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary summarize"));
}

#[test]
fn init_help() {
    cmd().args(["init", "--help"]).assert().success().stdout(
        predicate::str::contains("Install Claude Code integration")
            .and(predicate::str::contains("--uninstall"))
            .and(predicate::str::contains("--with-repomix")),
    );
}

// ---------------------------------------------------------------------------
// Subcommand aliases
// ---------------------------------------------------------------------------

#[test]
fn alias_s_routes_to_search() {
    cmd()
        .args(["s", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Search the web and summarize results",
        ));
}

#[test]
fn alias_sum_routes_to_summarize() {
    cmd()
        .args(["sum", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary summarize"));
}
