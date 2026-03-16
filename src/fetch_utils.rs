// Helper routines for fetch operations: HTML cleanup, text extraction, redirects.
// Exports: collapse_ws(), extract_redirect(), strip_html(), strip_ansi(), extracted_text(), truncate_text().
// Deps: readability extractor, scraper, std, defuddle CLI (optional).

use readability::extractor;
use scraper::{Html, Selector};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// Whether `defuddle` CLI is globally installed (checked once).
static HAS_DEFUDDLE: OnceLock<bool> = OnceLock::new();
static DEFUDDLE_SEQ: AtomicU64 = AtomicU64::new(0);

fn has_defuddle() -> bool {
    *HAS_DEFUDDLE.get_or_init(|| {
        let ok = Command::new("defuddle")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            eprintln!("[ai-summary] defuddle detected, using as content extractor");
        }
        ok
    })
}

fn defuddle_extract(_url: &str, html: &str) -> Option<String> {
    if !has_defuddle() {
        return None;
    }
    let seq = DEFUDDLE_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!("defuddle-{}-{seq}.html", std::process::id()));
    std::fs::write(&tmp, html).ok()?;
    let output = Command::new("defuddle")
        .args(["parse", tmp.to_str()?, "--markdown"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok();
    let _ = std::fs::remove_file(&tmp);
    let output = output?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.len() >= 200 {
        Some(text)
    } else {
        None
    }
}

pub fn extracted_text(url: &str, body: &str) -> String {
    let stripped = strip_html(body);
    // Try defuddle first — accept if it returns enough content relative to raw text.
    if let Some(md) = defuddle_extract(url, body) {
        if md.len() > stripped.len() / 4 {
            return md;
        }
    }
    match extractor::extract(
        &mut body.as_bytes(),
        &url
            .parse()
            .unwrap_or_else(|_| "https://example.com".parse().unwrap()),
    ) {
        Ok(product) if product.text.len() > stripped.len() / 4 && product.text.len() >= 200 => {
            product.text
        }
        _ => stripped,
    }
}

pub(crate) fn truncate_text(text: String, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
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
    let lower = html.to_lowercase();
    if let Some(index) = lower.find("url=") {
        let rest = &html[index + 4..];
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
