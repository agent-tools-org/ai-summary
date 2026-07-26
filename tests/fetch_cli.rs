// End-to-end CLI coverage for local fetch flows.
// Exports: integration tests for `ai-summary fetch`.
// Deps: assert_cmd, predicates, std::net local HTTP fixture.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

fn cmd() -> Command {
    Command::cargo_bin("ai-summary").unwrap()
}

#[test]
fn fetch_raw_reads_text_response_without_content_type() {
    let body = "Alpha local fixture content. ".repeat(8);
    let url = serve_once(&body);

    cmd()
        .args(["--raw", "fetch", &url])
        .assert()
        .success()
        .stdout(predicate::str::contains("Alpha local fixture content"));
}

fn serve_once(body: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let response_body = body.to_string();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    format!("http://{addr}/article")
}
