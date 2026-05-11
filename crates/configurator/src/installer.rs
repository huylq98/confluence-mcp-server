use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(windows)]
pub const SERVER_BINARY_NAME: &str = "confluence-mcp-server.exe";
#[cfg(not(windows))]
pub const SERVER_BINARY_NAME: &str = "confluence-mcp-server";

/// The server binary embedded at compile time.
#[cfg(windows)]
const EMBEDDED_SERVER: &[u8] = include_bytes!("../resources/confluence-mcp-server.exe");
#[cfg(not(windows))]
const EMBEDDED_SERVER: &[u8] = include_bytes!("../resources/confluence-mcp-server");

/// Ordered candidate paths for the default install dir.
fn install_dir_candidates() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(windows)]
    {
        if let Some(d) = std::env::var_os("LOCALAPPDATA") {
            v.push(PathBuf::from(d).join("ConfluenceConnect"));
        }
        if let Some(d) = std::env::var_os("USERPROFILE") {
            v.push(PathBuf::from(d).join("ConfluenceConnect"));
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(d) = std::env::var_os("HOME") {
        v.push(
            PathBuf::from(d)
                .join("Library")
                .join("Application Support")
                .join("ConfluenceConnect"),
        );
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    if let Some(d) = std::env::var_os("HOME") {
        v.push(
            PathBuf::from(d)
                .join(".local")
                .join("share")
                .join("ConfluenceConnect"),
        );
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
    candidates
        .into_iter()
        .next()
        .expect("at least one candidate")
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
    write_server_binary(&target)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&target, perms)?;
    }
    Ok(target)
}

/// Writes the embedded server bytes to `target`. On Windows, a running `.exe`
/// is locked by the loader (`ERROR_SHARING_VIOLATION`), so if Claude Desktop
/// already launched a previous build of the server the first write will fail.
/// In that case we kill any running instance and retry while Windows releases
/// the image-section handle.
fn write_server_binary(target: &Path) -> io::Result<()> {
    match fs::write(target, EMBEDDED_SERVER) {
        Ok(()) => return Ok(()),
        Err(e) if !is_sharing_violation(&e) => return Err(e),
        Err(_) => {}
    }
    kill_running_server();
    let mut last = io::Error::new(io::ErrorKind::Other, "no retry attempts ran");
    for attempt in 1..=5u64 {
        std::thread::sleep(Duration::from_millis(150 * attempt));
        match fs::write(target, EMBEDDED_SERVER) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
    }
    Err(last)
}

#[cfg(windows)]
fn is_sharing_violation(e: &io::Error) -> bool {
    // ERROR_SHARING_VIOLATION
    e.raw_os_error() == Some(32)
}

#[cfg(not(windows))]
fn is_sharing_violation(_e: &io::Error) -> bool {
    false
}

#[cfg(windows)]
fn kill_running_server() {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    for proc in sys.processes().values() {
        if proc
            .name()
            .to_string_lossy()
            .eq_ignore_ascii_case(SERVER_BINARY_NAME)
        {
            let _ = proc.kill();
        }
    }
}

#[cfg(not(windows))]
fn kill_running_server() {}
