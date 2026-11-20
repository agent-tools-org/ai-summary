// Helper routines for fetch operations: HTML cleanup, text extraction, redirects.
// Exports: collapse_ws(), extract_redirect(), strip_html(), strip_ansi(), extracted_text().
// Deps: readability extractor, scraper, std, defuddle CLI (optional).

use crate::subprocess;
use readability::extractor;
use scraper::{Html, Selector};
use std::{io::Write, process::Command, sync::OnceLock, time::Duration};

fn defuddle_extract(html: &str) -> Option<String> {
    static DETECTED: OnceLock<bool> = OnceLock::new();
    if !*DETECTED.get_or_init(|| subprocess::available(Command::new("defuddle").arg("--version"))) {
        return None;
    }
    // A file operand works on both installed and current Defuddle releases.
    let mut source = tempfile::Builder::new().suffix(".html").tempfile().ok()?;
    source.write_all(html.as_bytes()).ok()?;
    let result = subprocess::capture(
        Command::new("defuddle")
            .arg("parse")
            .arg(source.path())
            .arg("--markdown"),
        None,
        Duration::from_secs(5),
        400_000,
    );
    let output = match result {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            eprintln!(
                "[ai-summary] defuddle exited with {}; using built-in extraction",
                output.status
            );
            return None;
        }
        Err(error) => {
            eprintln!("[ai-summary] defuddle: {error}; using built-in extraction");
            return None;
        }
    };
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (text.len() >= 200).then_some(text)
}

pub fn extracted_text(url: &str, body: &str) -> (&'static str, String) {
    let stripped = strip_html(body);
    // Try defuddle first — accept if it returns enough content relative to raw text.
    if let Some(md) = defuddle_extract(body) {
        if md.len() > stripped.len() / 4 {
            return ("defuddle", md);
        }
    }
    match extractor::extract(
        &mut body.as_bytes(),
        &url.parse()
            .unwrap_or_else(|_| "https://example.com".parse().unwrap()),
    ) {
        Ok(product) if product.text.len() > stripped.len() / 4 && product.text.len() >= 200 => {
            ("readability", product.text)
        }
        _ => ("html_text", stripped),
    }
}

pub fn extract_redirect(html: &str) -> Option<String> {
    for pattern in &[
        "window.location.replace(\"",
        "window.location=\"",
        "window.location = \"",
    ] {
        if let Some(index) = html.find(pattern) {
            let start = index + pattern.len();
            if let Some(end) = html[start..].find('"') {
                let url = &html[start..start + end];
                if url.starts_with("http") {
                    return Some(url.to_string());
                }
            }
        }
    }
    let lower = html.to_ascii_lowercase();
    if let Some(index) = lower.find("url=") {
        let rest = html.get(index + 4..)?;
        let end = rest.find(['"', '\'', '>']).unwrap_or(rest.len());
        let url = rest[..end].trim();
        if url.starts_with("http") {
            return Some(url.to_string());
        }
    }
    None
}

pub fn strip_html(html: &str) -> String {
    let doc = Html::parse_document(html);
    let selector = Selector::parse("body").unwrap();
    let text = doc
        .select(&selector)
        .next()
        .map(|element| element.text().collect::<String>())
        .unwrap_or_else(|| doc.root_element().text().collect::<String>());
    collapse_ws(text.trim())
}

pub fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut sp = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !sp {
                out.push(' ');
                sp = true;
            }
        } else {
            out.push(ch);
            sp = false;
        }
    }
    out
}

pub(crate) fn strip_ansi(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b {
            index += 1;
            if bytes.get(index) == Some(&b'[') {
                index += 1;
                while index < bytes.len() && !(0x40..=0x7e).contains(&bytes[index]) {
                    index += 1;
                }
            } else if bytes.get(index) == Some(&b']') {
                index += 1;
                while index < bytes.len()
                    && bytes[index] != 0x07
                    && !(bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'\\'))
                {
                    index += 1;
                }
                if bytes.get(index) == Some(&0x1b) {
                    index += 1;
                }
            }
            index += 1;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
