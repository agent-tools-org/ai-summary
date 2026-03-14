// Token-saving stats API and reporting helpers.
// Exports: StatsStore, HistoryEntry, stats_path(), load_stats(), save_stats(), record_search(), record_compress(), get_stats(), print_stats(), print_stats_json().
// Deps: crate::stats_history::{HistoryEntry, PeriodMetrics}, crate::types::Usage, std::cmp::Reverse, std::collections::HashMap.
use std::cmp::Reverse;
use std::collections::HashMap;
use crate::stats_history::PeriodMetrics;
pub use crate::stats_history::{HistoryEntry, StatsStore, load_stats, save_stats, stats_path};
use crate::types::Usage;
pub type Stats = StatsStore;
pub type SearchRecord = HistoryEntry;
pub fn get_stats() -> Stats {
    load_stats()
}
pub fn record_search(
    query: &str,
    mode: &str,
    sources: u32,
    raw_chars: u64,
    summary_chars: u64,
    usage: Option<&Usage>,
    duration_secs: f64,
) {
    let mut stats: Stats = get_stats();
    let llm_tokens = usage
        .map(|usage| (usage.prompt_tokens + usage.completion_tokens) as u64)
        .unwrap_or(0);
    let saved = (raw_chars / 4).saturating_sub(summary_chars / 4);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    stats.total_searches += 1;
    stats.total_pages_fetched += sources as u64;
    stats.total_raw_chars += raw_chars;
    stats.total_summary_chars += summary_chars;
    stats.total_prompt_tokens += usage.map(|usage| usage.prompt_tokens as u64).unwrap_or(0);
    stats.total_completion_tokens += usage
        .map(|usage| usage.completion_tokens as u64)
        .unwrap_or(0);
    stats.estimated_claude_tokens_saved += saved;
    stats.total_duration_secs += duration_secs;
    stats.history.push(SearchRecord {
        timestamp: now,
        query: query.to_string(),
        mode: mode.to_string(),
        sources,
        raw_chars,
        summary_chars,
        llm_tokens,
        estimated_saved: saved,
        duration_secs,
    });
    if stats.history.len() > 500 {
        stats.history.drain(..stats.history.len() - 500);
    }
    save_stats(&stats);
}

pub fn record_compress(source: &str, raw_chars: u64, compressed_chars: u64) {
    let mut stats: Stats = get_stats();
    let saved = (raw_chars / 4).saturating_sub(compressed_chars / 4);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    stats.total_raw_chars += raw_chars;
    stats.total_summary_chars += compressed_chars;
    stats.estimated_claude_tokens_saved += saved;
    stats.history.push(SearchRecord {
        timestamp: now,
        query: "[compress]".to_string(),
        mode: source.to_string(),
        sources: 0,
        raw_chars,
        summary_chars: compressed_chars,
        llm_tokens: 0,
        estimated_saved: saved,
        duration_secs: 0.0,
    });
    if stats.history.len() > 500 {
        stats.history.drain(..stats.history.len() - 500);
    }
    save_stats(&stats);
}

pub fn print_stats() {
    let stats = get_stats();
    if stats.total_searches == 0 && stats.history.is_empty() {
        println!("No data yet. Run a search first!");
        return;
    }

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    const DAY: u64 = 86_400;
    const WEEK: u64 = DAY * 7;
    const MONTH: u64 = DAY * 30;

    let mut periods = [PeriodMetrics::default(); 3]; // today, 7d, 30d
    let mut hook_count: u64 = 0;
    let mut manual_count: u64 = 0;
    let mut mode_map: HashMap<String, (u64, u64, u64, f64, u64)> = HashMap::new();

    for record in &stats.history {
        if record.mode.starts_with("hook-") {
            hook_count += 1;
        } else {
            manual_count += 1;
        }
        let is_query = record.query != "[compress]";
        let age = now.saturating_sub(record.timestamp);
        let cutoffs = [DAY, WEEK, MONTH];
        for (i, &cutoff) in cutoffs.iter().enumerate() {
            if age <= cutoff {
                periods[i].add_record(record, is_query);
            }
        }
        let entry = mode_map.entry(record.mode.clone()).or_default();
        entry.0 += 1;
        entry.1 += record.estimated_saved;
        entry.2 += record.raw_chars;
        entry.3 += record.duration_secs;
        entry.4 += record.summary_chars;
    }

    let all_time = PeriodMetrics {
        queries: stats.total_searches,
        pages: stats.total_pages_fetched,
        saved: stats.estimated_claude_tokens_saved,
        raw_chars: stats.total_raw_chars,
        summary_chars: stats.total_summary_chars,
    };
    let pm = [periods[0], periods[1], periods[2], all_time];

    println!("ai-summary Token Savings");
    println!("{}", "═".repeat(60));
    println!();
    println!(
        "{:<17} {:>8} {:>8} {:>8} {:>10}",
        "Metric", "Today", "7 days", "30 days", "All Time"
    );
    println!("{}", "─".repeat(60));
    print_row("Queries", &pm, |m| m.queries.to_string());
    print_row("Pages fetched", &pm, |m| m.pages.to_string());
    print_row("Tokens saved", &pm, |m| fmtn(m.saved));
    print_row("Cost saved", &pm, |m| {
        format!("${:.2}", m.saved as f64 * 3.0 / 1_000_000.0)
    });
    print_row("Compression", &pm, |m| {
        format!("{:.0}%", m.compression_pct().clamp(0.0, 100.0))
    });
    println!("{}", "─".repeat(60));

    // ROI — cost-based: LLM tokens are ~free, Claude tokens are $3-15/M
    let llm_tokens = stats.total_prompt_tokens + stats.total_completion_tokens;
    let llm_cost = llm_tokens as f64 * 0.1 / 1_000_000.0; // ~$0.10/M (most backends are free)
    let claude_saved = stats.estimated_claude_tokens_saved as f64 * 3.0 / 1_000_000.0; // $3/M input
    let roi = if llm_cost > 0.01 {
        claude_saved / llm_cost
    } else {
        claude_saved / 0.01
    };
    println!();
    println!(
        "ROI: ${:.3} LLM cost -> ${:.2} Claude cost saved ({:.0}x return)",
        llm_cost, claude_saved, roi
    );

    // By Mode
    let mut modes: Vec<_> = mode_map.into_iter().collect();
    modes.sort_by(|a, b| b.1 .1.cmp(&a.1 .1));
    let max_saved = modes.first().map(|m| m.1 .1).unwrap_or(1).max(1);

    println!();
    println!("By Mode (hooks: {}, manual: {})", hook_count, manual_count);
    println!("{}", "─".repeat(68));
    println!("  #  Mode            Count    Saved   Avg%     Time  Impact");
    println!("{}", "─".repeat(68));
    for (i, (mode, (count, saved, raw, time, summary))) in modes.iter().enumerate() {
        let avg_pct = if *raw > 0 {
            (1.0 - *summary as f64 / *raw as f64) * 100.0
        } else {
            0.0
        };
        let avg_time = time / *count as f64;
        let bar_len = (*saved as f64 / max_saved as f64 * 10.0) as usize;
        let bar = format!("{}{}", "█".repeat(bar_len), "░".repeat(10 - bar_len));
        println!(
            " {:>2}.  {:<13} {:>5}  {:>7}  {:>5.1}%  {:>6.1}s  {}",
            i + 1,
            mode,
            count,
            fmtn(*saved),
            avg_pct,
            avg_time,
            bar
        );
    }
    println!("{}", "─".repeat(68));

    // Top 5 savings
    let mut top: Vec<&SearchRecord> = stats
        .history
        .iter()
        .filter(|r| r.query != "[compress]")
        .collect();
    top.sort_by_key(|r| Reverse(r.estimated_saved));
    top.truncate(5);
    if !top.is_empty() {
        println!();
        println!("Top 5 Savings");
        println!("{}", "─".repeat(68));
        for (i, record) in top.iter().enumerate() {
            println!(
                " {:>2}. [{:<13}] {:38} {:>7}",
                i + 1,
                record.mode,
                trunc(&record.query, 38),
                fmtn(record.estimated_saved)
            );
        }
        println!("{}", "─".repeat(68));
    }

    // Efficiency meter
    let pct = all_time.compression_pct().clamp(0.0, 100.0);
    let filled = (pct / 100.0 * 24.0) as usize;
    let empty = 24 - filled;
    println!();
    println!(
        "Efficiency meter:  {}{} {:.1}%",
        "█".repeat(filled),
        "░".repeat(empty),
        pct
    );

    // Recent
    let recent: Vec<&SearchRecord> = stats.history.iter().rev().take(5).collect();
    if !recent.is_empty() {
        println!();
        println!("Recent");
        println!("{}", "─".repeat(68));
        for record in recent {
            let saved_str = if record.estimated_saved >= 1000 {
                format!("~{:.1}K", record.estimated_saved as f64 / 1000.0)
            } else {
                format!("~{}", record.estimated_saved)
            };
            println!(
                "  [{:>13}] {:38} {:>2} src  {:>6} saved  {:.1}s",
                record.mode,
                trunc(&record.query, 36),
                record.sources,
                saved_str,
                record.duration_secs
            );
        }
        println!("{}", "─".repeat(68));
    }
}
pub fn print_stats_json() {
    let stats = get_stats();
    if let Ok(json) = serde_json::to_string_pretty(&stats) {
        println!("{}", json);
    } else {
        println!("{{}}");
    }
}
fn print_row<F: Fn(&PeriodMetrics) -> String>(label: &str, pm: &[PeriodMetrics; 4], f: F) {
    println!(
        "{:<17} {:>8} {:>8} {:>8} {:>10}",
        label,
        f(&pm[0]),
        f(&pm[1]),
        f(&pm[2]),
        f(&pm[3])
    );
}
pub fn fmtn(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}
pub fn trunc(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}
