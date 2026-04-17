//! Reads `history.jsonl` and `errors.jsonl` from the install dir and produces
//! UI-ready summaries. Pure functions — no Tauri, no side effects beyond file
//! reads.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

pub const HISTORY_FILE: &str = "history.jsonl";
pub const ERRORS_FILE: &str = "errors.jsonl";

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryRow {
    pub ts: i64,
    pub tool: String,
    #[serde(default)]
    pub args: serde_json::Value,
    pub out_chars: usize,
    pub tokens_est: usize,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ErrorRow {
    pub ts: i64,
    pub tool: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DayBucket {
    /// YYYY-MM-DD in the user's local timezone.
    pub date: String,
    pub calls: usize,
    pub tokens: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsSummary {
    pub today_calls: usize,
    pub today_tokens: usize,
    pub today_errors: usize,
    pub last_call_ts: Option<i64>,
    /// Seven entries, oldest first, covering the last 7 calendar days
    /// including today. Missing days get zero counts.
    pub seven_day_tokens: Vec<DayBucket>,
    pub recent_errors: Vec<ErrorRow>,
}

pub fn read_history(dir: &Path) -> Vec<HistoryRow> {
    read_jsonl(dir.join(HISTORY_FILE).as_path())
}

pub fn read_errors(dir: &Path) -> Vec<ErrorRow> {
    read_jsonl(dir.join(ERRORS_FILE).as_path())
}

fn read_jsonl<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    let Ok(contents) = std::fs::read_to_string(path) else { return Vec::new() };
    contents
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str::<T>(l).ok())
        .collect()
}

/// Build a 7-day UI summary. `now_ts` is passed in so tests are deterministic.
pub fn summarize(history: &[HistoryRow], errors: &[ErrorRow], now_ts: i64) -> StatsSummary {
    use chrono::{Local, TimeZone, Utc};

    let now = Utc.timestamp_opt(now_ts, 0).single().unwrap_or_else(Utc::now);
    let today_local = now.with_timezone(&Local).date_naive();

    let mut today_calls = 0usize;
    let mut today_tokens = 0usize;
    let mut last_call_ts: Option<i64> = None;
    let mut daily: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for row in history {
        let ts = Utc.timestamp_opt(row.ts, 0).single().unwrap_or_else(Utc::now);
        let local_date = ts.with_timezone(&Local).date_naive();
        let key = local_date.format("%Y-%m-%d").to_string();
        let entry = daily.entry(key).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += row.tokens_est;
        if local_date == today_local {
            today_calls += 1;
            today_tokens += row.tokens_est;
        }
        last_call_ts = Some(last_call_ts.map_or(row.ts, |cur| cur.max(row.ts)));
    }

    let today_errors = errors.iter().filter(|e| {
        let ts = Utc.timestamp_opt(e.ts, 0).single().unwrap_or_else(Utc::now);
        ts.with_timezone(&Local).date_naive() == today_local
    }).count();

    let mut seven_day_tokens = Vec::with_capacity(7);
    for i in (0..7).rev() {
        let d = today_local - chrono::Duration::days(i);
        let key = d.format("%Y-%m-%d").to_string();
        let (calls, tokens) = daily.get(&key).copied().unwrap_or((0, 0));
        seven_day_tokens.push(DayBucket { date: key, calls, tokens });
    }

    let mut recent_errors = errors.to_vec();
    recent_errors.sort_by(|a, b| b.ts.cmp(&a.ts));
    recent_errors.truncate(20);

    StatsSummary {
        today_calls, today_tokens, today_errors,
        last_call_ts,
        seven_day_tokens,
        recent_errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ts: i64, tool: &str, tokens: usize, status: &str) -> HistoryRow {
        HistoryRow {
            ts, tool: tool.into(), args: serde_json::json!({}),
            out_chars: tokens * 4, tokens_est: tokens, status: status.into(),
        }
    }

    #[test]
    fn empty_inputs_produce_seven_zero_days() {
        let s = summarize(&[], &[], 1_700_000_000);
        assert_eq!(s.today_calls, 0);
        assert_eq!(s.today_tokens, 0);
        assert_eq!(s.today_errors, 0);
        assert_eq!(s.seven_day_tokens.len(), 7);
        assert!(s.seven_day_tokens.iter().all(|d| d.tokens == 0));
    }

    #[test]
    fn today_counters_aggregate_correctly() {
        // Two calls on the same day.
        let now = 1_700_000_000;
        let history = vec![
            row(now - 10, "get_page", 100, "ok"),
            row(now - 20, "get_page", 200, "ok"),
        ];
        let s = summarize(&history, &[], now);
        assert_eq!(s.today_calls, 2);
        assert_eq!(s.today_tokens, 300);
    }

    #[test]
    fn errors_counted_for_today_only() {
        let now = 1_700_000_000;
        let day = 86_400i64;
        let errors = vec![
            ErrorRow { ts: now, tool: "get_page".into(), status: "403".into(), message: "today".into() },
            ErrorRow { ts: now - 2 * day, tool: "get_page".into(), status: "403".into(), message: "two days ago".into() },
        ];
        let s = summarize(&[], &errors, now);
        assert_eq!(s.today_errors, 1);
    }

    #[test]
    fn recent_errors_sorted_newest_first_truncated_to_20() {
        let base = 1_700_000_000;
        let errors: Vec<ErrorRow> = (0..30).map(|i| ErrorRow {
            ts: base - i,
            tool: "get_page".into(),
            status: "403".into(),
            message: format!("err{i}"),
        }).collect();
        let s = summarize(&[], &errors, base);
        assert_eq!(s.recent_errors.len(), 20);
        assert_eq!(s.recent_errors[0].message, "err0");
        assert_eq!(s.recent_errors[19].message, "err19");
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(HISTORY_FILE);
        std::fs::write(
            &path,
            b"{\"ts\":1,\"tool\":\"x\",\"args\":{},\"out_chars\":1,\"tokens_est\":1,\"status\":\"ok\"}\nnot json\n{\"ts\":2,\"tool\":\"y\",\"args\":{},\"out_chars\":2,\"tokens_est\":2,\"status\":\"ok\"}\n",
        ).unwrap();
        let rows = read_history(tmp.path());
        assert_eq!(rows.len(), 2);
    }
}
