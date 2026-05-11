//! End-to-end check that `extract_server` recovers from
//! `ERROR_SHARING_VIOLATION` by killing the running server and retrying — the
//! exact failure mode users hit when re-running the wizard while Claude
//! Desktop has the previous server binary loaded.

#![cfg(windows)]

use configurator::installer::{extract_server, SERVER_BINARY_NAME};
use std::fs;
use std::io::Read;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

/// Path to the freshly-built server binary that the configurator embeds.
fn embedded_server_source() -> std::path::PathBuf {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.join("resources").join(SERVER_BINARY_NAME)
}

#[test]
fn extract_server_recovers_from_running_instance_holding_lock() {
    let src = embedded_server_source();
    assert!(
        src.exists(),
        "expected resources/{SERVER_BINARY_NAME} to be present \
         (run scripts/build.ps1 first): {}",
        src.display()
    );

    let dir = TempDir::new().expect("temp dir");
    let target = dir.path().join(SERVER_BINARY_NAME);
    fs::copy(&src, &target).expect("seed the target with a runnable binary");

    // Spawn the binary so Windows takes an image-section lock on the file.
    // The MCP server blocks on stdin → it'll stay alive until we kill it.
    let mut child = Command::new(&target)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn child server");

    // Wait until the OS actually has the file locked. Polling beats sleeping
    // because Windows image-section setup is async and varies by machine.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&target)
        {
            Err(e) if e.raw_os_error() == Some(32) => break, // locked — what we want
            Ok(_) | Err(_) => {
                if Instant::now() > deadline {
                    let _ = child.kill();
                    panic!("file never became locked by the spawned server");
                }
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    // Smoke-check: the naïve write *does* fail with the very error our fix
    // exists to handle. Without this assertion the test would silently degrade
    // if Windows ever changed its locking semantics.
    let raw_err = fs::write(&target, b"naive overwrite attempt").unwrap_err();
    assert_eq!(
        raw_err.raw_os_error(),
        Some(32),
        "expected ERROR_SHARING_VIOLATION before the fix runs, got: {raw_err}"
    );

    // The fix under test: should kill the running server and rewrite the file.
    let extracted = extract_server(dir.path()).expect("extract_server must recover");
    assert_eq!(extracted, target);

    // Drain pipes so the child can't deadlock us, then confirm it actually
    // died (the kill path inside extract_server should have terminated it).
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_end(&mut Vec::new());
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_end(&mut Vec::new());
    }
    let status = child.wait().expect("child wait");
    assert!(
        !status.success(),
        "child should have been killed by extract_server, not exited cleanly"
    );

    // Verify the new file is actually our embedded binary. Same source as the
    // seed → contents must still match the on-disk reference.
    let written = fs::read(&target).expect("read rewritten target");
    let reference = fs::read(&src).expect("read reference binary");
    assert_eq!(
        written.len(),
        reference.len(),
        "rewritten binary size diverged from embedded reference"
    );
    assert_eq!(written, reference, "rewritten binary contents diverged");
}
