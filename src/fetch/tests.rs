//! Verify extraction helpers used by the fetch adapters.
//! Uses local strings and the optional browser capability check.

use super::*;
use crate::fetch_utils::strip_ansi;
use http::is_text_response;
#[test]
fn strips_html_tags() {
    let html = "<body><p>hello <span>world</span></p></body>";
    assert_eq!(strip_html(html), "hello world");
}
#[test]
fn strips_color_codes() {
    assert_eq!(strip_ansi("\x1b[32mhello\x1b[0m"), "hello");
}
#[test]
fn has_agent_browser_returns_bool() {
    let _: bool = browser::has_agent_browser();
}

#[test]
fn empty_content_type_is_allowed_for_text_fallback() {
    assert!(is_text_response("", "https://example.com/article"));
}

#[test]
fn binary_content_type_is_rejected_for_text_fallback() {
    assert!(!is_text_response(
        "image/png",
        "https://example.com/image.png"
    ));
}
