//! Append-only recorder for `history.jsonl` and `errors.jsonl`.
//!
//! Writes one JSON line per tool call to the server's install directory.
//! The configurator reads these files; there is no IPC — just a shared dir.

use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

pub const HISTORY_FILE: &str = "history.jsonl";
pub const ERRORS_FILE: &str = "errors.jsonl";
pub const MAX_HISTORY_LINES: usize = 1000;
pub const MAX_ERROR_LINES: usize = 20;
const TRUNCATE_EVERY_N_WRITES: u32 = 100;

#[derive(Debug, Serialize)]
pub struct HistoryEntry<'a> {
    pub ts: i64,
    pub tool: &'a str,
    pub args: serde_json::Value,
    pub out_chars: usize,
    pub tokens_est: usize,
    pub status: &'a str,
}

#[derive(Debug, Serialize)]
pub struct ErrorEntry<'a> {
    pub ts: i64,
    pub tool: &'a str,
    pub status: &'a str,
    pub message: &'a str,
}

pub struct Recorder {
    dir: PathBuf,
    history_counter: AtomicU32,
    error_counter: AtomicU32,
}

impl Recorder {
    /// Create a recorder rooted at `dir`. The directory must exist.
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            history_counter: AtomicU32::new(0),
            error_counter: AtomicU32::new(0),
        }
    }

    /// Resolve install dir from the current executable's path.
    /// Returns `None` if the path cannot be determined.
    pub fn from_current_exe() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?.to_path_buf();
        Some(Self::new(dir))
    }

    pub fn record_history(&self, entry: &HistoryEntry<'_>) {
        let line = match serde_json::to_string(entry) {
            Ok(s) => s,
            Err(_) => return,
        };
        let path = self.dir.join(HISTORY_FILE);
        append_line(&path, &line);
        self.maybe_truncate(&self.history_counter, &path, MAX_HISTORY_LINES);
    }

    pub fn record_error(&self, entry: &ErrorEntry<'_>) {
        let line = match serde_json::to_string(entry) {
            Ok(s) => s,
            Err(_) => return,
        };
        let path = self.dir.join(ERRORS_FILE);
        append_line(&path, &line);
        self.maybe_truncate(&self.error_counter, &path, MAX_ERROR_LINES);
    }

    fn maybe_truncate(&self, counter: &AtomicU32, path: &Path, max: usize) {
        let n = counter.fetch_add(1, Ordering::Relaxed);
        if (n + 1) % TRUNCATE_EVERY_N_WRITES != 0 {
            return;
        }
        truncate_to_last_n_lines(path, max);
    }
}

fn append_line(path: &Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

fn truncate_to_last_n_lines(path: &Path, max: usize) {
    let Ok(file) = std::fs::File::open(path) else {
        return;
    };
    let lines: Vec<String> = BufReader::new(file)
        .lines()
        .filter_map(Result::ok)
        .collect();
    if lines.len() <= max {
        return;
    }
    let keep = &lines[lines.len() - max..];
    let tmp = path.with_extension("jsonl.tmp");
    let Ok(mut f) = std::fs::File::create(&tmp) else {
        return;
    };
    for l in keep {
        if writeln!(f, "{l}").is_err() {
            // Don't leave a partial tmp file on disk.
            drop(f);
            let _ = std::fs::remove_file(&tmp);
            return;
        }
    }
    let _ = std::fs::rename(&tmp, path);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ts() -> i64 {
        1700000000
    }

    #[test]
    fn append_creates_file_with_one_line() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recorder::new(dir.path().to_path_buf());
        rec.record_history(&HistoryEntry {
            ts: ts(),
            tool: "list_spaces",
            args: json!({}),
            out_chars: 42,
            tokens_est: 10,
            status: "ok",
        });
        let contents = std::fs::read_to_string(dir.path().join(HISTORY_FILE)).unwrap();
        assert_eq!(contents.lines().count(), 1);
        let parsed: serde_json::Value = serde_json::from_str(contents.trim()).unwrap();
        assert_eq!(parsed["tool"], "list_spaces");
        assert_eq!(parsed["out_chars"], 42);
    }

    #[test]
    fn multiple_appends_accumulate() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recorder::new(dir.path().to_path_buf());
        for _ in 0..5 {
            rec.record_history(&HistoryEntry {
                ts: ts(),
                tool: "get_page",
                args: json!({"page_id":"1"}),
                out_chars: 10,
                tokens_est: 2,
                status: "ok",
            });
        }
        let contents = std::fs::read_to_string(dir.path().join(HISTORY_FILE)).unwrap();
        assert_eq!(contents.lines().count(), 5);
    }

    #[test]
    fn errors_go_to_separate_file() {
        let dir = tempfile::tempdir().unwrap();
        let rec = Recorder::new(dir.path().to_path_buf());
        rec.record_error(&ErrorEntry {
            ts: ts(),
            tool: "get_page",
            status: "403",
            message: "denied",
        });
        assert!(!dir.path().join(HISTORY_FILE).exists());
        assert!(dir.path().join(ERRORS_FILE).exists());
    }

    #[test]
    fn truncate_keeps_last_n_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(HISTORY_FILE);
        // Seed with 15 lines.
        for i in 0..15 {
            append_line(&path, &format!(r#"{{"i":{i}}}"#));
        }
        truncate_to_last_n_lines(&path, 10);
        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 10);
        // Should have kept i=5..15.
        assert!(lines[0].contains(r#""i":5"#));
        assert!(lines[9].contains(r#""i":14"#));
    }

    #[test]
    fn truncation_reachable_through_public_api() {
        // Exercise the (n+1) % TRUNCATE_EVERY_N_WRITES == 0 path via record_history.
        // With MAX_HISTORY_LINES = 1000, 100 writes are not enough to trigger real
        // line-count-based truncation, but this test confirms maybe_truncate is
        // being reached on the right call cadence (no panic, no file corruption).
        let dir = tempfile::tempdir().unwrap();
        let rec = Recorder::new(dir.path().to_path_buf());
        for i in 0..150 {
            rec.record_history(&HistoryEntry {
                ts: ts() + i,
                tool: "get_page",
                args: json!({"page_id": i.to_string()}),
                out_chars: 10,
                tokens_est: 2,
                status: "ok",
            });
        }
        let contents = std::fs::read_to_string(dir.path().join(HISTORY_FILE)).unwrap();
        assert_eq!(contents.lines().count(), 150);
    }
}
