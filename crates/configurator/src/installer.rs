use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const SERVER_BINARY_NAME: &str = "confluence-mcp-server.exe";

/// The server binary embedded at compile time.
const EMBEDDED_SERVER: &[u8] = include_bytes!("../resources/confluence-mcp-server.exe");

/// Ordered candidate paths for the default install dir.
fn install_dir_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if cfg!(windows) {
        if let Some(d) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(d).join("ConfluenceConnect"));
        }
        if let Some(d) = std::env::var_os("USERPROFILE") {
            v.push(PathBuf::from(d).join("ConfluenceConnect"));
        }
    } else if let Some(d) = std::env::var_os("HOME") {
        v.push(PathBuf::from(d).join(".local").join("share").join("ConfluenceConnect"));
    }
    if v.is_empty() {
        v.push(PathBuf::from("ConfluenceConnect"));
    }
    v
}

/// Returns the first writable candidate path. If none are writable, returns the
/// preferred candidate anyway so the UI can display it and the user can Change…
pub fn default_install_dir() -> PathBuf {
    let candidates = install_dir_candidates();
    for c in &candidates {
        if probe_writable(c).is_ok() {
            return c.clone();
        }
    }
    candidates.into_iter().next().expect("at least one candidate")
}

pub fn resolve_install_dir(override_path: Option<String>) -> PathBuf {
    match override_path {
        Some(s) if !s.trim().is_empty() => PathBuf::from(s),
        _ => default_install_dir(),
    }
}

pub fn probe_writable(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let probe = dir.join(".probe");
    {
        let mut f = fs::File::create(&probe)?;
        f.write_all(b"ok")?;
    }
    fs::remove_file(&probe)
}

pub fn extract_server(dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let target = dir.join(SERVER_BINARY_NAME);
    fs::write(&target, EMBEDDED_SERVER)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target, perms)?;
    }
    Ok(target)
}
