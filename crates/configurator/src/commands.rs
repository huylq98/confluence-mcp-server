use crate::claude_config::{default_config_path, read_config, remove_confluence_entry, write_confluence_entry, ConfluenceEntry};
use crate::installer::{extract_server, probe_writable, resolve_install_dir, default_install_dir};
use confluence_core::{Client, Config};
use serde::{Deserialize, Serialize};
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
}

#[tauri::command]
pub async fn load_existing_config() -> Result<LoadedConfig, String> {
    let path = default_config_path();
    let existing = read_config(&path).map_err(|e| e.to_string())?;
    let (url, username, password, token, ssl_verify, install_dir) = match &existing.confluence {
        Some(c) => {
            let dir = PathBuf::from(&c.command).parent().map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| default_install_dir().to_string_lossy().to_string());
            (
                c.url.clone(),
                c.username.clone().unwrap_or_default(),
                c.password.clone().unwrap_or_default(),
                c.token.clone().unwrap_or_default(),
                c.ssl_verify,
                dir,
            )
        }
        None => (
            String::new(), String::new(), String::new(), String::new(), true,
            default_install_dir().to_string_lossy().to_string(),
        ),
    };
    Ok(LoadedConfig {
        config_exists: existing.path_exists,
        confluence_configured: existing.confluence.is_some(),
        url, username, password, token, ssl_verify, install_dir,
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
        return Ok(TestConnectionResult { success: false, message: "Please enter the Confluence URL.".into() });
    }
    if args.token.is_empty() && (args.username.is_empty() || args.password.is_empty()) {
        return Ok(TestConnectionResult {
            success: false,
            message: "Please enter either a Personal Access Token or both Username and Password.".into(),
        });
    }

    let cfg = Config {
        confluence_url: args.url.trim_end_matches('/').into(),
        username: (!args.username.is_empty()).then_some(args.username),
        password: (!args.password.is_empty()).then_some(args.password),
        token: (!args.token.is_empty()).then_some(args.token),
        ssl_verify: args.ssl_verify,
        ca_bundle: None,
        timeout: Duration::from_secs(15),
        rate_limit: 5,
        max_content_length: 50_000,
        default_search_limit: 10,
        log_level: "WARN".into(),
    };

    let client = match Client::new(cfg) {
        Ok(c) => c,
        Err(e) => return Ok(TestConnectionResult { success: false, message: e.to_string() }),
    };

    match client.list_spaces(Some("global"), 5, "").await {
        Ok(data) => {
            let count = data.pointer("/results").and_then(|v| v.as_array()).map(|a| a.len()).unwrap_or(0);
            Ok(TestConnectionResult { success: true, message: format!("Connected! Found {count} space(s).") })
        }
        Err(e) => {
            let detail = e.to_string();
            let msg = match e.status_code() {
                401 => format!("Authentication failed. Check your username/password or token.\n\n{detail}"),
                403 if detail.contains("CAPTCHA_CHALLENGE") => format!(
                    "Confluence is requiring CAPTCHA for your account. Open Confluence in a browser, sign in, solve the CAPTCHA, then retry here.\n\n{detail}"
                ),
                403 => format!("Confluence refused the request (403). This is usually CAPTCHA, account lockout, or a WAF rule.\n\n{detail}"),
                0 => format!("Cannot reach the server. Please check:\n- The URL is correct\n- You are connected to VPN (if required)\n- The server is running\n\n{detail}"),
                code => format!("Error (HTTP {code}): {detail}"),
            };
            Ok(TestConnectionResult { success: false, message: msg })
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
            message: format!("Cannot write to {}: {e}. Pick a different folder.", dir.display()),
            config_path: String::new(), server_path: String::new(),
        });
    }

    let server_path = match extract_server(&dir) {
        Ok(p) => p,
        Err(e) => return Ok(SaveConfigResult {
            success: false,
            message: format!("Failed to extract server binary: {e}. Your antivirus may be blocking this — add an exception or choose a different folder."),
            config_path: String::new(), server_path: String::new(),
        }),
    };

    let entry = ConfluenceEntry {
        command: server_path.to_string_lossy().replace('\\', "/"),
        url: args.url.clone(),
        username: (!args.username.is_empty()).then_some(args.username),
        password: (!args.password.is_empty()).then_some(args.password),
        token: (!args.token.is_empty()).then_some(args.token),
        ssl_verify: args.ssl_verify,
    };
    let config_path = default_config_path();
    if let Err(e) = write_confluence_entry(&config_path, &entry) {
        return Ok(SaveConfigResult {
            success: false,
            message: format!("Cannot write Claude Desktop config: {e}. Try running as Administrator."),
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
        return Ok(RemoveResult { success: false, message: format!("Failed to update config: {e}") });
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
