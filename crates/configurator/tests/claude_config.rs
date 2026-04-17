use configurator::claude_config::{read_config, write_confluence_entry, remove_confluence_entry, ConfluenceEntry};
use serde_json::json;
use tempfile::TempDir;
use std::fs;

fn tmp(initial: Option<&str>) -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("claude_desktop_config.json");
    if let Some(text) = initial {
        fs::write(&path, text).unwrap();
    }
    (dir, path)
}

#[test]
fn reads_existing_confluence_entry() {
    let (_dir, path) = tmp(Some(r#"{
        "mcpServers": {
            "confluence": {
                "command": "C:\\\\app\\\\server.exe",
                "args": [],
                "env": {"CONFLUENCE_URL": "https://wiki.example.com", "CONFLUENCE_TOKEN": "t"}
            }
        }
    }"#));
    let existing = read_config(&path).unwrap();
    assert!(existing.confluence.is_some());
    let c = existing.confluence.unwrap();
    assert_eq!(c.url, "https://wiki.example.com");
    assert_eq!(c.token.as_deref(), Some("t"));
}

#[test]
fn write_preserves_other_mcp_servers() {
    let (_dir, path) = tmp(Some(r#"{
        "mcpServers": {
            "other": {"command": "C:\\\\other.exe", "args": []}
        }
    }"#));
    let entry = ConfluenceEntry {
        command: r"C:\app\server.exe".into(),
        url: "https://wiki".into(),
        username: None,
        password: None,
        token: Some("t".into()),
        ssl_verify: true,
    };
    write_confluence_entry(&path, &entry).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert!(parsed.pointer("/mcpServers/other").is_some());
    assert_eq!(parsed.pointer("/mcpServers/confluence/env/CONFLUENCE_TOKEN").unwrap(), "t");
}

#[test]
fn malformed_config_is_backed_up_and_replaced() {
    let (_dir, path) = tmp(Some("this is not json"));
    let entry = ConfluenceEntry {
        command: r"C:\server.exe".into(),
        url: "https://wiki".into(),
        username: None, password: None,
        token: Some("t".into()),
        ssl_verify: true,
    };
    write_confluence_entry(&path, &entry).unwrap();

    // A malformed backup file should exist alongside
    let dir = path.parent().unwrap();
    let backups: Vec<_> = fs::read_dir(dir).unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("malformed"))
        .collect();
    assert_eq!(backups.len(), 1);
}

#[test]
fn remove_deletes_entry() {
    let (_dir, path) = tmp(Some(r#"{"mcpServers": {"confluence": {"command": "x"}, "other": {"command": "y"}}}"#));
    remove_confluence_entry(&path).unwrap();

    let parsed: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    assert!(parsed.pointer("/mcpServers/confluence").is_none());
    assert!(parsed.pointer("/mcpServers/other").is_some());
}
