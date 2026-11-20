//! Render retained statistics history and efficiency.
//! Uses the existing statistics data model and text format.

use super::{fmtn, trunc, HistoryEntry as SearchRecord, PeriodMetrics, Reverse, StatsStore};

pub(super) fn print_history(stats: &StatsStore, all_time: PeriodMetrics) {
    print_top(stats);
    print_efficiency(all_time);
    print_recent(stats);
}

fn print_top(stats: &StatsStore) {
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
}

fn print_efficiency(all_time: PeriodMetrics) {
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
}

fn print_recent(stats: &StatsStore) {
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
