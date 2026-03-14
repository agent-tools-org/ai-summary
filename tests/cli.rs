// Integration tests for CLI argument parsing and subcommand routing.
// Validates that clap parses all subcommands, flags, aliases, and the
// bare-word fallback correctly -- without making any network calls.

use assert_cmd::Command;
use predicates::prelude::*;

fn cmd() -> Command {
    Command::cargo_bin("ai-summary").unwrap()
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
        .stdout(predicate::str::contains("Search the web, fetch pages, or pipe text"));
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
        .stdout(predicate::str::contains("Search the web and summarize results"));
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
    cmd()
        .args(["github", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Search GitHub code or read files from repos")
                .and(predicate::str::contains("-r, --repo"))
                .and(predicate::str::contains("-l, --language"))
                .and(predicate::str::contains("-p, --prompt")),
        );
}

#[test]
fn repo_help() {
    cmd()
        .args(["repo", "--help"])
        .assert()
        .success()
        .stdout(
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
    cmd()
        .args(["crawl", "--help"])
        .assert()
        .success()
        .stdout(
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
        .stdout(predicate::str::contains("Run a command, compress large passing test output"));
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
    cmd()
        .args(["init", "--help"])
        .assert()
        .success()
        .stdout(
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
        .stdout(predicate::str::contains("Search the web and summarize results"));
}

#[test]
fn alias_sum_routes_to_summarize() {
    cmd()
        .args(["sum", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage: ai-summary summarize"));
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
    let assert = cmd()
        .args(["what", "is", "rust", "--raw"])
        .assert();
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
    cmd()
        .args(["github", "owner/repo"])
        .assert()
        .stderr(
            predicate::str::contains("error: unexpected argument").not()
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
            "-r", "owner/repo",
            "-l", "rust",
            "-p", "explain the architecture",
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

// ---------------------------------------------------------------------------
// Global flags work with subcommands
// ---------------------------------------------------------------------------

#[test]
fn global_raw_with_search() {
    cmd()
        .args(["search", "--help", "--raw"])
        .assert()
        .success();
}

#[test]
fn global_json_with_stats() {
    // stats --json should run and succeed (no network needed)
    cmd()
        .args(["stats", "--json"])
        .assert()
        .success();
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
    cmd()
        .args(["--raw", "search", "--help"])
        .assert()
        .success();
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
    cmd()
        .args(["crawl", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("[default: 10]")
                .and(predicate::str::contains("[default: 2]")),
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
    cmd()
        .arg("--foo")
        .assert()
        .code(2);
}

// ---------------------------------------------------------------------------
// Subcommands that run without network (smoke tests)
// ---------------------------------------------------------------------------

#[test]
fn stats_runs_successfully() {
    cmd()
        .args(["stats"])
        .assert()
        .success();
}

#[test]
fn config_runs_successfully() {
    cmd()
        .args(["config"])
        .assert()
        .success();
}

#[test]
fn stats_json_outputs_valid_json() {
    let output = cmd()
        .args(["stats", "--json"])
        .assert()
        .success();
    let out = output.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should be valid JSON
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_ok(),
        "stats --json should output valid JSON, got: {}",
        stdout,
    );
}
