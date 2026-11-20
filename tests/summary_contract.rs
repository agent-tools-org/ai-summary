// Exercise summary and configuration contracts through the shipped CLI.
// Uses isolated homes and loopback HTTP fixtures without real providers.

mod summary_fixture;

use serde_json::Value;
use std::fs;
use summary_fixture::{Fixture, Server};

#[test]
fn summary_failure_is_nonzero_and_does_not_record_savings() {
    let server = Server::new(503, r#"{"error":{"message":"fixture unavailable"}}"#);
    let fixture = Fixture::new();
    let result = fixture.run(
        &["--json", "--api-url", &server.url, "sum", "question"],
        "input",
    );
    assert_eq!(result.status.code(), Some(2));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("fixture unavailable"));
    assert!(!fixture.path(".ai-summary/stats.json").exists());
}

#[test]
fn raw_json_fetch_never_calls_summary_backend() {
    let page = Server::new(200, &"fixture article text ".repeat(30));
    let backend = Server::new(
        200,
        r#"{"choices":[{"message":{"content":"unexpected summary"}}]}"#,
    );
    let fixture = Fixture::new();
    let result = fixture.run(
        &[
            "--api-url",
            &backend.url,
            "--raw",
            "--json",
            "fetch",
            &page.url,
        ],
        "",
    );
    assert!(result.status.success());
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(json["pages"][0]["text"]
        .as_str()
        .unwrap()
        .contains("fixture article"));
    assert!(backend.requests().is_empty());
}

#[test]
fn local_service_key_is_not_sent_to_another_origin() {
    let server = Server::new(
        200,
        r#"{"choices":[{"message":{"content":"fixture answer"}}]}"#,
    );
    let fixture = Fixture::new();
    fs::create_dir(fixture.path(".omlx")).unwrap();
    fs::write(
        fixture.path(".omlx/settings.json"),
        r#"{"auth":{"api_key":"synthetic-local-value"}}"#,
    )
    .unwrap();
    let result = fixture.run(&["--api-url", &server.url, "--metadata", "sum"], "input");
    assert!(result.status.success());
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].to_ascii_lowercase().contains("authorization:"));
    let json: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(json["provider"], "http");
}

#[test]
fn setup_saves_a_usable_key_and_config_redacts_it() {
    let fixture = Fixture::new();
    let saved = fixture.run(
        &["setup", "--openrouter-key", "synthetic-fixture-value"],
        "",
    );
    assert!(saved.status.success());
    let status = fixture.run(&["setup"], "");
    let status = String::from_utf8_lossy(&status.stdout);
    assert!(status
        .lines()
        .any(|line| line.contains("OpenRouter API (") && line.ends_with("— configured")));
    let config = fixture.run(&["config"], "");
    assert!(!String::from_utf8_lossy(&config.stdout).contains("synthetic-fixture-value"));
}

#[test]
fn fetch_failure_is_never_cached() {
    let page = Server::new(200, &"fixture article text ".repeat(30));
    let failed = Server::new(503, r#"{"error":{"message":"temporary failure"}}"#);
    let fixture = Fixture::new();
    let failure = fixture.run(
        &["--json", "--api-url", &failed.url, "fetch", &page.url],
        "",
    );
    assert_eq!(failure.status.code(), Some(2));
    failed.set_response(200, r#"{"choices":[{"message":{"content":"recovered"}}]}"#);
    let success = fixture.run(
        &["--metadata", "--api-url", &failed.url, "fetch", &page.url],
        "",
    );
    let value: Value = serde_json::from_slice(&success.stdout).unwrap();
    assert!(value["summary"].as_str().unwrap().contains("recovered"));
    assert_eq!(value["cached"], false);
}

#[test]
fn raw_fetch_handles_utf8_at_the_display_limit() {
    let page = Server::new(200, &format!("A{}", "你".repeat(1400)));
    let fixture = Fixture::new();
    let result = fixture.run(&["--raw", "fetch", &page.url], "");
    assert!(result.status.success());
    assert!(String::from_utf8(result.stdout).unwrap().contains("你"));
}

#[test]
fn cache_hit_preserves_provenance_and_model_changes_miss() {
    let page = Server::new(200, &"fixture article text ".repeat(30));
    let backend = Server::new(
        200,
        r#"{"choices":[{"message":{"content":"fixture summary"}}]}"#,
    );
    let fixture = Fixture::new();
    let fetch = |model| {
        let output = fixture.run(
            &[
                "--metadata",
                "--api-url",
                &backend.url,
                "--model",
                model,
                "fetch",
                &page.url,
            ],
            "",
        );
        assert!(output.status.success());
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    assert_eq!(fetch("first")["cached"], false);
    let hit = fetch("first");
    assert_eq!(hit["summary"], "fixture summary");
    assert_eq!(hit["provider"], "http");
    assert_eq!(hit["model"], "first");
    assert_eq!(hit["cached"], true);
    assert_eq!(fetch("second")["cached"], false);
    assert_eq!(backend.requests().len(), 2);
}

#[test]
fn changing_endpoint_drops_the_key_from_the_original_config() {
    let backend = Server::new(
        200,
        r#"{"choices":[{"message":{"content":"fixture summary"}}]}"#,
    );
    let fixture = Fixture::new();
    fs::create_dir(fixture.path(".ai-summary")).unwrap();
    fs::write(
        fixture.path(".ai-summary/config.toml"),
        r#"api_url = "http://localhost:8000"
api_key = "synthetic-config-value"
model = "fixture-model"
"#,
    )
    .unwrap();
    let result = fixture.run(&["--api-url", &backend.url, "sum"], "input");
    assert!(result.status.success());
    let requests = backend.requests();
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].to_ascii_lowercase().contains("authorization:"));
}
