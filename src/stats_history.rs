// History handling for stats persistence and aggregation helpers.
// Exports: StatsStore, HistoryEntry, PeriodMetrics, stats_path(), load_stats(), save_stats().
// Deps: crate::config::dirs_home, serde::{Deserialize, Serialize}, serde_json, std::fs.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::dirs_home;

#[derive(Serialize, Deserialize, Default)]
pub struct StatsStore {
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

pub fn stats_path() -> PathBuf {
    dirs_home().join(".ai-summary/stats.json")
}

pub fn load_stats() -> StatsStore {
    stats_path()
        .exists()
        .then(|| fs::read_to_string(stats_path()).ok())
        .flatten()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

pub fn save_stats(stats: &StatsStore) {
    let path = stats_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(stats) {
        let _ = fs::write(&path, json);
    }
}
