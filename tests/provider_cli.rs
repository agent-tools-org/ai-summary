//! Drive subprocess provider adapters with disposable executable fixtures.
//! Verifies argv boundaries, stdin transport, and nonzero error handling.

#![cfg(unix)]
#[allow(dead_code)]
mod summary_fixture;

use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
use summary_fixture::Fixture;

fn stub(fixture: &Fixture, name: &str, script: &str) -> PathBuf {
    let bin = fixture.path("bin");
    fs::create_dir_all(&bin).unwrap();
    let executable = bin.join(name);
    fs::write(&executable, script).unwrap();
    fs::set_permissions(executable, fs::Permissions::from_mode(0o700)).unwrap();
    bin
}

#[test]
fn github_percent_encodes_each_file_path_component() {
    let fixture = Fixture::new();
    let bin = stub(
        &fixture,
        "gh",
        r#"#!/bin/sh
[ "$1" = api ] && [ "$2" = repos/owner/repo/contents/src/file%20name.rs ] || exit 11
if [ "$3" = -H ]; then
    printf 'fixture file body'
else
    printf '%s' '{"path":"src/file name.rs","size":17}'
fi
"#,
    );
    let output = fixture.run_with_path(
        &["--raw", "github", "owner/repo", "src/file name.rs"],
        "",
        &bin,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("fixture file body"));
}

#[test]
fn repomix_preserves_include_glob_as_one_argument() {
    let fixture = Fixture::new();
    let bin = stub(
        &fixture,
        "repomix",
        r#"#!/bin/sh
[ "$#" = 6 ] && [ "$2" = owner/repo ] && [ "$6" = 'src/**/*.rs,docs/Space Here.md' ] || exit 11
printf 'packed fixture'
"#,
    );
    let output = fixture.run_with_path(
        &[
            "--raw",
            "repo",
            "owner/repo",
            "-I",
            "src/**/*.rs,docs/Space Here.md",
        ],
        "",
        &bin,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"packed fixture\n");
}

#[test]
fn opencode_receives_large_summary_input_over_stdin() {
    let fixture = Fixture::new();
    let bin = stub(
        &fixture,
        "opencode",
        r#"#!/bin/sh
if [ "$1" = --version ]; then
    exit 0
fi
[ "$1" = run ] && [ "$#" -eq 7 ] || exit 11
bytes=$(/bin/cat | /usr/bin/wc -c)
[ "$bytes" -gt 131072 ] || exit 12
printf '%s\n' '{"type":"text","part":{"text":"fixture summary"}}'
"#,
    );
    let output =
        fixture.run_with_path(&["--json", "sum"], &"fixture content ".repeat(10_000), &bin);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("fixture summary"));
}

#[test]
fn github_preserves_failed_child_status_in_diagnostics() {
    let fixture = Fixture::new();
    let bin = stub(
        &fixture,
        "gh",
        "#!/bin/sh\nprintf 'fixture outage' >&2\nexit 42\n",
    );
    let output = fixture.run_with_path(&["--raw", "github", "owner/repo"], "", &bin);
    assert_eq!(output.status.code(), Some(2));
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(diagnostics.contains("exit code 42"));
    assert!(diagnostics.contains("fixture outage"));
}
