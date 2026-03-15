// Summary cache for ai-summary CLI.
// Exports: CacheEntry, DEFAULT_CACHE_TTL, cache_dir, cache_key, cache_get, cache_put, cache_clear.
// Deps: serde, sha2, std (fs, path, time).

#![allow(dead_code)]

use crate::config::dirs_home;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_CACHE_TTL: u64 = 3600;

#[derive(Serialize, Deserialize)]
pub struct CacheEntry {
    pub url: String,
    pub prompt: String,
    pub summary: String,
    pub raw_chars: u64,
    pub summary_chars: u64,
    pub timestamp: u64,
    pub ttl: u64,
}

pub fn cache_dir() -> PathBuf {
    let dir = dirs_home().join(".ai-summary/cache");
    let _ = fs::create_dir_all(&dir);
    dir
}

pub fn cache_key(url: &str, prompt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(url.as_bytes());
    hasher.update(b"|");
    hasher.update(prompt.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn cache_get(url: &str, prompt: &str) -> Option<CacheEntry> {
    let key = cache_key(url, prompt);
    let path = cache_dir().join(&key);
    let content = fs::read_to_string(&path).ok()?;
    let entry: CacheEntry = serde_json::from_str(&content).ok()?;
    let now = current_timestamp();
    if is_stale(&entry, now) {
        let _ = fs::remove_file(&path);
        return None;
    }
    Some(entry)
}

pub fn cache_put(
    url: &str,
    prompt: &str,
    summary: &str,
    raw_chars: u64,
    summary_chars: u64,
) {
    let entry = CacheEntry {
        url: url.to_string(),
        prompt: prompt.to_string(),
        summary: summary.to_string(),
        raw_chars,
        summary_chars,
        timestamp: current_timestamp(),
        ttl: DEFAULT_CACHE_TTL,
    };
    let key = cache_key(url, prompt);
    let path = cache_dir().join(&key);
    if let Ok(content) = serde_json::to_string(&entry) {
        let _ = fs::write(&path, content);
    }
}

pub fn cache_clear() -> u64 {
    let dir = cache_dir();
    let mut removed = 0;
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

fn is_stale(entry: &CacheEntry, now: u64) -> bool {
    now >= entry.timestamp.saturating_add(entry.ttl)
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| dur.as_secs())
        .unwrap_or_default()
}
