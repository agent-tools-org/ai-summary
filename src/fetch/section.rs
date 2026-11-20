//! Find real HTML anchor targets and read their enclosing content section.
//! Keeps selectors static so fragments are treated as data.

use crate::fetch_utils::collapse_ws;
use reqwest::Url;
use scraper::{ElementRef, Html, Selector};

pub(super) fn extract(body: &str, fragment: &str) -> Option<String> {
    let document = Html::parse_document(body);
    let anchor = decode_fragment(fragment);
    let selectors = Selector::parse("[id], a[name]").expect("static selector");
    let element = document.select(&selectors).find(|element| {
        element.attr("id").or_else(|| element.attr("name")) == Some(anchor.as_str())
    })?;
    let container = element
        .ancestors()
        .filter_map(ElementRef::wrap)
        .find(|ancestor| ancestor.value().name() == "section");
    if let Some(container) = container {
        return Some(collapse_ws(&container.text().collect::<Vec<_>>().join(" ")));
    }
    let heading = element
        .parent()
        .and_then(ElementRef::wrap)
        .filter(|parent| heading_level(parent).is_some())
        .unwrap_or(element);
    let text = read_target(heading);
    (!text.is_empty()).then_some(text)
}

fn read_target(heading: ElementRef<'_>) -> String {
    let mut text = heading.text().collect::<Vec<_>>().join(" ");
    if let Some(level) = heading_level(&heading) {
        for node in heading.next_siblings().filter_map(ElementRef::wrap) {
            if heading_level(&node).is_some_and(|next| next <= level)
                || node.value().name() == "section"
            {
                break;
            }
            text.push(' ');
            text.push_str(&node.text().collect::<Vec<_>>().join(" "));
            if text.len() >= 64 * 1024 {
                break;
            }
        }
    }
    collapse_ws(text.trim())
}

fn heading_level(element: &ElementRef<'_>) -> Option<u8> {
    let bytes = element.value().name().as_bytes();
    match bytes {
        [b'h', level @ b'1'..=b'6'] => Some(level - b'0'),
        _ => None,
    }
}

fn decode_fragment(fragment: &str) -> String {
    let mut url = Url::parse("https://example.invalid/").expect("fixed URL");
    url.set_query(Some(&format!(
        "anchor={}",
        fragment.replace('+', "%2B").replace('&', "%26")
    )));
    url.query_pairs()
        .next()
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

pub(super) fn document_url(source: &Url) -> Option<Url> {
    // RFC Editor serves the full HTML separately from its bibliographic record.
    if !matches!(
        source.host_str(),
        Some("rfc-editor.org" | "www.rfc-editor.org")
    ) {
        return None;
    }
    let slug = source
        .path()
        .trim_end_matches('/')
        .strip_prefix("/info/rfc")?;
    if slug.is_empty() || !slug.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    source.join(&format!("/rfc/rfc{slug}.html")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_only_rfc_editor_records_to_document_bodies() {
        let record = Url::parse("https://www.rfc-editor.org/info/rfc9110/").unwrap();
        assert_eq!(
            document_url(&record).unwrap().as_str(),
            "https://www.rfc-editor.org/rfc/rfc9110.html"
        );
        let unrelated = Url::parse("https://example.com/info/rfc9110/").unwrap();
        assert_eq!(document_url(&unrelated), None);
    }
}
