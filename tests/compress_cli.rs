// End-to-end CLI coverage for stdin compression flows.
// Exports: integration tests for `ai-summary compress`.
// Deps: assert_cmd, predicates.

use assert_cmd::Command;
use predicates::prelude::*;

fn cmd() -> Command {
    Command::cargo_bin("ai-summary").unwrap()
}

#[test]
fn compress_strips_html_from_stdin() {
    let input = "<html><body><main>Important release notes stay visible.</main></body></html>";

    cmd()
        .args(["compress", "-m", "200"])
        .write_stdin(input)
        .assert()
        .success()
        .stdout(predicate::str::contains("Important release notes stay visible"))
        .stdout(predicate::str::contains("<html>").not());
}
