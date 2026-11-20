//! Submit a Browser Run crawl, poll for completion, and paginate records.
//! Uses one wall-clock deadline across the request sequence.

use crate::{cloudflare, fetch::http::read_body};
use reqwest::{
    blocking::{Client, RequestBuilder},
    Url,
};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

const MAX_BODY_BYTES: u64 = 12_000_000;
const POLL_DELAY: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Deserialize)]
pub struct Record {
    pub url: String,
    pub status: String,
    #[serde(default)]
    pub markdown: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Queued,
    Pending,
    Completed,
    Errored,
    CancelledDueToLimits,
    CancelledDueToTimeout,
    CancelledByUser,
}

#[derive(Deserialize)]
struct Job {
    status: Status,
    #[serde(default)]
    records: Vec<Record>,
    cursor: Option<Cursor>,
}

#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum Cursor {
    Number(u64),
    Text(String),
}

impl Cursor {
    fn value(self) -> String {
        match self {
            Self::Number(value) => value.to_string(),
            Self::Text(value) => value,
        }
    }
}

pub struct CrawlApi<'a> {
    pub client: &'a Client,
    pub endpoint: Url,
    pub token: &'a str,
    pub deadline: Instant,
}

impl CrawlApi<'_> {
    pub fn run(&self, url: &str, limit: u32, depth: u32) -> Result<(Status, Vec<Record>), String> {
        let request = self.client.post(self.endpoint.clone()).json(&json!({
            "url": url, "limit": limit, "depth": depth, "formats": ["markdown"], "render": true,
            "crawlPurposes": ["search", "ai-input"],
            "rejectResourceTypes": ["image", "media", "font", "stylesheet"],
        }));
        let id: String = self.request(request)?;
        if id.is_empty()
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("invalid Cloudflare crawl job identifier".into());
        }
        let mut job_url = self.endpoint.clone();
        job_url
            .path_segments_mut()
            .map_err(|_| "invalid Cloudflare job URL")?
            .push(&id);
        let status = self.wait(&job_url)?;
        let records = self.records(&job_url, limit)?;
        Ok((status, records))
    }

    fn wait(&self, job_url: &Url) -> Result<Status, String> {
        loop {
            let request = self.client.get(job_url.clone()).query(&[("limit", 1)]);
            let job: Job = self.request(request)?;
            match job.status {
                Status::Running | Status::Queued | Status::Pending => {
                    std::thread::sleep(self.time_left()?.min(POLL_DELAY));
                }
                terminal => return Ok(terminal),
            }
        }
    }

    fn records(&self, job_url: &Url, limit: u32) -> Result<Vec<Record>, String> {
        let mut records = Vec::new();
        let mut seen = HashSet::new();
        let mut cursor: Option<String> = None;
        for _ in 0..limit {
            let remaining = (limit as usize).saturating_sub(records.len());
            let mut request = self
                .client
                .get(job_url.clone())
                .query(&[("limit", remaining)]);
            if let Some(cursor) = &cursor {
                request = request.query(&[("cursor", cursor)]);
            }
            let mut job: Job = self.request(request)?;
            records.extend(job.records.drain(..).take(remaining));
            cursor = match job.cursor {
                Some(cursor) if records.len() < limit as usize => {
                    let cursor = cursor.value();
                    if cursor.is_empty() || !seen.insert(cursor.clone()) {
                        return Err("Cloudflare repeated a crawl cursor".into());
                    }
                    Some(cursor)
                }
                _ => return Ok(records),
            };
        }
        Err("Cloudflare crawl pagination exceeded the page budget".into())
    }

    fn request<T: serde::de::DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> Result<T, String> {
        let response = request
            .bearer_auth(self.token)
            .timeout(self.time_left()?.min(REQUEST_TIMEOUT))
            .send()
            .map_err(|error| format!("Cloudflare request failed: {}", error.without_url()))?;
        cloudflare::decode(&read_body(response, MAX_BODY_BYTES)?)
    }

    fn time_left(&self) -> Result<Duration, String> {
        self.deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| "crawl deadline exceeded".into())
    }
}
