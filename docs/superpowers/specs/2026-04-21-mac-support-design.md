# Mac Support Design

**Date:** 2026-04-21  
**Status:** Approved

## Goal

Make Confluence Connect available on macOS so colleagues can download and use it without any developer tooling. Deliverable: an unsigned `.dmg` built by CI, downloadable from GitHub Releases and linked from the GitHub Pages landing page.

## Constraints

- No Apple Developer account — no code signing or notarization. Users bypass Gatekeeper via right-click → Open.
- Distribution must be standalone: the `.dmg` contains a `.app` that embeds the server binary. No runtime required on the user machine.
- Build must run on `macos-latest` GitHub Actions runners.

## Section 1 — Rust Code Changes

### `crates/configurator/src/installer.rs`

**Binary name** — platform-conditional via `cfg!`:
```rust
pub const SERVER_BINARY_NAME: &str = if cfg!(windows) {
    "confluence-mcp-server.exe"
} else {
    "confluence-mcp-server"
};
```

**Embedded binary** — use a build-time environment variable or `cfg` attribute so each platform embeds its own binary. The `resources/` directory holds one binary at a time; the build script places the correct one there before `cargo build`.

**Install directory** — macOS gets `~/Library/Application Support/ConfluenceConnect`, not the Linux `~/.local/share` path:
```rust
#[cfg(target_os = "macos")]
v.push(home.join("Library").join("Application Support").join("ConfluenceConnect"));
#[cfg(not(any(windows, target_os = "macos")))]
v.push(home.join(".local").join("share").join("ConfluenceConnect"));
```

### `crates/configurator/src/claude_config.rs`

**Config path** — Mac Claude Desktop stores its config at `~/Library/Application Support/Claude/claude_desktop_config.json`:
```rust
#[cfg(target_os = "macos")]
{ home.join("Library").join("Application Support").join("Claude").join("claude_desktop_config.json") }
#[cfg(not(any(windows, target_os = "macos")))]
{ home.join(".config").join("Claude").join("claude_desktop_config.json") }
```

### `crates/configurator/tauri.conf.json`

Add `"dmg"` to bundle targets. The `"nsis"` target only builds on Windows; `"dmg"` only builds on macOS. Tauri ignores targets not supported by the current platform.
```json
"targets": ["nsis", "dmg"]
```

Add a macOS icon (`icons/icon.icns`) alongside the existing `icons/icon.ico`.

## Section 2 — CI / Release Workflow

### `rust-ci.yml` — new `build-mac` job

- Runner: `macos-latest`
- Steps: install Rust stable → build server (`cargo build --release -p server`) → copy binary to `crates/configurator/resources/confluence-mcp-server` → build configurator (`cargo build --release -p configurator` or `cargo tauri build`) → upload `ConfluenceConnect.dmg` as a workflow artifact
- Linting (fmt, clippy, tests) stays on the existing Windows job only — no duplication

### New `release.yml` workflow

- Trigger: push of a `v*` tag (e.g. `v0.2.1`)
- Jobs: `build-windows` and `build-mac` run in parallel, each producing their binary
- After both complete: a `publish` job creates a GitHub Release and uploads both `ConfluenceConnect.exe` and `ConfluenceConnect.dmg` as release assets
- This is what populates `releases/latest/download/ConfluenceConnect.exe` and `releases/latest/download/ConfluenceConnect.dmg`

## Section 3 — GitHub Pages (`docs/index.html`)

- Two side-by-side download buttons replace the single Windows button
- Windows: `releases/latest/download/ConfluenceConnect.exe`
- Mac: `releases/latest/download/ConfluenceConnect.dmg`
- Gatekeeper note below Mac button: *"macOS may block the app on first launch — right-click → Open, or go to System Settings → Privacy & Security → Open Anyway"*
- Requirements section gains: **macOS 12 (Monterey) or later**
- Meta description and Step 1 copy updated to mention both Windows and Mac

## What Is Not Changing

- The server crate (`crates/server`) requires no changes — pure Rust, already cross-platform
- The `confluence-core` crate requires no changes
- The existing Windows build pipeline is untouched
- No code signing or notarization (no developer account)
