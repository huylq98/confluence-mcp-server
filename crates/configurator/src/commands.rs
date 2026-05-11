use crate::analyzer::{analyze, Tip};
use crate::claude_config::{
    default_config_path, read_config, remove_confluence_entry, write_confluence_entry,
    ConfluenceEntry,
};
use crate::installer::{default_install_dir, extract_server, probe_writable, resolve_install_dir};
use crate::stats::{read_errors, read_history, summarize, StatsSummary};
use confluence_core::{Client, Config, ConfluenceError};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedConfig {
    pub config_exists: bool,
    pub confluence_configured: bool,
    pub url: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub ssl_verify: bool,
    pub install_dir: String,
    pub proxy_url: String,
    /// Proxy string to pre-fill into the input (may be a static proxy, a PAC
    /// URL, or the WPAD sentinel).
    pub detected_proxy_url: String,
    /// Which kind of proxy was detected: "static" | "pac" | "wpad" | "".
    pub detected_proxy_kind: String,
}

#[tauri::command]
pub async fn load_existing_config() -> Result<LoadedConfig, String> {
    let path = default_config_path();
    let existing = read_config(&path).map_err(|e| e.to_string())?;
    let (url, username, password, token, ssl_verify, install_dir, proxy_url) =
        match &existing.confluence {
            Some(c) => {
                let dir = PathBuf::from(&c.command)
                    .parent()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| default_install_dir().to_string_lossy().to_string());
                (
                    c.url.clone(),
                    c.username.clone().unwrap_or_default(),
                    c.password.clone().unwrap_or_default(),
                    c.token.clone().unwrap_or_default(),
                    c.ssl_verify,
                    dir,
                    c.proxy_url.clone().unwrap_or_default(),
                )
            }
            None => (
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                true,
                default_install_dir().to_string_lossy().to_string(),
                String::new(),
            ),
        };
    let detected = confluence_core::system_proxy::detect();
    let detected_proxy_url = detected.display().unwrap_or_default();
    let detected_proxy_kind = if detected.pac_url.is_some() {
        "pac"
    } else if detected.static_proxy.is_some() {
        "static"
    } else if detected.auto_detect {
        "wpad"
    } else {
        ""
    }
    .to_string();
    Ok(LoadedConfig {
        config_exists: existing.path_exists,
        confluence_configured: existing.confluence.is_some(),
        url,
        username,
        password,
        token,
        ssl_verify,
        install_dir,
        proxy_url,
        detected_proxy_url,
        detected_proxy_kind,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionArgs {
    pub url: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub ssl_verify: bool,
    #[serde(default)]
    pub proxy_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn test_connection(args: TestConnectionArgs) -> Result<TestConnectionResult, String> {
    if args.url.trim().is_empty() {
        return Ok(TestConnectionResult {
            success: false,
            message: "Please enter the Confluence URL.".into(),
        });
    }
    if args.token.is_empty() && (args.username.is_empty() || args.password.is_empty()) {
        return Ok(TestConnectionResult {
            success: false,
            message: "Please enter either a Personal Access Token or both Username and Password."
                .into(),
        });
    }

    let cfg = Config {
        confluence_url: args.url.trim_end_matches('/').into(),
        username: (!args.username.is_empty()).then_some(args.username),
        password: (!args.password.is_empty()).then_some(args.password),
        token: (!args.token.is_empty()).then_some(args.token),
        ssl_verify: args.ssl_verify,
        ca_bundle: None,
        proxy_url: (!args.proxy_url.trim().is_empty()).then(|| args.proxy_url.trim().to_string()),
        timeout: Duration::from_secs(15),
        rate_limit: 5,
        max_content_length: 50_000,
        default_search_limit: 10,
        log_level: "WARN".into(),
    };

    let client = match Client::new(cfg) {
        Ok(c) => c,
        Err(e) => {
            return Ok(TestConnectionResult {
                success: false,
                message: e.to_string(),
            })
        }
    };

    match client.list_spaces(Some("global"), 5, "").await {
        Ok(data) => {
            let count = data
                .pointer("/results")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(TestConnectionResult {
                success: true,
                message: format!("Connected! Found {count} space(s)."),
            })
        }
        Err(e) => {
            let detail = format_error_chain(&e);
            let msg = match e.status_code() {
                401 => format!("Authentication failed. Check your username/password or token.\n\n{detail}"),
                403 if detail.contains("CAPTCHA_CHALLENGE") => format!(
                    "Confluence is requiring CAPTCHA for your account. Open Confluence in a browser, sign in, solve the CAPTCHA, then retry here.\n\n{detail}"
                ),
                403 => format!("Confluence refused the request (403). This is usually CAPTCHA, account lockout, or a WAF rule.\n\n{detail}"),
                0 => {
                    let mut hints: Vec<&str> = vec![
                        "- The URL is correct",
                        "- You are connected to VPN (if required)",
                        "- The server is running",
                    ];
                    let target_is_private = url_host(&args.url)
                        .map(|h| is_private_or_local_host(&h))
                        .unwrap_or(false);
                    let proxy_configured = !args.proxy_url.trim().is_empty();
                    if target_is_private && proxy_configured {
                        hints.push(
                            "- The proxy can route to this address — corporate HTTP proxies \
                             often refuse internal IPs (10.x / 192.168.x / internal hostnames). \
                             Try clearing the Proxy field if the Confluence server is on your \
                             internal network.",
                        );
                    }
                    format!(
                        "Cannot reach the server. Please check:\n{}\n\n{detail}",
                        hints.join("\n")
                    )
                }
                code => format!("Error (HTTP {code}): {detail}"),
            };
            Ok(TestConnectionResult {
                success: false,
                message: msg,
            })
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigArgs {
    pub url: String,
    pub username: String,
    pub password: String,
    pub token: String,
    pub ssl_verify: bool,
    pub install_dir: String,
    #[serde(default)]
    pub proxy_url: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConfigResult {
    pub success: bool,
    pub message: String,
    pub config_path: String,
    pub server_path: String,
}

#[tauri::command]
pub async fn save_config(args: SaveConfigArgs) -> Result<SaveConfigResult, String> {
    let dir = resolve_install_dir(Some(args.install_dir.clone()));
    if let Err(e) = probe_writable(&dir) {
        return Ok(SaveConfigResult {
            success: false,
            message: format!(
                "Cannot write to {}: {e}. Pick a different folder.",
                dir.display()
            ),
            config_path: String::new(),
            server_path: String::new(),
        });
    }

    let server_path = match extract_server(&dir) {
        Ok(p) => p,
        Err(e) => {
            let message = if e.raw_os_error() == Some(32) {
                format!(
                    "Failed to extract server binary: the previous server is still running \
                     and has the file locked. Quit Claude Desktop completely (check the \
                     system tray) and try Save again. ({e})"
                )
            } else {
                format!(
                    "Failed to extract server binary: {e}. Your antivirus may be blocking \
                     this — add an exception or choose a different folder."
                )
            };
            return Ok(SaveConfigResult {
                success: false,
                message,
                config_path: String::new(),
                server_path: String::new(),
            });
        }
    };

    let entry = ConfluenceEntry {
        command: server_path.to_string_lossy().replace('\\', "/"),
        url: args.url.clone(),
        username: (!args.username.is_empty()).then_some(args.username),
        password: (!args.password.is_empty()).then_some(args.password),
        token: (!args.token.is_empty()).then_some(args.token),
        ssl_verify: args.ssl_verify,
        proxy_url: (!args.proxy_url.trim().is_empty()).then(|| args.proxy_url.trim().to_string()),
    };
    let config_path = default_config_path();
    if let Err(e) = write_confluence_entry(&config_path, &entry) {
        return Ok(SaveConfigResult {
            success: false,
            message: format!(
                "Cannot write Claude Desktop config: {e}. Try running as Administrator."
            ),
            config_path: config_path.to_string_lossy().into(),
            server_path: server_path.to_string_lossy().into(),
        });
    }

    Ok(SaveConfigResult {
        success: true,
        message: "Configuration saved! Restart Claude Desktop to activate.".into(),
        config_path: config_path.to_string_lossy().into(),
        server_path: server_path.to_string_lossy().into(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveResult {
    pub success: bool,
    pub message: String,
}

#[tauri::command]
pub async fn remove_config(install_dir: String) -> Result<RemoveResult, String> {
    let dir = PathBuf::from(&install_dir);
    let config_path = default_config_path();
    if let Err(e) = remove_confluence_entry(&config_path) {
        return Ok(RemoveResult {
            success: false,
            message: format!("Failed to update config: {e}"),
        });
    }
    let server_path = dir.join(crate::installer::SERVER_BINARY_NAME);
    let _ = std::fs::remove_file(&server_path);
    let _ = std::fs::remove_dir(&dir); // only removes if empty
    Ok(RemoveResult {
        success: true,
        message: "Confluence MCP removed. Restart Claude Desktop to apply.".into(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub memory_mb: Option<u64>,
    pub configured: bool,
}

#[tauri::command]
pub async fn server_status() -> Result<ServerStatus, String> {
    use sysinfo::System;

    let configured = read_config(&default_config_path())
        .ok()
        .map(|e| e.confluence.is_some())
        .unwrap_or(false);

    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let target = crate::installer::SERVER_BINARY_NAME;
    let mut running = None;
    for (pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy();
        if name.eq_ignore_ascii_case(target) {
            running = Some((pid.as_u32(), proc.memory() / 1024 / 1024));
            break;
        }
    }

    Ok(match running {
        Some((pid, memory_mb)) => ServerStatus {
            running: true,
            pid: Some(pid),
            memory_mb: Some(memory_mb),
            configured,
        },
        None => ServerStatus {
            running: false,
            pid: None,
            memory_mb: None,
            configured,
        },
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StopResult {
    pub success: bool,
    pub message: String,
    pub killed: u32,
}

#[tauri::command]
pub async fn stop_server() -> Result<StopResult, String> {
    use sysinfo::System;

    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let target = crate::installer::SERVER_BINARY_NAME;
    let mut killed = 0u32;
    for (_, proc) in sys.processes() {
        let name = proc.name().to_string_lossy();
        if name.eq_ignore_ascii_case(target) && proc.kill() {
            killed += 1;
        }
    }

    Ok(StopResult {
        success: killed > 0,
        killed,
        message: if killed == 0 {
            "Server is not running.".into()
        } else {
            format!(
                "Stopped {killed} server process(es). Claude Desktop may relaunch it \
                 automatically; use Remove below to unregister it permanently."
            )
        },
    })
}

fn install_dir_from_config() -> Option<PathBuf> {
    let existing = read_config(&default_config_path()).ok()?;
    let cmd = existing.confluence?.command;
    PathBuf::from(cmd).parent().map(|p| p.to_path_buf())
}

#[tauri::command]
pub async fn get_stats() -> Result<StatsSummary, String> {
    let dir = install_dir_from_config().ok_or_else(|| "Not configured yet.".to_string())?;
    let history = read_history(&dir);
    let errors = read_errors(&dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(summarize(&history, &errors, now))
}

#[tauri::command]
pub async fn get_recommendations() -> Result<Vec<Tip>, String> {
    let dir = install_dir_from_config().ok_or_else(|| "Not configured yet.".to_string())?;
    let history = read_history(&dir);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Ok(analyze(&history, now))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTestResult {
    pub success: bool,
    pub message: String,
    pub latency_ms: u64,
    pub space_count: usize,
}

fn config_from_entry(entry: ConfluenceEntry, timeout: Duration) -> Config {
    Config {
        confluence_url: entry.url.trim_end_matches('/').into(),
        username: entry.username,
        password: entry.password,
        token: entry.token,
        ssl_verify: entry.ssl_verify,
        ca_bundle: None,
        proxy_url: entry.proxy_url,
        timeout,
        rate_limit: 5,
        max_content_length: 50_000,
        default_search_limit: 10,
        log_level: "WARN".into(),
    }
}

#[tauri::command]
pub async fn test_live_connection() -> Result<LiveTestResult, String> {
    let existing = read_config(&default_config_path()).map_err(|e| e.to_string())?;
    let entry = existing
        .confluence
        .ok_or_else(|| "Not configured yet.".to_string())?;

    let cfg = config_from_entry(entry, Duration::from_secs(10));

    let client = Client::new(cfg).map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    match client.list_spaces(Some("global"), 5, "").await {
        Ok(data) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let count = data
                .pointer("/results")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(LiveTestResult {
                success: true,
                message: format!("OK · {count} space(s) · {latency_ms} ms"),
                latency_ms,
                space_count: count,
            })
        }
        Err(e) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            Ok(LiveTestResult {
                success: false,
                message: format_error_chain(&e),
                latency_ms,
                space_count: 0,
            })
        }
    }
}

#[tauri::command]
pub async fn copy_diagnostics() -> Result<String, String> {
    let dir = install_dir_from_config().ok_or_else(|| "Not configured yet.".to_string())?;
    let history = read_history(&dir);
    let errors = read_errors(&dir);

    let mut md = String::new();
    md.push_str("# Confluence Connect — diagnostics\n\n");
    md.push_str(&format!(
        "## Recent tool calls ({} rows, newest last)\n\n",
        history.len().min(100)
    ));
    md.push_str("| ts | tool | args | tokens | status |\n|---|---|---|---|---|\n");
    for r in history.iter().rev().take(100).rev() {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            r.ts,
            esc_md(&r.tool),
            esc_md(&r.args.to_string()),
            r.tokens_est,
            esc_md(&r.status),
        ));
    }
    md.push_str("\n## Recent errors\n\n");
    for e in errors.iter().take(20) {
        md.push_str(&format!(
            "- `{}` · **{}** · {}: {}\n",
            e.ts, e.tool, e.status, e.message
        ));
    }
    md.push_str(
        "\n---\nPlease analyze usage patterns and suggest ways to reduce token usage, \
         avoid repeated fetches, or narrow my CQL queries. Any error patterns I should fix?\n",
    );

    arboard::Clipboard::new()
        .and_then(|mut cb| cb.set_text(md.clone()))
        .map_err(|e| format!("Clipboard error: {e}"))?;
    Ok(format!(
        "Copied {} chars to clipboard — paste into Claude Desktop.",
        md.len()
    ))
}

#[tauri::command]
pub async fn open_claude_log() -> Result<(), String> {
    // Windows: %APPDATA%\Claude\logs\
    let appdata =
        std::env::var_os("APPDATA").ok_or_else(|| "APPDATA env var not set".to_string())?;
    let logs = PathBuf::from(appdata).join("Claude").join("logs");
    if !logs.exists() {
        return Err(format!("Log folder not found at {}", logs.display()));
    }
    opener::open(&logs).map_err(|e| format!("Failed to open folder: {e}"))
}

#[tauri::command]
pub async fn open_external_url(url: String) -> Result<(), String> {
    // Minimal guard: only allow http(s) URLs to prevent opening arbitrary
    // schemes (e.g., file://, javascript:) if this command is ever called
    // with untrusted input.
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("Refusing to open non-http(s) URL: {url}"));
    }
    opener::open(&url).map_err(|e| format!("Failed to open URL: {e}"))
}

/// Walk the error's `source()` chain so the user sees the real cause
/// ("connection refused", "dns error", "tls handshake failed", …) instead of
/// just reqwest's generic top-level "error sending request".
fn format_error_chain(err: &ConfluenceError) -> String {
    let mut parts = vec![err.to_string()];
    let mut src: Option<&(dyn std::error::Error + 'static)> = std::error::Error::source(err);
    while let Some(e) = src {
        let msg = e.to_string();
        if !parts.last().is_some_and(|last| last.contains(&msg)) {
            parts.push(msg);
        }
        src = e.source();
    }
    let base = parts.join(" → ");
    let lower = base.to_lowercase();

    let mut hints: Vec<&str> = Vec::new();
    if lower.contains("timed out") || lower.contains("timeout") {
        hints.push("Connection timed out — check the URL and that you're on VPN if the server is internal.");
    }
    if lower.contains("invalid") && lower.contains("certificate") {
        hints.push("CA bundle invalid — re-check the path, and confirm the file is PEM-encoded.");
    }
    if lower.contains("dns error") || lower.contains("failed to lookup") {
        hints.push("DNS lookup failed — the hostname isn't resolving. Check spelling and DNS/VPN.");
    }
    if hints.is_empty() {
        base
    } else {
        format!("{base}\n\nHint:\n- {}", hints.join("\n- "))
    }
}

fn url_host(s: &str) -> Option<String> {
    url::Url::parse(s.trim())
        .ok()
        .and_then(|u| u.host_str().map(String::from))
}

/// Escape pipe characters so they don't break markdown table column layout.
fn esc_md(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Returns true for RFC1918 private IPs, loopback, link-local, and plain
/// hostnames without a dot (e.g. `wiki`, `intranet`) — addresses that
/// external-facing HTTP proxies typically can't or won't route to.
fn is_private_or_local_host(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return match ip {
            IpAddr::V4(v4) => {
                v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
            }
            IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
        };
    }
    let h = host.to_ascii_lowercase();
    h == "localhost" || !h.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_ipv4_detected() {
        assert!(is_private_or_local_host("10.254.136.35"));
        assert!(is_private_or_local_host("192.168.1.1"));
        assert!(is_private_or_local_host("172.16.5.5"));
        assert!(is_private_or_local_host("127.0.0.1"));
    }

    #[test]
    fn public_ipv4_not_private() {
        assert!(!is_private_or_local_host("8.8.8.8"));
        assert!(!is_private_or_local_host("172.15.0.1")); // just outside 172.16/12
    }

    #[test]
    fn bare_hostname_treated_as_local() {
        assert!(is_private_or_local_host("wiki"));
        assert!(is_private_or_local_host("intranet"));
        assert!(is_private_or_local_host("localhost"));
    }

    #[test]
    fn fqdn_is_not_local() {
        assert!(!is_private_or_local_host("wiki.example.com"));
        assert!(!is_private_or_local_host("confluence.corp.example"));
    }

    #[test]
    fn esc_md_escapes_pipe() {
        assert_eq!(esc_md("a|b"), "a\\|b");
        assert_eq!(esc_md("no pipes"), "no pipes");
        assert_eq!(esc_md("|||"), "\\|\\|\\|");
    }

    #[test]
    fn format_error_chain_adds_timeout_hint() {
        let err = ConfluenceError::Http {
            status: 0,
            message: "request timed out after 15s".into(),
        };
        let out = format_error_chain(&err);
        assert!(out.contains("Connection timed out"), "got: {out}");
    }
}
