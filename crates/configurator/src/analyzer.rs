//! Heuristic rules over a parsed history slice. Pure functions; no I/O.

use crate::stats::HistoryRow;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tip {
    pub id: &'static str,
    pub title: String,
    pub detail: String,
}

const SEVEN_DAYS_SECS: i64 = 7 * 86_400;
const OVERSIZED_TOKENS: usize = 20_000;
const REPEAT_FETCH_THRESHOLD: usize = 5;
const FREQUENT_403_THRESHOLD: usize = 3;
const HIGH_ERROR_RATE: f64 = 0.10;

pub fn analyze(history: &[HistoryRow], now_ts: i64) -> Vec<Tip> {
    let cutoff = now_ts - SEVEN_DAYS_SECS;
    let recent: Vec<&HistoryRow> = history.iter().filter(|r| r.ts >= cutoff).collect();
    let mut tips = Vec::new();

    if let Some(t) = repeated_page_fetch(&recent) { tips.push(t); }
    if let Some(t) = oversized_output(&recent) { tips.push(t); }
    if let Some(t) = high_error_rate(&recent) { tips.push(t); }
    if let Some(t) = frequent_403s(&recent) { tips.push(t); }
    tips
}

fn repeated_page_fetch(rows: &[&HistoryRow]) -> Option<Tip> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in rows {
        if r.tool != "get_page" { continue; }
        if let Some(id) = r.args.get("page_id").and_then(|v| v.as_str()) {
            *counts.entry(id.to_string()).or_insert(0) += 1;
        }
    }
    let (page_id, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    if n < REPEAT_FETCH_THRESHOLD { return None; }
    Some(Tip {
        id: "repeated_page_fetch",
        title: format!("You've fetched page {page_id} {n} times this week."),
        detail: "Pin it in your Claude prompt or use `include_body=false` for previews.".into(),
    })
}

fn oversized_output(rows: &[&HistoryRow]) -> Option<Tip> {
    let n = rows.iter().filter(|r| r.tokens_est > OVERSIZED_TOKENS).count();
    if n == 0 { return None; }
    Some(Tip {
        id: "oversized_output",
        title: format!("{n} tool call(s) returned over 20k tokens."),
        detail: "Narrow your CQL (e.g. add `space=...`) or call `get_page` with `include_body=false` first.".into(),
    })
}

fn high_error_rate(rows: &[&HistoryRow]) -> Option<Tip> {
    if rows.is_empty() { return None; }
    let errors = rows.iter().filter(|r| r.status != "ok").count();
    let rate = errors as f64 / rows.len() as f64;
    if rate < HIGH_ERROR_RATE { return None; }
    Some(Tip {
        id: "high_error_rate",
        title: format!("{}% of calls failed this week.", (rate * 100.0).round() as u32),
        detail: "Check the Recent errors list below — the same endpoint may keep failing.".into(),
    })
}

fn frequent_403s(rows: &[&HistoryRow]) -> Option<Tip> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for r in rows {
        if r.status != "403" { continue; }
        if let Some(id) = r.args.get("page_id").and_then(|v| v.as_str()) {
            *counts.entry(id.to_string()).or_insert(0) += 1;
        }
    }
    let (page_id, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    if n < FREQUENT_403_THRESHOLD { return None; }
    Some(Tip {
        id: "frequent_403s",
        title: format!("Page {page_id} returned 403 {n} times."),
        detail: "That page is restricted — ask your Confluence admin for access or use a different page.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn row(ts: i64, tool: &str, status: &str, tokens: usize, args: serde_json::Value) -> HistoryRow {
        HistoryRow { ts, tool: tool.into(), args, out_chars: tokens * 4, tokens_est: tokens, status: status.into() }
    }

    #[test]
    fn no_tips_when_empty() {
        assert!(analyze(&[], 1_700_000_000).is_empty());
    }

    #[test]
    fn repeated_page_fetch_fires_at_threshold() {
        let now = 1_700_000_000;
        let rows: Vec<HistoryRow> = (0..5).map(|i| row(now - i, "get_page", "ok", 100, json!({"page_id":"42"}))).collect();
        let tips = analyze(&rows, now);
        assert!(tips.iter().any(|t| t.id == "repeated_page_fetch"));
    }

    #[test]
    fn repeated_page_fetch_does_not_fire_below_threshold() {
        let now = 1_700_000_000;
        let rows: Vec<HistoryRow> = (0..4).map(|i| row(now - i, "get_page", "ok", 100, json!({"page_id":"42"}))).collect();
        let tips = analyze(&rows, now);
        assert!(!tips.iter().any(|t| t.id == "repeated_page_fetch"));
    }

    #[test]
    fn oversized_output_fires_on_large_call() {
        let now = 1_700_000_000;
        let rows = vec![row(now, "search_confluence", "ok", 25_000, json!({}))];
        let tips = analyze(&rows, now);
        assert!(tips.iter().any(|t| t.id == "oversized_output"));
    }

    #[test]
    fn high_error_rate_fires_above_ten_percent() {
        let now = 1_700_000_000;
        // 2 errors out of 10 calls = 20%.
        let mut rows = Vec::new();
        for i in 0..8 { rows.push(row(now - i, "get_page", "ok", 50, json!({"page_id":"1"}))); }
        for i in 0..2 { rows.push(row(now - 20 - i, "get_page", "500", 50, json!({"page_id":"1"}))); }
        let tips = analyze(&rows, now);
        assert!(tips.iter().any(|t| t.id == "high_error_rate"));
    }

    #[test]
    fn frequent_403s_fires_on_same_page() {
        let now = 1_700_000_000;
        let rows: Vec<HistoryRow> = (0..3).map(|i| row(now - i, "get_page", "403", 50, json!({"page_id":"99"}))).collect();
        let tips = analyze(&rows, now);
        assert!(tips.iter().any(|t| t.id == "frequent_403s"));
    }

    #[test]
    fn old_entries_ignored() {
        let now = 1_700_000_000;
        let eight_days = 8 * 86_400;
        // Five fetches of the same page, but 8 days ago — should NOT fire.
        let rows: Vec<HistoryRow> = (0..5).map(|i| row(now - eight_days - i, "get_page", "ok", 100, json!({"page_id":"42"}))).collect();
        let tips = analyze(&rows, now);
        assert!(!tips.iter().any(|t| t.id == "repeated_page_fetch"));
    }
}
