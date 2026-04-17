use configurator::installer::{probe_writable, default_install_dir, resolve_install_dir};
use tempfile::TempDir;

#[test]
fn probe_succeeds_on_writable_dir() {
    let d = TempDir::new().unwrap();
    assert!(probe_writable(d.path()).is_ok());
}

#[test]
fn probe_creates_missing_parent_then_succeeds() {
    let d = TempDir::new().unwrap();
    let nested = d.path().join("a/b/c");
    assert!(probe_writable(&nested).is_ok());
    assert!(nested.is_dir());
}

#[test]
fn default_install_dir_is_under_known_variable() {
    let p = default_install_dir();
    // On Windows, path is under LOCALAPPDATA or USERPROFILE; on Linux/Mac under home.
    let s = p.to_string_lossy();
    assert!(s.ends_with("ConfluenceConnect"), "unexpected path: {s}");
}

#[test]
fn resolve_install_dir_uses_override() {
    let d = TempDir::new().unwrap();
    let got = resolve_install_dir(Some(d.path().to_string_lossy().to_string()));
    assert_eq!(got, d.path());
}
