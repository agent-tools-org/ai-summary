// Test-only process and loopback HTTP fixtures.
// Exports Fixture and Server for summary_contract.rs.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

static NEXT_HOME: AtomicUsize = AtomicUsize::new(0);

pub struct Fixture(PathBuf);

impl Fixture {
    pub fn new() -> Self {
        let sequence = NEXT_HOME.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "summary-contract-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    pub fn path(&self, path: &str) -> PathBuf {
        self.0.join(path)
    }

    pub fn run(&self, args: &[&str], input: &str) -> Output {
        self.run_with_path(args, input, std::path::Path::new("/nonexistent"))
    }

    pub fn run_with_path(&self, args: &[&str], input: &str, path: &std::path::Path) -> Output {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ai-summary"))
            .env_clear()
            .env("HOME", &self.0)
            .env("PATH", path)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if !input.is_empty() {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        }
        child.wait_with_output().unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct Server {
    pub url: String,
    #[allow(dead_code)]
    requests: Arc<Mutex<Vec<String>>>,
    worker: Option<JoinHandle<()>>,
    response: Arc<Mutex<(u16, String)>>,
    delay: Arc<Mutex<Duration>>,
}

impl Server {
    pub fn new(status: u16, body: &str) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&requests);
        let response = Arc::new(Mutex::new((status, body.to_owned())));
        let fixture_response = Arc::clone(&response);
        let delay = Arc::new(Mutex::new(Duration::ZERO));
        let response_delay = Arc::clone(&delay);
        let worker = thread::spawn(move || {
            for client in listener.incoming() {
                let Ok(stream) = client else { break };
                let delay = *response_delay.lock().unwrap();
                thread::sleep(delay);
                let (status, body) = fixture_response.lock().unwrap().clone();
                if !respond(stream, status, &body, &captured) {
                    break;
                }
            }
        });
        Self {
            url,
            requests,
            worker: Some(worker),
            response,
            delay,
        }
    }

    #[allow(dead_code)]
    pub fn set_response(&self, status: u16, body: &str) {
        *self.response.lock().unwrap() = (status, body.to_owned());
    }

    #[allow(dead_code)]
    pub fn set_delay(&self, delay: Duration) {
        *self.delay.lock().unwrap() = delay;
    }

    #[allow(dead_code)]
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let addr = self.url.trim_start_matches("http://");
        let _ = TcpStream::connect(addr);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn respond(mut stream: TcpStream, status: u16, body: &str, captured: &Mutex<Vec<String>>) -> bool {
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut reader = BufReader::new(&mut stream);
    let mut headers = String::new();
    let mut length = 0;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return false;
        }
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            length = value.trim().parse().unwrap();
        }
        headers.push_str(&line);
    }
    reader.read_exact(&mut vec![0; length]).unwrap();
    captured.lock().unwrap().push(headers);
    let _ = write!(
        stream,
        "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    true
}
