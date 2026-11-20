//! Download HTTP sources and explicit hosted-renderer responses.
//! Validates status, media type, and byte limits before extraction.

use super::{documents, page, FetchedPage};
use crate::{
    config::Config,
    fetch_utils::{extract_redirect, extracted_text},
    search::UA,
};
use reqwest::{
    blocking::{Client, Response},
    Url,
};
use std::{io::Read, time::Duration};

const TEXT_BYTES: u64 = 2 * 1024 * 1024;
const DOCUMENT_BYTES: u64 = 10_000_000;

pub(super) fn direct(client: &Client, url: &str, max_chars: usize) -> Result<FetchedPage, String> {
    direct_with_focus(client, url, max_chars, None)
}

pub(super) fn direct_with_focus(
    client: &Client,
    url: &str,
    max_chars: usize,
    focus: Option<&str>,
) -> Result<FetchedPage, String> {
    let mut target = validate_url(url)?;
    for hop in 0..=3 {
        let response = download(client, &target)?;
        let effective = response.url().clone();
        if let Some(document) = (hop < 3 && focus.is_some())
            .then(|| super::section::document_url(&effective))
            .flatten()
        {
            target = document;
            continue;
        }
        let media = content_type(&response);
        if let Some(format) = documents::Format::detect(&media, effective.path()) {
            let bytes = read_body(response, DOCUMENT_BYTES)?;
            let text = format.extract(&bytes)?;
            return focused_page(
                url,
                &effective,
                format.name(),
                format.media_type(),
                text,
                (max_chars, focus),
            );
        }
        if !is_text_response(&media, effective.path()) {
            return Err("unsupported response type".into());
        }
        let (bytes, body_clipped) = read_text_body(response)?;
        let body = String::from_utf8_lossy(&bytes);
        let html =
            media.contains("html") || (media.is_empty() && body.trim_start().starts_with('<'));
        if html && hop < 3 && body.len() < 1000 {
            if let Some(redirect) = extract_redirect(&body) {
                target = validate_url(&redirect)?;
                continue;
            }
        }
        return assemble_text(
            url,
            effective,
            &media,
            html,
            &body,
            body_clipped,
            (max_chars, focus),
        );
    }
    Err("redirect limit exceeded".into())
}

fn download(client: &Client, target: &Url) -> Result<Response, String> {
    client
        .get(target.clone())
        .header("User-Agent", UA)
        .timeout(Duration::from_secs(8))
        .send()
        .map_err(connection_error)
}

fn content_type(response: &Response) -> String {
    response
        .headers()
        .get("content-type")
        .and_then(|header| header.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn assemble_text(
    requested: &str,
    mut effective: Url,
    media: &str,
    html: bool,
    body: &str,
    body_clipped: bool,
    (max_chars, focus): (usize, Option<&str>),
) -> Result<FetchedPage, String> {
    let fragment = validate_url(requested)?.fragment().map(str::to_owned);
    if effective.fragment().is_none() {
        effective.set_fragment(fragment.as_deref());
    }
    let (kind, text) = if html {
        if let Some(anchor) = effective.fragment().filter(|anchor| !anchor.is_empty()) {
            if let Some(text) = super::section::extract(body, anchor) {
                ("html_section", text)
            } else {
                eprintln!("[ai-summary] requested section was not found in the fetched HTML");
                extracted_text(effective.as_str(), body)
            }
        } else {
            extracted_text(effective.as_str(), body)
        }
    } else {
        ("text", body.trim().to_owned())
    };
    let focus = focus.filter(|_| kind != "html_section");
    let mut page = focused_page(requested, &effective, kind, media, text, (max_chars, focus))?;
    page.truncated |= body_clipped;
    Ok(page)
}

fn focused_page(
    requested: &str,
    effective: &Url,
    kind: &'static str,
    media: &str,
    text: String,
    (max_chars, focus): (usize, Option<&str>),
) -> Result<FetchedPage, String> {
    let offset = focus.map_or(0, |query| super::passage::start(&text, query, max_chars));
    let text = if offset == 0 {
        text
    } else {
        text[offset..].to_owned()
    };
    let mut page = page(requested, effective.as_str(), kind, media, text, max_chars)?;
    page.truncated |= offset > 0;
    Ok(page)
}

fn read_text_body(response: Response) -> Result<(Vec<u8>, bool), String> {
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    response
        .take(TEXT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read response")?;
    let clipped = bytes.len() as u64 > TEXT_BYTES;
    if clipped {
        bytes.truncate(TEXT_BYTES as usize);
    }
    Ok((bytes, clipped))
}

pub(super) fn is_text_response(media: &str, url: &str) -> bool {
    media.is_empty()
        || media.starts_with("text/")
        || media.contains("application/xhtml+xml")
        || url.ends_with(".html")
        || url.ends_with(".txt")
}

pub(super) fn cloudflare(cfg: &Config, url: &str, max_chars: usize) -> Result<FetchedPage, String> {
    validate_url(url)?;
    let endpoint = crate::cloudflare::endpoint(&cfg.cf_account_id, "content")?;
    let client = crate::credential_client()?;
    let response = client
        .post(endpoint)
        .bearer_auth(&cfg.cf_api_token)
        .json(&serde_json::json!({
            "url": url, "rejectResourceTypes": ["image", "media", "font", "stylesheet"]
        }))
        .timeout(Duration::from_secs(30))
        .send()
        .map_err(connection_error)?;
    let bytes = read_body(response, TEXT_BYTES)?;
    let (html, final_url) = crate::cloudflare::content_body(&bytes)?;
    let final_url = final_url.as_deref().unwrap_or(url);
    let (_, text) = extracted_text(final_url, &html);
    page(url, final_url, "cloudflare", "text/html", text, max_chars)
}

pub(super) fn jina(url: &str, max_chars: usize, key: &str) -> Result<FetchedPage, String> {
    let target = validate_url(url)?;
    if key.is_empty()
        || target.host_str().is_some_and(|host| {
            host == "localhost"
                || host.ends_with(".localhost")
                || host.parse::<std::net::IpAddr>().is_ok()
                || host.starts_with('[')
        })
    {
        return Err("hosted reader not configured for this source".into());
    }
    let reader = format!("https://r.jina.ai/{target}");
    let response = crate::credential_client()?
        .get(reader)
        .header("Accept", "text/plain")
        .bearer_auth(key)
        .timeout(Duration::from_secs(15))
        .send()
        .map_err(connection_error)?;
    let bytes = read_body(response, TEXT_BYTES)?;
    let text = String::from_utf8_lossy(&bytes);
    if text.contains("AuthenticationRequiredError") {
        return Err("hosted reader authentication failed".into());
    }
    page(url, url, "jina", "text/plain", text.into_owned(), max_chars)
}

pub(crate) fn validate_url(url: &str) -> Result<Url, String> {
    let parsed = Url::parse(url).map_err(|_| "invalid source URL")?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("source must be an HTTP(S) URL without embedded credentials".into());
    }
    Ok(parsed)
}

pub(crate) fn read_body(response: Response, limit: u64) -> Result<Vec<u8>, String> {
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    let mut bytes = Vec::new();
    response
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read response")?;
    if bytes.len() as u64 > limit {
        return Err(format!("response exceeds {limit} bytes"));
    }
    Ok(bytes)
}

fn connection_error(error: reqwest::Error) -> String {
    format!("request failed: {}", error.without_url())
}
