//! Reproduce live-use failures through the CLI with controlled inputs.
//! The loopback server keeps large-document tests deterministic.

#[allow(dead_code)]
mod summary_fixture;

use serde_json::Value;
use summary_fixture::{Fixture, Server};

#[test]
fn isolated_long_html_reaches_the_requested_section() {
    let body = format!("<html><body><nav>{}</nav><section><h2 id='target'>Wanted section</h2><p>Primary evidence is here.</p></section><section>Other section</section></body></html>",
        "Navigation ".repeat(20_000));
    let server = Server::new(200, &body);
    let fixture = Fixture::new();
    let url = format!("{}/reference.html#target", server.url);
    let result = fixture.run(&["--raw", "--json", "fetch", &url], "");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    let text = json["pages"][0]["text"].as_str().unwrap();
    assert!(text.contains("Primary evidence is here."));
    assert!(!text.contains("Navigation"));
    assert!(!text.contains("Other section"));
    assert!(json["pages"][0]["final_url"]
        .as_str()
        .unwrap()
        .ends_with("#target"));
}

#[test]
fn isolated_long_text_is_clipped_without_losing_a_working_response() {
    let body = "Long plain text starts here. ".repeat(100_000);
    let server = Server::new(200, &body);
    let fixture = Fixture::new();
    let url = format!("{}/reference.txt", server.url);
    let result = fixture.run(&["--raw", "--json", "fetch", &url], "");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    let page = &json["pages"][0];
    assert_eq!(page["extractor"], "text");
    assert_eq!(page["truncated"], true);
    assert!(page["text"]
        .as_str()
        .unwrap()
        .starts_with("Long plain text starts here."));
    assert!(page["text"].as_str().unwrap().len() <= 16_000);
}

#[test]
fn doctor_distinguishes_an_unreachable_loopback_and_unset_hosted_routes() {
    let fixture = Fixture::new();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let result = fixture.run(&["--json", "--api-url", &url, "doctor"], "");
    assert!(result.status.success());
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["summary"]["http"]["status"], "unreachable");
    assert_eq!(json["summary"]["openrouter"]["status"], "not_configured");
    assert_eq!(json["tools"]["opencode"]["status"], "missing");
}

#[test]
#[cfg(unix)]
fn defuddle_file_adapter_matches_the_installed_cli_contract() {
    let server = Server::new(
        200,
        &format!(
            "<html><article>{}</article></html>",
            "technical article text ".repeat(25)
        ),
    );
    let fixture = Fixture::new();
    executable(
        &fixture,
        "defuddle",
        r#"#!/bin/sh
[ "$1" = --version ] && { printf '0.13.0\n'; exit 0; }
[ "$1" = parse ] && [ -f "$2" ] && [ "$3" = --markdown ] || exit 1
/bin/cat "$2"
"#,
    );
    let url = format!("{}/article.html", server.url);
    let result = fixture.run_with_path(&["--raw", "--json", "fetch", &url], "", &fixture.path(""));
    assert!(result.status.success());
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["pages"][0]["extractor"], "defuddle");
}

#[test]
fn doctor_reports_a_payment_error_from_the_loopback_backend() {
    let fixture = Fixture::new();
    let server = Server::new(402, "{}");
    let result = fixture.run(&["--json", "--api-url", &server.url, "doctor"], "");
    assert!(result.status.success());
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["summary"]["http"]["status"], "credits_required");
    assert!(server.requests()[0].starts_with("GET /v1/models HTTP/"));
}

#[test]
#[cfg(unix)]
fn browser_uses_options_supported_by_the_installed_release() {
    let fixture = Fixture::new();
    executable(
        &fixture,
        "agent-browser",
        r#"#!/bin/sh
[ "$1" = --help ] && { printf '%s\n' '--config --session'; exit 0; }
[ "$1" = --config ] && [ "$(/bin/cat "$2")" = '{}' ] || exit 1
[ "$3" = --session ] && [ -n "$4" ] || exit 1
case "$5" in
    open|wait|close) exit 0 ;;
    get) printf 'Browser text extracted from an isolated session.' ;;
    *) exit 1 ;;
esac
"#,
    );
    let result = fixture.run_with_path(
        &[
            "fetch",
            "--browser",
            "--raw",
            "--json",
            "https://example.com",
        ],
        "",
        &fixture.path(""),
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["pages"][0]["extractor"], "browser");
    assert!(json["pages"][0]["text"]
        .as_str()
        .unwrap()
        .contains("isolated session"));
}

#[test]
#[cfg(unix)]
fn interruption_stops_opencode_and_its_child_without_touching_other_processes() {
    verify_interruption(libc::SIGINT);
    verify_interruption(libc::SIGTERM);
}

#[cfg(unix)]
fn verify_interruption(signal: i32) {
    use std::{
        fs,
        process::{Command, Stdio},
        time::Duration,
    };

    let fixture = Fixture::new();
    executable(
        &fixture,
        "opencode",
        r#"#!/bin/sh
[ "$1" = --version ] && { printf '1.0.0\n'; exit 0; }
/bin/sleep 10 &
printf '%s %s\n' "$$" "$!" > "$HOME/ready"
wait
"#,
    );
    fs::write(fixture.path("input"), "Summarize this document").unwrap();
    let mut other = Command::new("/bin/sleep").arg("10").spawn().unwrap();
    let mut cli = Command::new(env!("CARGO_BIN_EXE_ai-summary"))
        .env_clear()
        .env("HOME", fixture.path(""))
        .env("PATH", fixture.path(""))
        .arg("sum")
        .stdin(fs::File::open(fixture.path("input")).unwrap())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pids = (0..200).find_map(|_| {
        std::thread::sleep(Duration::from_millis(10));
        let text = fs::read_to_string(fixture.path("ready")).ok()?;
        let pids: Vec<i32> = text
            .split_whitespace()
            .filter_map(|pid| pid.parse().ok())
            .collect();
        (pids.len() == 2).then_some(pids)
    });
    unsafe { libc::kill(cli.id() as i32, signal) };
    let code = cli.wait().unwrap().code();
    let other_alive = other.try_wait().unwrap().is_none();
    let _ = other.kill();
    let _ = other.wait();
    for pid in pids.expect("opencode started with a child") {
        assert!(
            process_stopped(pid),
            "process {pid} still running after cancellation"
        );
    }
    assert_eq!(code, Some(128 + signal));
    assert!(other_alive);
}

#[cfg(unix)]
fn process_stopped(pid: i32) -> bool {
    (0..100).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(10));
        let output = std::process::Command::new("/bin/ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .unwrap();
        !output.status.success()
            || String::from_utf8_lossy(&output.stdout)
                .trim_start()
                .starts_with('Z')
    })
}

#[cfg(unix)]
fn executable(fixture: &Fixture, name: &str, script: &str) {
    use std::{fs, os::unix::fs::PermissionsExt};
    let path = fixture.path(name);
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
}
