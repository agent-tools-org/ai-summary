// Contract tests for acquisition, local utilities, and durable state.
// Drives the installed CLI interface with disposable homes and HTTP fixtures.

mod summary_fixture;

use serde_json::{json, Value};
use std::fs;
use summary_fixture::{Fixture, Server};

#[test]
fn fetch_preserves_input_order_and_reports_partial_failure() {
    let slow = Server::new(200, &"slow page ".repeat(24));
    let fast = Server::new(200, &"fast page ".repeat(24));
    let missing = Server::new(404, "not found");
    slow.set_delay(std::time::Duration::from_millis(180));
    let fixture = Fixture::new();
    let result = fixture.run(
        &[
            "--raw",
            "--json",
            "fetch",
            &slow.url,
            &missing.url,
            &fast.url,
        ],
        "",
    );
    assert!(result.status.success());
    let value: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["pages"][0]["url"], slow.url);
    assert_eq!(value["pages"][1]["url"], fast.url);
    assert_eq!(value["failed"][0]["url"], missing.url);
    assert_eq!(value["status"], "partial");
}

#[test]
fn uninstall_removes_only_its_own_hook_from_a_mixed_group() {
    let fixture = Fixture::new();
    let folder = fixture.path(".claude/hooks");
    fs::create_dir_all(&folder).unwrap();
    let owned = folder.join("ai-summary-prebash.sh");
    let settings = fixture.path(".claude/settings.json");
    fs::write(
        &settings,
        json!({"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[
            {"type":"command","command":owned}, {"type":"command","command":"other-hook"},
        ]}]}})
        .to_string(),
    )
    .unwrap();
    let result = fixture.run(&["init", "--uninstall"], "");
    assert!(result.status.success());
    let value: Value = serde_json::from_slice(&fs::read(settings).unwrap()).unwrap();
    let hooks = value["hooks"]["PreToolUse"][0]["hooks"].as_array().unwrap();
    assert_eq!(hooks.len(), 1);
    assert_eq!(hooks[0]["command"], "other-hook");
}

#[test]
fn compress_byte_limit_preserves_utf8() {
    let fixture = Fixture::new();
    let result = fixture.run(&["compress", "-m", "10"], &"你好世界。".repeat(12));
    assert!(result.status.success());
    assert_eq!(String::from_utf8(result.stdout).unwrap(), "你好世");
}

#[test]
fn malformed_stats_are_preserved_when_writers_run() {
    let fixture = Fixture::new();
    let path = fixture.path(".ai-summary/stats.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, "{bad json").unwrap();
    let result = fixture.run(&["compress", "-m", "12"], &"test input ".repeat(12));
    assert!(result.status.success());
    assert_eq!(fs::read_to_string(path).unwrap(), "{bad json");
}

#[test]
fn wrap_preserves_unknown_stdout_stderr_and_exit() {
    let fixture = Fixture::new();
    let result = fixture.run(
        &[
            "wrap",
            "/bin/sh",
            "-c",
            "printf '%s' \"$1\"; printf diagnostic >&2; exit 73",
            "fixture",
            "two words",
        ],
        "",
    );
    assert_eq!(result.status.code(), Some(73));
    assert_eq!(result.stdout, b"two words");
    assert_eq!(result.stderr, b"diagnostic");
}

#[test]
fn install_reinstall_uninstall_preserve_existing_claude_settings() {
    let fixture = Fixture::new();
    let root = fixture.path(".claude");
    fs::create_dir(&root).unwrap();
    let settings = root.join("settings.json");
    let prompt = root.join("CLAUDE.md");
    let original = json!({"theme":"dark","hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[
        {"type":"command","command":"other-hook"}
    ]}]}});
    fs::write(&settings, original.to_string()).unwrap();
    fs::write(&prompt, "Existing instructions.\n\n").unwrap();
    assert!(fixture.run(&["init"], "").status.success());
    let first = fs::read(&settings).unwrap();
    assert!(fixture.run(&["init"], "").status.success());
    assert_eq!(first, fs::read(&settings).unwrap());
    assert!(fixture.run(&["init", "--uninstall"], "").status.success());
    let actual: Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(actual, original);
    assert_eq!(
        fs::read_to_string(prompt).unwrap(),
        "Existing instructions.\n\n"
    );
}

#[test]
fn concurrent_writes_migrate_legacy_stats_without_losing_data() {
    let fixture = Fixture::new();
    let path = fixture.path(".ai-summary/stats.json");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        json!({
            "total_searches":12, "total_pages_fetched":17, "total_raw_chars":0,
            "total_summary_chars":0, "total_prompt_tokens":0, "total_completion_tokens":0,
            "estimated_claude_tokens_saved":0, "total_duration_secs":0.0, "history":[]
        })
        .to_string(),
    )
    .unwrap();
    std::thread::scope(|scope| {
        for _ in 0..12 {
            scope.spawn(|| {
                let output = fixture.run(&["compress", "-m", "12"], &"fixture input ".repeat(14));
                assert!(output.status.success());
            });
        }
    });
    let actual: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(actual["schema_version"], 1);
    assert_eq!(actual["total_searches"], 12);
    assert_eq!(actual["total_pages_fetched"], 17);
    assert_eq!(actual["history"].as_array().unwrap().len(), 12);
}
