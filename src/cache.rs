//! Store completed summaries against a versioned content and backend identity.
//! Exports CacheKey, cache_get, cache_put, and cache_clear.

use crate::{config::dirs_home, llm::Summary};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

const CACHE_REVISION: &str = "summary-v2";
const TTL_SECONDS: u64 = 3600;

pub struct CacheKey(String);

impl CacheKey {
    pub fn new(mode: &str, prompt: &str, content: &str, scope: &str) -> Self {
        let mut hash = Sha256::new();
        for field in [CACHE_REVISION, mode, prompt, content, scope] {
            hash.update((field.len() as u64).to_be_bytes());
            hash.update(field.as_bytes());
        }
        Self(format!("{:x}", hash.finalize()))
    }

    fn path(&self) -> PathBuf {
        cache_dir().join(&self.0)
    }
}

#[derive(Serialize, Deserialize)]
pub struct CacheEntry {
    pub summary: Summary,
    pub timestamp: u64,
}

fn cache_dir() -> PathBuf {
    dirs_home().join(".ai-summary/cache")
}

pub fn cache_get(key: &CacheKey) -> Option<CacheEntry> {
    let contents = fs::read(key.path()).ok()?;
    let entry: CacheEntry = serde_json::from_slice(&contents).ok()?;
    let now = current_timestamp();
    (entry.timestamp <= now
        && now - entry.timestamp < TTL_SECONDS
        && !entry.summary.text.trim().is_empty())
    .then_some(entry)
}

pub fn cache_put(key: &CacheKey, summary: &Summary) {
    let entry = CacheEntry {
        summary: summary.clone(),
        timestamp: current_timestamp(),
    };
    let Ok(contents) = serde_json::to_vec(&entry) else {
        return;
    };
    let Ok(_lock) = crate::storage::lock(&dirs_home().join(".ai-summary/cache.lock")) else {
        return;
    };
    if crate::storage::write_atomic(&key.path(), &contents, 0o600).is_ok() {
        prune_cache();
    }
}

pub fn cache_clear() -> u64 {
    let Ok(_lock) = crate::storage::lock(&dirs_home().join(".ai-summary/cache.lock")) else {
        return 0;
    };
    fs::read_dir(cache_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .filter(|entry| fs::remove_file(entry.path()).is_ok())
        .count() as u64
}

pub fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn prune_cache() {
    const MAX_ENTRIES: usize = 512;
    let Ok(directory) = fs::read_dir(cache_dir()) else {
        return;
    };
    let mut live = Vec::new();
    let now = SystemTime::now();
    for entry in directory.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.len() != 64 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        if now
            .duration_since(modified)
            .is_ok_and(|age| age.as_secs() >= TTL_SECONDS)
        {
            let _ = fs::remove_file(entry.path());
        } else {
            live.push((modified, entry.path()));
        }
    }
    live.sort_unstable();
    let excess = live.len().saturating_sub(MAX_ENTRIES);
    for (_, path) in live.into_iter().take(excess) {
        let _ = fs::remove_file(path);
    }
}
