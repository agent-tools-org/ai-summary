// Fast stdin compression without LLM usage.
// Exports: run_compress().
// Deps: crate::fetch::strip_html, crate::stats::{fmtn, load_stats, save_stats}, std::io::Read.

use std::io::Read;

use crate::fetch::strip_html;
use crate::stats::{fmtn, record_compress};

pub(crate) struct CompressionResult {
    pub text: String,
    pub truncated: bool,
}

pub(crate) fn compress_text(input: &str, max_chars: usize) -> CompressionResult {
    if input.is_empty() {
        return CompressionResult {
            text: String::new(),
            truncated: false,
        };
    }

    let cleaned = if input.contains("<html") || input.contains("<body") || input.contains("<div") {
        strip_html(input)
    } else {
        input.to_string()
    };

    if cleaned.len() <= max_chars {
        return CompressionResult {
            text: cleaned,
            truncated: false,
        };
    }

    let skip_patterns = [
        "cookie",
        "privacy policy",
        "terms of service",
        "subscribe",
        "sign up",
        "sign in",
        "log in",
        "advertisement",
        "sponsored",
        "all rights reserved",
        "loading...",
        "please enable javascript",
        "accept cookies",
        "newsletter",
    ];

    let mut out = String::with_capacity(max_chars);
    let lines: Vec<&str> = cleaned.lines().collect();
    let chunks: Vec<&str> = if lines.len() > 3 {
        lines
    } else {
        cleaned.split(". ").collect()
    };

    for chunk in &chunks {
        let trimmed = chunk.trim();
        if trimmed.is_empty() || trimmed.len() < 5 {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if skip_patterns.iter().any(|pattern| lower.contains(pattern)) {
            continue;
        }

        out.push_str(trimmed);
        if !trimmed.ends_with('.') && !trimmed.ends_with('\n') {
            out.push_str(". ");
        } else {
            out.push('\n');
        }

        if out.len() >= max_chars {
            out.truncate(max_chars);
            break;
        }
    }

    CompressionResult {
        text: out,
        truncated: true,
    }
}

pub fn run_compress(max_chars: usize, source: Option<&str>) {
    let mut input = String::new();
    if let Err(e) = std::io::stdin().read_to_string(&mut input) {
        eprintln!("[ai-summary] Failed to read stdin: {e}");
        std::process::exit(1);
    }
    let input = input.trim();
    if input.is_empty() {
        std::process::exit(1);
    }

    let raw_len = input.len();
    let result = compress_text(input, max_chars);
    if !result.truncated {
        let out_len = result.text.len();
        let compression = (1.0 - out_len as f64 / raw_len as f64) * 100.0;
        if compression > 5.0 {
            eprintln!(
                "[ai-summary compress] {} -> {} ({:.0}% reduction)",
                fmtn(raw_len as u64),
                fmtn(out_len as u64),
                compression
            );
        }
        println!("{}", result.text);
        return;
    }

    let out_len = result.text.len();
    let compression = (1.0 - out_len as f64 / raw_len as f64) * 100.0;
    eprintln!(
        "[ai-summary compress] {} -> {} ({:.0}% reduction)",
        fmtn(raw_len as u64),
        fmtn(out_len as u64),
        compression
    );

    record_compress(
        source.unwrap_or("compress"),
        raw_len as u64,
        out_len as u64,
    );

    print!("{}", result.text);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_returns_empty() {
        let result = compress_text("", 10);
        assert!(result.text.is_empty());
        assert!(!result.truncated);
    }

    #[test]
    fn short_input_passes_through() {
        let text = "a quick test";
        let result = compress_text(text, 20);
        assert_eq!(result.text, text);
        assert!(!result.truncated);
    }

    #[test]
    fn long_input_truncates_to_max() {
        let text = "one. two. three. four. five. six. seven. eight.";
        let result = compress_text(text, 20);
        assert!(result.truncated);
        assert_eq!(result.text.len(), 20);
    }

    #[test]
    fn keeps_sentence_boundaries_when_space_allows() {
        let text = "First sentence. Second sentence. Third sentence.";
        let result = compress_text(text, 33);
        assert!(result.truncated);
        assert_eq!(result.text, "First sentence. Second sentence. ");
        assert!(!result.text.contains("Third sentence"));
    }

    #[test]
    fn skip_patterns_are_removed_from_output() {
        let text = "Main idea stays. Privacy Policy update announced. Bonus sentence. Additional sentence to trigger truncation.";
        let result = compress_text(text, 70);
        assert!(result.text.contains("Main idea stays."));
        assert!(result.text.contains("Bonus sentence."));
        assert!(!result.text.to_lowercase().contains("privacy policy"));
        assert!(result.truncated);
    }

    #[test]
    fn respects_max_chars_across_chunks() {
        let text = "Alpha beta gamma. Delta epsilon zeta. Eta theta iota.";
        let result = compress_text(text, 25);
        assert!(result.truncated);
        assert_eq!(result.text.len(), 25);
    }

    #[test]
    fn compression_ratio_positive_when_truncated() {
        let text = "word ".repeat(30);
        let result = compress_text(&text, 60);
        assert!(result.truncated);
        let ratio = (1.0 - result.text.len() as f64 / text.len() as f64) * 100.0;
        assert!(ratio > 0.0);
    }
}
