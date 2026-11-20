// History handling for stats persistence and aggregation helpers.
// Exports: StatsStore, HistoryEntry, PeriodMetrics, load_stats(), update_stats(), reset_stats().
// Deps: crate::config::dirs_home, serde::{Deserialize, Serialize}, serde_json, std::fs.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{fs, io::ErrorKind};

use crate::config::dirs_home;

#[derive(Serialize, Deserialize, Default)]
pub struct StatsStore {
    #[serde(default)]
    pub schema_version: u32,
    pub total_searches: u64,
    pub total_pages_fetched: u64,
    pub total_raw_chars: u64,
    pub total_summary_chars: u64,
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub estimated_claude_tokens_saved: u64,
    pub total_duration_secs: f64,
    pub history: Vec<HistoryEntry>,
}

#[derive(Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub query: String,
    pub mode: String,
    pub sources: u32,
    pub raw_chars: u64,
    pub summary_chars: u64,
    pub llm_tokens: u64,
    pub estimated_saved: u64,
    pub duration_secs: f64,
}

#[derive(Default, Clone, Copy)]
pub struct PeriodMetrics {
    pub queries: u64,
    pub pages: u64,
    pub saved: u64,
    pub raw_chars: u64,
    pub summary_chars: u64,
}

impl PeriodMetrics {
    pub fn add_record(&mut self, record: &HistoryEntry, track_queries: bool) {
        if track_queries {
            self.queries += 1;
            self.pages += record.sources as u64;
        }
        self.saved += record.estimated_saved;
        self.raw_chars += record.raw_chars;
        self.summary_chars += record.summary_chars;
    }

    pub fn compression_pct(&self) -> f64 {
        if self.raw_chars == 0 {
            0.0
        } else {
            (1.0 - self.summary_chars as f64 / self.raw_chars as f64) * 100.0
        }
    }
}

const SCHEMA_VERSION: u32 = 1;
const RETAINED_HISTORY: usize = 500;

fn stats_path() -> PathBuf {
    dirs_home().join(".ai-summary/stats.json")
}

pub fn load_stats() -> Result<StatsStore, String> {
    let data = match fs::read(stats_path()) {
        Ok(data) => data,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(StatsStore {
                schema_version: SCHEMA_VERSION,
                ..StatsStore::default()
            });
        }
        Err(error) => return Err(format!("cannot read statistics: {error}")),
    };
    let mut stats: StatsStore = serde_json::from_slice(&data)
        .map_err(|_| "statistics file is invalid; existing data was kept")?;
    if stats.schema_version > SCHEMA_VERSION {
        return Err("unsupported statistics schema; existing data was kept".into());
    }
    stats.schema_version = SCHEMA_VERSION;
    Ok(stats)
}

pub fn update_stats(update: impl FnOnce(&mut StatsStore)) {
    if let Err(error) = update_locked(update) {
        eprintln!("[ai-summary] {error}");
    }
}

fn update_locked(update: impl FnOnce(&mut StatsStore)) -> Result<(), String> {
    let path = stats_path();
    let _lock = crate::storage::lock(&path.with_extension("lock"))
        .map_err(|error| format!("cannot lock statistics: {error}"))?;
    let mut stats = load_stats()?;
    update(&mut stats);
    if stats.history.len() > RETAINED_HISTORY {
        stats
            .history
            .drain(..stats.history.len() - RETAINED_HISTORY);
    }
    let bytes = serde_json::to_vec_pretty(&stats).map_err(|_| "cannot encode statistics")?;
    crate::storage::write_atomic(&path, &bytes, 0o600)
        .map_err(|error| format!("cannot save statistics: {error}"))
}

pub fn reset_stats() -> Result<(), String> {
    let path = stats_path();
    let _lock = crate::storage::lock(&path.with_extension("lock"))
        .map_err(|error| format!("cannot lock statistics: {error}"))?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot reset statistics: {error}")),
    }
}
