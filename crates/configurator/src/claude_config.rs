use serde_json::{json, Map, Value};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ConfluenceEntry {
    pub command: String,
    pub url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub token: Option<String>,
    pub ssl_verify: bool,
    pub proxy_url: Option<String>,
}

#[derive(Debug, Default)]
pub struct ExistingConfig {
    pub path_exists: bool,
    pub confluence: Option<ConfluenceEntry>,
}

/// Default config path for the current platform.
pub fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        appdata.join("Claude").join("claude_desktop_config.json")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        home.join(".config")
            .join("Claude")
            .join("claude_desktop_config.json")
    }
}

pub fn read_config(path: &Path) -> std::io::Result<ExistingConfig> {
    let mut out = ExistingConfig::default();
    if !path.is_file() {
        return Ok(out);
    }
    out.path_exists = true;

    let raw = fs::read_to_string(path)?;
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return Ok(out),
    };
    let confluence = parsed.pointer("/mcpServers/confluence").cloned();
    if let Some(c) = confluence {
        let env = c.pointer("/env").cloned().unwrap_or(Value::Null);
        let cmd = c
            .pointer("/command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        out.confluence = Some(ConfluenceEntry {
            command: cmd,
            url: env
                .pointer("/CONFLUENCE_URL")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            username: env
                .pointer("/CONFLUENCE_USERNAME")
                .and_then(Value::as_str)
                .map(String::from),
            password: env
                .pointer("/CONFLUENCE_PASSWORD")
                .and_then(Value::as_str)
                .map(String::from),
            token: env
                .pointer("/CONFLUENCE_TOKEN")
                .and_then(Value::as_str)
                .map(String::from),
            ssl_verify: env
                .pointer("/CONFLUENCE_SSL_VERIFY")
                .and_then(Value::as_str)
                .map(|v| v != "false")
                .unwrap_or(true),
            proxy_url: env
                .pointer("/CONFLUENCE_PROXY_URL")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(String::from),
        });
    }
    Ok(out)
}

pub fn write_confluence_entry(path: &Path, entry: &ConfluenceEntry) -> std::io::Result<()> {
    let mut doc: Value = if path.is_file() {
        match fs::read_to_string(path)
            .ok()
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
        {
            Some(v) => v,
            None => {
                // Malformed — back up and start fresh
                let ts = chrono_now();
                let backup = path.with_extension(format!("json.malformed.{ts}"));
                let _ = fs::copy(path, &backup);
                json!({"mcpServers": {}})
            }
        }
    } else {
        json!({"mcpServers": {}})
    };

    // Backup good existing config
    if path.is_file() && doc.pointer("/mcpServers").is_some() {
        let _ = fs::copy(path, path.with_extension("json.backup"));
    }

    let mut env = Map::new();
    env.insert(
        "CONFLUENCE_URL".into(),
        json!(entry.url.trim_end_matches('/')),
    );
    env.insert("MCP_TRANSPORT".into(), json!("stdio"));
    if let Some(t) = &entry.token {
        env.insert("CONFLUENCE_TOKEN".into(), json!(t));
    } else {
        if let Some(u) = &entry.username {
            env.insert("CONFLUENCE_USERNAME".into(), json!(u));
        }
        if let Some(p) = &entry.password {
            env.insert("CONFLUENCE_PASSWORD".into(), json!(p));
        }
    }
    if !entry.ssl_verify {
        env.insert("CONFLUENCE_SSL_VERIFY".into(), json!("false"));
    }
    if let Some(proxy) = entry
        .proxy_url
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        env.insert("CONFLUENCE_PROXY_URL".into(), json!(proxy));
    }

    let server_entry = json!({
        "command": entry.command,
        "args": [],
        "env": env,
    });

    if doc.get("mcpServers").is_none() {
        doc["mcpServers"] = json!({});
    }
    doc["mcpServers"]["confluence"] = server_entry;

    atomic_write(path, &doc)
}

pub fn remove_confluence_entry(path: &Path) -> std::io::Result<()> {
    if !path.is_file() {
        return Ok(());
    }
    let mut doc: Value = match fs::read_to_string(path)
        .ok()
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
    {
        Some(v) => v,
        None => return Ok(()),
    };
    if let Some(servers) = doc.get_mut("mcpServers").and_then(Value::as_object_mut) {
        servers.remove("confluence");
    }
    atomic_write(path, &doc)
}

fn atomic_write(path: &Path, doc: &Value) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(doc).unwrap();
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
    }
    fs::rename(&tmp, path)
}

fn chrono_now() -> String {
    // Avoid pulling in a date crate; use seconds-since-epoch as a unique suffix.
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}
