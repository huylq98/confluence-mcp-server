# Mac Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Confluence Connect available on macOS — unsigned `.dmg` built by CI, downloadable from GitHub Releases and linked from the GitHub Pages landing page.

**Architecture:** Platform-conditional Rust code (`#[cfg]`) handles binary name, install dir, and config path. Tauri 2 builds the `.app`/`.dmg` natively on `macos-latest` CI runners. A new `release.yml` workflow runs Windows and Mac builds in parallel on `v*` tags and creates a GitHub Release with both artifacts.

**Tech Stack:** Rust `#[cfg]` attrs, Tauri 2 (`cargo tauri build`), GitHub Actions `macos-latest` runner, `softprops/action-gh-release`, `sips`+`iconutil` (macOS built-ins for icon generation).

---

## File Map

| File | Change |
|------|--------|
| `crates/configurator/src/installer.rs` | Platform-conditional binary name, embedded bytes, install dir |
| `crates/configurator/src/claude_config.rs` | macOS config path (`~/Library/Application Support/Claude/`) |
| `crates/configurator/tauri.conf.json` | Add `"dmg"` bundle target + `icon.icns` entry |
| `crates/configurator/tests/installer.rs` | Add `#[cfg(target_os = "macos")]` test for install dir |
| `crates/configurator/tests/claude_config.rs` | Add `#[cfg(target_os = "macos")]` test for config path |
| `.github/workflows/rust-ci.yml` | Add `build-mac` job (fmt, server build, clippy, test) |
| `.github/workflows/release.yml` | New: parallel Windows+Mac build → GitHub Release |
| `docs/index.html` | Two download buttons, Mac Gatekeeper note, updated requirements |

---

## Task 1: Fix installer.rs — binary name, embedded bytes, install dir

**Files:**
- Modify: `crates/configurator/src/installer.rs`
- Modify: `crates/configurator/tests/installer.rs`

### Background

`resources/confluence-mcp-server.exe` is gitignored — CI builds the server first and copies it there before compiling the configurator. The same applies on Mac: the CI job will copy `target/release/confluence-mcp-server` to `resources/confluence-mcp-server` before building the configurator. The `#[cfg(not(windows))]` branch for `include_bytes!` is only compiled on Mac/Linux, so Windows builds are unaffected by the missing Mac binary.

- [ ] **Step 1: Write the failing test**

Add to `crates/configurator/tests/installer.rs`:

```rust
#[test]
#[cfg(target_os = "macos")]
fn default_install_dir_uses_library_application_support_on_mac() {
    let p = default_install_dir();
    let s = p.to_string_lossy();
    assert!(
        s.contains("Library/Application Support"),
        "expected Library/Application Support in path: {s}"
    );
    assert!(s.ends_with("ConfluenceConnect"), "expected ConfluenceConnect suffix in: {s}");
}
```

- [ ] **Step 2: Verify test fails on Mac**

On a Mac runner (or after CI picks it up in Task 4):
```
cargo test -p configurator default_install_dir_uses_library_application_support_on_mac
```
Expected: FAIL — path contains `.local/share` not `Library/Application Support`

- [ ] **Step 3: Replace `install_dir_candidates`, `SERVER_BINARY_NAME`, and `EMBEDDED_SERVER` in installer.rs**

Replace lines 1-12 (the `SERVER_BINARY_NAME` constant and `EMBEDDED_SERVER` constant and the `install_dir_candidates` function body) with:

```rust
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[cfg(windows)]
pub const SERVER_BINARY_NAME: &str = "confluence-mcp-server.exe";
#[cfg(not(windows))]
pub const SERVER_BINARY_NAME: &str = "confluence-mcp-server";

#[cfg(windows)]
const EMBEDDED_SERVER: &[u8] = include_bytes!("../resources/confluence-mcp-server.exe");
#[cfg(not(windows))]
const EMBEDDED_SERVER: &[u8] = include_bytes!("../resources/confluence-mcp-server");

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
        v.push(PathBuf::from(d).join("Library").join("Application Support").join("ConfluenceConnect"));
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    if let Some(d) = std::env::var_os("HOME") {
        v.push(PathBuf::from(d).join(".local").join("share").join("ConfluenceConnect"));
    }
    if v.is_empty() {
        v.push(PathBuf::from("ConfluenceConnect"));
    }
    v
}
```

- [ ] **Step 4: Verify tests pass on Windows (existing tests unchanged)**

```
cargo test -p configurator -- --test-threads=1
```
Expected: all existing installer tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/configurator/src/installer.rs crates/configurator/tests/installer.rs
git commit -m "feat(installer): platform-conditional binary name, install dir, and embedded server"
```

---

## Task 2: Fix claude_config.rs — macOS config path

**Files:**
- Modify: `crates/configurator/src/claude_config.rs`
- Modify: `crates/configurator/tests/claude_config.rs`

### Background

Mac Claude Desktop stores its config at `~/Library/Application Support/Claude/claude_desktop_config.json`. The current non-Windows branch returns `~/.config/Claude/` which is Linux-specific.

- [ ] **Step 1: Write the failing test**

Add to `crates/configurator/tests/claude_config.rs`:

```rust
#[test]
#[cfg(target_os = "macos")]
fn default_config_path_on_mac_uses_library_application_support() {
    let p = configurator::claude_config::default_config_path();
    let s = p.to_string_lossy();
    assert!(
        s.contains("Library/Application Support/Claude"),
        "expected Library/Application Support/Claude in: {s}"
    );
    assert!(s.ends_with("claude_desktop_config.json"));
}
```

- [ ] **Step 2: Verify test fails on Mac**

```
cargo test -p configurator default_config_path_on_mac_uses_library_application_support
```
Expected: FAIL — path contains `.config/Claude` not `Library/Application Support/Claude`

- [ ] **Step 3: Replace `default_config_path` in claude_config.rs**

Replace the `default_config_path` function (currently lines ~13-21) with:

```rust
pub fn default_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        appdata.join("Claude").join("claude_desktop_config.json")
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join("Library").join("Application Support").join("Claude").join("claude_desktop_config.json")
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        home.join(".config").join("Claude").join("claude_desktop_config.json")
    }
}
```

- [ ] **Step 4: Verify tests pass on Windows**

```
cargo test -p configurator -- --test-threads=1
```
Expected: all existing claude_config tests PASS

- [ ] **Step 5: Commit**

```bash
git add crates/configurator/src/claude_config.rs crates/configurator/tests/claude_config.rs
git commit -m "feat(claude_config): use Library/Application Support/Claude path on macOS"
```

---

## Task 3: Update tauri.conf.json — add dmg target and icns icon

**Files:**
- Modify: `crates/configurator/tauri.conf.json`

### Background

Tauri 2 ignores bundle targets not supported by the current platform, so adding `"dmg"` does not break the Windows build. The `"icons/icon.icns"` file does not need to exist on Windows; Tauri only loads the icon matching the current platform. The `.icns` file is generated by the Mac CI job in Task 4.

- [ ] **Step 1: Update tauri.conf.json bundle section**

Replace:
```json
"bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/icon.ico"]
}
```

With:
```json
"bundle": {
    "active": true,
    "targets": ["nsis", "dmg"],
    "icon": ["icons/icon.ico", "icons/icon.icns"]
}
```

- [ ] **Step 2: Verify Windows build still passes**

```powershell
cargo build --release -p server
Copy-Item target/release/confluence-mcp-server.exe crates/configurator/resources/confluence-mcp-server.exe -Force
cargo build --release -p configurator
```
Expected: `target/release/ConfluenceConnect.exe` produced, no errors

- [ ] **Step 3: Commit**

```bash
git add crates/configurator/tauri.conf.json
git commit -m "feat(tauri): add dmg bundle target and icns icon entry for macOS"
```

---

## Task 4: Add Mac CI job to rust-ci.yml

**Files:**
- Modify: `.github/workflows/rust-ci.yml`

### Background

The Mac job builds the server binary first (before clippy) because `include_bytes!("../resources/confluence-mcp-server")` is resolved at compile time — if the file isn't there, compilation fails. The icon.icns is generated here using `sips` and `iconutil`, which are built into every macOS system (no install needed). This job validates compilation and tests on macOS; it does not produce a `.dmg` artifact (that's `release.yml`'s job).

- [ ] **Step 1: Add `build-mac` job to rust-ci.yml**

Append the following job to `.github/workflows/rust-ci.yml` (after the existing `build:` job):

```yaml
  build-mac:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo registry and build
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('Cargo.lock') }}

      - name: Build server binary
        run: cargo build --release -p server

      - name: Copy server binary into configurator resources
        run: |
          mkdir -p crates/configurator/resources
          cp target/release/confluence-mcp-server crates/configurator/resources/confluence-mcp-server

      - name: Generate icon.icns
        run: |
          sips -s format png crates/configurator/icons/icon.ico --out /tmp/app_icon.png
          mkdir /tmp/icon.iconset
          for size in 16 32 128 256 512; do
            sips -z $size $size /tmp/app_icon.png \
              --out /tmp/icon.iconset/icon_${size}x${size}.png
            sips -z $((size * 2)) $((size * 2)) /tmp/app_icon.png \
              --out /tmp/icon.iconset/icon_${size}x${size}@2x.png
          done
          iconutil -c icns /tmp/icon.iconset -o crates/configurator/icons/icon.icns

      - name: cargo fmt --check
        run: cargo fmt --all -- --check

      - name: cargo clippy
        run: cargo clippy --workspace --all-targets -- -D warnings

      - name: cargo test
        run: cargo test --workspace -- --test-threads=1
```

- [ ] **Step 2: Commit and push, verify CI passes**

```bash
git add .github/workflows/rust-ci.yml
git commit -m "ci: add macos build job with server embed and icon generation"
git push
```

Open GitHub → Actions → the new push → confirm `build-mac` job goes green. The new `#[cfg(target_os = "macos")]` tests should run and pass.

---

## Task 5: Create release.yml — tag-triggered release with both artifacts

**Files:**
- Create: `.github/workflows/release.yml`

### Background

`releases/latest/download/` URLs (used by the GitHub Pages download buttons) only work with files uploaded as GitHub Release assets. Actions artifacts expire and require login. This workflow creates a real GitHub Release when a `v*` tag is pushed.

The `dmg` output path from `cargo tauri build` varies by architecture (arm64 vs x86_64). We use `find` to locate it regardless.

- [ ] **Step 1: Create `.github/workflows/release.yml`**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build-windows:
    runs-on: windows-latest
    defaults:
      run:
        shell: pwsh
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('Cargo.lock') }}

      - name: Build distribution
        run: powershell -ExecutionPolicy Bypass -File scripts/build.ps1

      - name: Upload Windows artifact
        uses: actions/upload-artifact@v4
        with:
          name: windows-dist
          path: dist/ConfluenceConnect.exe

  build-mac:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust toolchain
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('Cargo.lock') }}

      - name: Install Tauri CLI
        run: npm install -g @tauri-apps/cli@^2

      - name: Build server binary
        run: cargo build --release -p server

      - name: Copy server binary into configurator resources
        run: |
          mkdir -p crates/configurator/resources
          cp target/release/confluence-mcp-server crates/configurator/resources/confluence-mcp-server

      - name: Generate icon.icns
        run: |
          sips -s format png crates/configurator/icons/icon.ico --out /tmp/app_icon.png
          mkdir /tmp/icon.iconset
          for size in 16 32 128 256 512; do
            sips -z $size $size /tmp/app_icon.png \
              --out /tmp/icon.iconset/icon_${size}x${size}.png
            sips -z $((size * 2)) $((size * 2)) /tmp/app_icon.png \
              --out /tmp/icon.iconset/icon_${size}x${size}@2x.png
          done
          iconutil -c icns /tmp/icon.iconset -o crates/configurator/icons/icon.icns

      - name: Build Mac distribution
        run: |
          cd crates/configurator
          tauri build
          mkdir -p ../../dist
          find ../../target/release/bundle/dmg -name "*.dmg" -maxdepth 1 \
            -exec cp {} ../../dist/ConfluenceConnect.dmg \;

      - name: Upload Mac artifact
        uses: actions/upload-artifact@v4
        with:
          name: mac-dist
          path: dist/ConfluenceConnect.dmg

  publish:
    needs: [build-windows, build-mac]
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/download-artifact@v4
        with:
          name: windows-dist
          path: dist/windows

      - uses: actions/download-artifact@v4
        with:
          name: mac-dist
          path: dist/mac

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            dist/windows/ConfluenceConnect.exe
            dist/mac/ConfluenceConnect.dmg
          generate_release_notes: true
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add release workflow — builds Windows and Mac in parallel, creates GitHub Release on v* tag"
```

- [ ] **Step 3: Test with a tag**

```bash
git tag v0.2.1
git push origin v0.2.1
```

Open GitHub → Actions → Release workflow → confirm both build jobs go green and the `publish` job creates a release with both `ConfluenceConnect.exe` and `ConfluenceConnect.dmg` as assets.

Verify the direct download URL resolves:
`https://github.com/huylq33/confluence-mcp-server/releases/latest/download/ConfluenceConnect.dmg`

---

## Task 6: Update GitHub Pages — Mac download button and requirements

**Files:**
- Modify: `docs/index.html`

### Background

The Pages deploy workflow (`pages.yml`) runs on every push to master and substitutes `{{VERSION}}` from `Cargo.toml`. No workflow changes needed — just update the HTML.

- [ ] **Step 1: Update meta description**

Replace:
```html
<meta name="description" content="Confluence Connect — a free Windows tool that lets Claude read your Confluence pages. No runtime required.">
```
With:
```html
<meta name="description" content="Confluence Connect — a free tool for Windows and Mac that lets Claude read your Confluence pages. No runtime required.">
```

- [ ] **Step 2: Add CSS for side-by-side download buttons**

After the `.smartscreen-note` rule in `<style>`, add:

```css
.download-buttons { display: flex; gap: 16px; justify-content: center; flex-wrap: wrap; margin-bottom: 0; }
.download-btn-secondary {
  display: inline-block;
  background: transparent;
  color: #4fc3f7;
  font-size: 1.1rem;
  font-weight: 700;
  padding: 15px 36px;
  border-radius: 8px;
  text-decoration: none;
  border: 2px solid #4fc3f7;
  transition: background 0.2s, color 0.2s;
}
.download-btn-secondary:hover { background: #4fc3f7; color: #0d1b2a; }
.platform-note { margin-top: 8px; font-size: 0.8rem; color: #5a7a8f; text-align: center; }
```

- [ ] **Step 3: Replace the single download button with two side-by-side buttons**

Replace:
```html
    <a class="download-btn" href="https://github.com/huylq33/confluence-mcp-server/releases/latest/download/ConfluenceConnect.exe" target="_blank" rel="noopener noreferrer">
      ⬇ Download for Windows
    </a>
    <p class="version-badge">v{{VERSION}}</p>
    <p class="smartscreen-note">Windows may show an unknown publisher warning — click <strong>More info → Run anyway</strong></p>
```

With:
```html
    <div class="download-buttons">
      <div>
        <a class="download-btn" href="https://github.com/huylq33/confluence-mcp-server/releases/latest/download/ConfluenceConnect.exe" target="_blank" rel="noopener noreferrer">
          ⬇ Download for Windows
        </a>
        <p class="platform-note">Windows may show an unknown publisher warning —<br>click <strong>More info → Run anyway</strong></p>
      </div>
      <div>
        <a class="download-btn-secondary" href="https://github.com/huylq33/confluence-mcp-server/releases/latest/download/ConfluenceConnect.dmg" target="_blank" rel="noopener noreferrer">
          ⬇ Download for Mac
        </a>
        <p class="platform-note">macOS may block the app on first launch —<br>right-click → <strong>Open</strong>, or System Settings → Privacy &amp; Security → <strong>Open Anyway</strong></p>
      </div>
    </div>
    <p class="version-badge">v{{VERSION}}</p>
```

- [ ] **Step 4: Update Step 1 copy in "How it works"**

Replace:
```html
        <p>Download <code>ConfluenceConnect.exe</code> and run it. No installer, no runtime required.</p>
```
With:
```html
        <p>Download <code>ConfluenceConnect.exe</code> (Windows) or <code>ConfluenceConnect.dmg</code> (Mac) and run it. No installer, no runtime required.</p>
```

- [ ] **Step 5: Update Requirements section**

Replace:
```html
      <li>Windows 10 or later</li>
```
With:
```html
      <li>Windows 10 or later, or macOS 12 (Monterey) or later</li>
```

- [ ] **Step 6: Commit and push**

```bash
git add docs/index.html
git commit -m "feat(pages): add Mac download button, Gatekeeper note, and macOS requirement"
git push
```

Open `https://huylq33.github.io/confluence-mcp-server/` after the Pages deploy workflow completes and verify both buttons appear.

---

## Self-Review Notes

- **Spec coverage:** All three spec sections covered — Rust code (Tasks 1–3), CI/Release (Tasks 4–5), Pages (Task 6). ✓
- **No placeholders:** All steps contain actual code/commands. ✓
- **Type consistency:** `SERVER_BINARY_NAME`, `EMBEDDED_SERVER`, `install_dir_candidates`, `default_config_path` — names match across tasks. ✓
- **Known limitation:** The macOS `#[cfg]` tests in Tasks 1–2 only execute on Mac (Task 4's CI job). They don't run locally on Windows — that's by design, not a gap.
- **Tag for release:** Task 5 uses `v0.2.1` as the example tag — matches current `Cargo.toml` version `0.2.1`. If the version has been bumped before running Task 5, update the tag accordingly.
