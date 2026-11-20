//! Shared Cloudflare Browser Run transport and response validation.
//! Constructs endpoints under a fixed origin and disables credential redirects.

use reqwest::Url;
use serde::Deserialize;

pub fn endpoint(account: &str, action: &str) -> Result<Url, String> {
    if account.is_empty() || !account.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err("invalid Cloudflare account identifier".into());
    }
    Url::parse(&format!(
        "https://api.cloudflare.com/client/v4/accounts/{account}/browser-run/{action}"
    ))
    .map_err(|_| "invalid Cloudflare endpoint".into())
}

#[derive(Deserialize)]
struct Envelope<T> {
    success: bool,
    result: Option<T>,
    meta: Option<ContentMeta>,
}

pub fn decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, String> {
    let response: Envelope<T> =
        serde_json::from_slice(bytes).map_err(|_| "invalid Cloudflare response")?;
    if !response.success {
        return Err("Cloudflare rejected the request".into());
    }
    response
        .result
        .ok_or_else(|| "Cloudflare response has no result".into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ContentMeta {
    final_url: Option<String>,
    status: Option<u16>,
}

pub fn content_body(bytes: &[u8]) -> Result<(String, Option<String>), String> {
    let body = String::from_utf8_lossy(bytes);
    if !body.trim_start().starts_with('{') {
        return Ok((body.into_owned(), None));
    }
    let response: Envelope<String> =
        serde_json::from_slice(bytes).map_err(|_| "invalid Cloudflare response")?;
    if !response.success {
        return Err("Cloudflare rejected the request".into());
    }
    let final_url = if let Some(meta) = response.meta {
        if let Some(status) = meta.status.filter(|status| !(200..300).contains(status)) {
            return Err(format!("rendered origin returned HTTP {status}"));
        }
        meta.final_url
            .filter(|url| crate::fetch::http::validate_url(url).is_ok())
    } else {
        None
    };
    Ok((
        response.result.ok_or("Cloudflare response has no result")?,
        final_url,
    ))
}
