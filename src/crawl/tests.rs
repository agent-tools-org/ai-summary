//! Exercise crawl submission, polling, and pagination against local HTTP.
//! No service token or live Cloudflare request is involved.

use super::*;
use reqwest::Url;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};

#[test]
fn exhausted_crawl_retains_records_from_multiple_cursor_pages() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = Url::parse(&format!("http://{}/crawl", listener.local_addr().unwrap())).unwrap();
    let server = std::thread::spawn(move || {
        let replies = [
            ("POST /crawl ", json!({"success":true,"result":"job-id"})),
            (
                "GET /crawl/job-id?limit=1 ",
                json!({"success":true,"result":{"status":"cancelled_due_to_limits"}}),
            ),
            (
                "GET /crawl/job-id?limit=2 ",
                json!({"success":true,"result":{
                    "status":"cancelled_due_to_limits", "cursor":1,
                    "records":[{"url":"https://example.com/a","status":"completed","markdown":"你好"}]
                }}),
            ),
            (
                "GET /crawl/job-id?limit=1&cursor=1 ",
                json!({"success":true,"result":{
                    "status":"cancelled_due_to_limits",
                    "records":[{"url":"https://example.com/b","status":"errored","outcome":"page_error"}]
                }}),
            ),
        ];
        for (path, body) in replies {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.starts_with(path), "unexpected request: {line}");
            while line != "\r\n" {
                line.clear();
                if reader.read_line(&mut line).unwrap() == 0 {
                    break;
                }
            }
            let body = body.to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let api = CrawlApi {
        client: &client,
        endpoint,
        token: "fixture",
        deadline: Instant::now() + Duration::from_secs(3),
    };
    let (status, records) = api.run("https://example.com", 2, 1).unwrap();
    server.join().unwrap();
    assert_eq!(status, Status::CancelledDueToLimits);
    assert_eq!(records[0].markdown.as_deref(), Some("你好"));
    assert_eq!(records[1].outcome.as_deref(), Some("page_error"));
}
