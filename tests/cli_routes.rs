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
// Backwards compatibility: bare words treated as search
// (the v2.7.0 fix -- bare "ai-summary what is rust" should not fail with
//  InvalidSubcommand but instead be re-parsed as "search what is rust")
// ---------------------------------------------------------------------------

#[test]
fn bare_words_fallback_to_search() {
    // "what" is not a known subcommand -- the fallback should re-parse as
    // `search what is rust`. The command will fail because there is no
    // network/LLM backend, but it should NOT exit 2 (clap parse error).
    // It should get past parsing and fail at search execution.
    cmd()
        .args(["what", "is", "rust"])
        .assert()
        .stderr(predicate::str::contains("unexpected argument").not())
        .stderr(predicate::str::contains("unrecognized subcommand").not());
}

#[test]
fn bare_single_word_fallback_to_search() {
    cmd()
        .args(["kubernetes"])
        .assert()
        .stderr(predicate::str::contains("unexpected argument").not())
        .stderr(predicate::str::contains("unrecognized subcommand").not());
}

#[test]
fn bare_words_with_global_flag_fallback() {
    // "what" is not a subcommand, but --raw is a global flag placed after it.
    // The fallback re-inserts "search" so the final parse is:
    //   ai-summary search what is rust --raw
    // which clap should accept (no parse error).
    // May exit 2 on CI (no API keys = no results) — that's fine, it parsed.
    let assert = cmd().args(["what", "is", "rust", "--raw"]).assert();
    assert
        .stderr(predicate::str::contains("unexpected argument").not())
        .stderr(predicate::str::contains("unrecognized subcommand").not());
}

// ---------------------------------------------------------------------------
// The critical bug scenario: "github" must NOT be swallowed as a search term.
// Before v2.7.0, `Commands` was optional with a positional `query: Vec<String>`
// that would eat "github" as a query word instead of routing to the subcommand.
// ---------------------------------------------------------------------------

#[test]
fn github_is_recognized_as_subcommand_not_search_word() {
    // "github owner/repo" must route to the Github variant, not be treated as
    // search query "github owner/repo".
    // It may exit non-zero (gh CLI / no results) but stderr must NOT contain
    // clap parse errors or search-mode output.
    cmd().args(["github", "owner/repo"]).assert().stderr(
        predicate::str::contains("error: unexpected argument")
            .not()
            .and(predicate::str::contains("error: unrecognized subcommand").not())
            .and(predicate::str::contains("Searching via Gemini").not()),
    );
}

#[test]
fn github_with_flags_parses_correctly() {
    // The full github invocation with all flags should parse without error.
    // It may exit non-zero (e.g. no results from gh CLI) but stderr must NOT
    // contain a clap parse error.
    cmd()
        .args([
            "github",
            "some search query",
            "-r",
            "owner/repo",
            "-l",
            "rust",
            "-p",
            "explain the architecture",
        ])
        .assert()
        .stderr(predicate::str::contains("error: unexpected argument").not())
        .stderr(predicate::str::contains("error: unrecognized subcommand").not());
}

#[test]
fn github_with_repo_flag_only() {
    // May exit non-zero (gh CLI failure / no results) but must not be a parse error.
    cmd()
        .args(["github", "hooks lifecycle", "-r", "anthropics/claude-code"])
        .assert()
        .stderr(predicate::str::contains("error: unexpected argument").not())
        .stderr(predicate::str::contains("error: unrecognized subcommand").not());
}

#[test]
fn github_with_language_flag_only() {
    cmd()
        .args(["github", "async runtime", "-l", "rust"])
        .assert()
        .stderr(predicate::str::contains("error: unexpected argument").not())
        .stderr(predicate::str::contains("error: unrecognized subcommand").not());
}

#[test]
fn github_with_prompt_flag_only() {
    cmd()
        .args(["github", "owner/repo", "-p", "what does this do"])
        .assert()
        .stderr(predicate::str::contains("error: unexpected argument").not())
        .stderr(predicate::str::contains("error: unrecognized subcommand").not());
}
