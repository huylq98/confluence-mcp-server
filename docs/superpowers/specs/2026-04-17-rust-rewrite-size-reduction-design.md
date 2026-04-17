# Rust Rewrite for Distribution Size Reduction — Design

**Date:** 2026-04-17
**Status:** Design — pending user approval
**Author:** Brainstorming session (Tech Lead + Claude Code)

## Problem

The Windows distribution (`ConfluenceMCPSetup.exe`) is ~27 MB after UPX (~33 MB without). Primary goal: ship the smallest possible, usable, nice-looking distribution. Hard constraint from `CLAUDE.md`: the tester environment has **nothing** installed — no Python, no runtimes. Whatever ships must be fully self-contained.

The current exe is one PyInstaller onefile binary that serves two roles: (a) a desktop GUI wizard for non-technical users, and (b) an MCP stdio server that Claude Desktop launches on every boot. Both roles share the same ~25 MB Python+MCP+pydantic+pywebview bundle.

## Non-Goals

- Backwards compatibility with existing Python installs. Users re-run the new wizard after update; no migration path needed.
- macOS support. Dropped to simplify CI and avoid Apple Developer ID signing costs.
- HTTP transport mode (`MCP_TRANSPORT=http`). The shipped distribution only ever runs in Claude Desktop / stdio context; removing HTTP mode saves dependencies and code.
- Auto-updater. Manual "download and re-run wizard" is simpler and saves 1–2 MB of updater framework.

## Target Outcome

- Single download: `ConfluenceMCPSetup.exe` at **~6–9 MB**.
- Runtime footprint on every Claude Desktop boot: **~2–5 MB** MCP server binary (GUI runtime never loaded again after first-run).
- Same 7 Confluence tools, same environment variable contract, same wizard UX.

## Architecture

Two Rust binaries, one user-facing download:

- **`ConfluenceMCPSetup.exe`** — Tauri 2 desktop app. HTML/CSS/JS UI rendered via Windows' built-in WebView2 (no bundled runtime). The only thing the user downloads and double-clicks.
- **`confluence-mcp-server.exe`** — lean Rust MCP stdio server built on `rmcp` + `reqwest` + `tokio`. Never shown to the user.

The server binary is embedded inside the wizard at compile time as a byte blob (via `include_bytes!`). On Save, the wizard extracts it to disk and writes that path into `claude_desktop_config.json`. After the first run, Claude Desktop launches only the small server exe — the wizard and webview code never run again until the user re-runs the installer.

**Why two crates instead of one dual-mode binary:** keeps the server's dependency closure free of GUI code, so the server stays ~2–5 MB regardless of how the wizard grows. The wizard can also be updated independently later without touching the server that Claude Desktop talks to.

### Save Location Handling

Target users may sit behind locked-down corporate environments where even per-user paths can be restricted.

- Default path (pre-filled): `%LOCALAPPDATA%\ConfluenceMCP\`.
- Fallback chain: if `%LOCALAPPDATA%` isn't writable, default to `%USERPROFILE%\ConfluenceMCP\` before forcing the user to pick manually.
- "Change…" button next to the path field opens a native folder picker (Tauri `dialog::open`).
- **Writability probe** before extraction: wizard writes and deletes a `.probe` file in the chosen folder. Failure shows an inline error and keeps Save disabled.
- Persistence: the chosen path is recovered from `claude_desktop_config.json`'s `command` field on re-run; no separate settings file.

## Components — Cargo Workspace

```
confluence-mcp-rust/
├── Cargo.toml                   # workspace root
├── scripts/
│   └── build.ps1                # build orchestrator (see Build Pipeline)
├── crates/
│   ├── server/                  # MCP server binary
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs          # rmcp stdio wiring, config from env
│   │       ├── tools.rs         # 7 tool handlers
│   │       ├── url_parse.rs     # port of _parse_confluence_url (8 URL formats)
│   │       └── format.rs        # HTML strip, truncate, page formatting
│   ├── configurator/            # Tauri wizard binary
│   │   ├── Cargo.toml
│   │   ├── tauri.conf.json
│   │   ├── resources/
│   │   │   └── confluence-mcp-server.exe  # copied here by build.ps1 before build
│   │   ├── src/
│   │   │   ├── main.rs          # Tauri app setup, command registrations
│   │   │   ├── commands.rs      # #[tauri::command] fns
│   │   │   ├── installer.rs     # embeds server via include_bytes!, extracts to disk, writability probe
│   │   │   └── claude_config.rs # read/modify/write claude_desktop_config.json
│   │   └── ui/                  # reworked HTML/CSS/JS frontend
│   └── confluence-core/         # shared library
│       ├── Cargo.toml
│       └── src/
│           ├── client.rs        # HTTP client, auth, retry/rate-limit
│           ├── config.rs        # Config struct + env var parsing
│           └── error.rs         # ConfluenceError enum
```

**Separations of concern:**

- `confluence-core` is a `lib` crate used by both binaries. The server calls it for tool execution; the wizard calls it for the "Test Connection" probe. No duplication.
- `server/` has **zero GUI dependencies** — only `rmcp`, `reqwest`, `tokio`, `serde`, `confluence-core`.
- `configurator/` depends on Tauri + `confluence-core`. The server binary is copied into `crates/configurator/resources/` by `scripts/build.ps1` **before** the wizard is compiled; `installer.rs` then pulls it in at compile time via `include_bytes!("../resources/confluence-mcp-server.exe")`.
- Frontend is **reworked**, not a literal port of the existing `configurator/assets/`. The JS bridge changes from `window.pywebview.api.*` to `window.__TAURI__.core.invoke('...')`.

### Dependency Justification

**Server crate:**
- `rmcp` — official Rust MCP SDK (`modelcontextprotocol/rust-sdk`). 3.3k stars, actively maintained.
- `reqwest` (with default-tls off, `rustls-tls` on) — HTTP client with async support. `rustls` keeps the binary smaller than OpenSSL bindings.
- `tokio` with minimal feature flags (`rt`, `macros`, `io-std`, `net`) — async runtime.
- `serde` + `serde_json` — JSON (de)serialization.
- `tracing` + `tracing-subscriber` — structured logging to stderr.

**Configurator crate:**
- `tauri` 2.x — desktop GUI framework. Uses system WebView2 on Windows — no bundled runtime.
- `tauri-plugin-dialog` — native folder picker.
- `confluence-core` — for Test Connection.
- `serde_json` — reading/writing `claude_desktop_config.json`.

## Data Flow

### First Run (Install)

1. User downloads `ConfluenceMCPSetup.exe` → double-clicks.
2. Tauri window opens, loads wizard UI from Tauri's bundled assets.
3. UI calls `load_existing_config` Tauri command; Rust reads `claude_desktop_config.json` and pre-fills URL, creds, and install path if already configured. Otherwise shows default install path.
4. User enters Confluence URL + token (or username/password), clicks **Test Connection**.
5. JS calls `test_connection` command → Rust uses `confluence-core::Client` → calls Confluence `/rest/api/space` → returns success + space count or a specific error message.
6. User reviews path, optionally clicks **Change…** (native folder picker).
7. User clicks **Save**. Rust side:
   1. Probes chosen path for writability (writes a `.probe` file, deletes it). Failure → inline error, don't proceed.
   2. Extracts embedded server bytes to `{path}/confluence-mcp-server.exe`.
   3. Reads existing `claude_desktop_config.json`; backs it up to `.json.backup`.
   4. Adds/replaces the `mcpServers.confluence` entry with `{command: extracted_path, args: [], env: {CONFLUENCE_URL, CONFLUENCE_TOKEN or CONFLUENCE_USERNAME+CONFLUENCE_PASSWORD, CONFLUENCE_SSL_VERIFY?}}`.
   5. Writes atomically (write to `.tmp`, rename).
8. Success screen: "Restart Claude Desktop to activate." Optional button launches Claude Desktop.

### Steady State (Every Claude Desktop Boot)

1. Claude Desktop reads `claude_desktop_config.json`.
2. Launches `confluence-mcp-server.exe` with env vars from the config.
3. Server stdio-handshakes with Claude Desktop via `rmcp`.
4. Server registers the 7 tools, accepts tool calls, returns formatted markdown.
5. No wizard, no webview, no GUI code loaded.

### Re-Running the Wizard (Reconfigure)

1. User double-clicks `ConfluenceMCPSetup.exe` again.
2. Step 3 of first run reads URL, creds, and install path from existing config.
3. Saving overwrites the server binary at the same path (simple upgrade path) and updates the config.

### Uninstall

Wizard includes a **Remove** action in an overflow menu that:
- Removes the `mcpServers.confluence` entry from `claude_desktop_config.json`.
- Deletes the extracted `confluence-mcp-server.exe` and its folder.
- Prompts the user to manually delete `ConfluenceMCPSetup.exe` (can't self-delete reliably on Windows while running).

## Error Handling

### Wizard (Setup)

| Scenario | Behavior |
|---|---|
| Default path not writable | Warning shown, forces folder picker before enabling Save. |
| Chosen path writability probe fails | Inline error next to path field; Save disabled. |
| Test Connection — 401 | "Authentication failed — check your token or username/password." |
| Test Connection — no route / VPN off | "Cannot reach the server. Check the URL and your network/VPN." |
| Test Connection — SSL cert error | "SSL certificate error. If your company uses a self-signed cert, try unchecking 'Verify SSL Certificate'." |
| Test Connection — unexpected error | Show the error string verbatim; don't swallow. |
| Save — existing `claude_desktop_config.json` is malformed JSON | Back it up as `.json.malformed.YYYYMMDD-HHMM`, start fresh. Log to stderr. |
| Save — config write fails mid-flight | Restore `.json.backup` before reporting error. |
| WebView2 missing (older Win10) | Message box with download link (port of existing `_check_webview2` logic in `configurator/app.py`). |
| Antivirus blocks server extraction | Catch IO error, show path + "your antivirus may be blocking this — add an exception or choose a different folder." |

### Server (Runtime)

| Scenario | Behavior |
|---|---|
| Missing `CONFLUENCE_URL` or auth env vars | Log to stderr, exit 1. Claude Desktop shows the server as failed; user re-runs wizard. |
| HTTP 429 rate limit | Retry with exponential backoff (port of `confluence_client.py` logic). |
| HTTP 503 | Same retry pattern. |
| HTTP 4xx (other) | Return formatted error string to the LLM via tool result (no crash). |
| Network timeout | Return error string, don't crash. |
| Confluence returns HTML error page instead of JSON | Detect + return a clear tool-result error, don't panic on parse. |

**Hard rule preserved from Python:** never log to stdout in stdio mode — every log goes to stderr. `rmcp` handles stdio framing; all `tracing` output goes to `stderr` via `tracing_subscriber::fmt().with_writer(std::io::stderr)`.

## Testing

### Rust Unit + Integration

| Crate | What's tested | Tooling |
|---|---|---|
| `confluence-core` | URL parser (all 8 formats from `_parse_confluence_url`), HTML stripper, config env-var loading + validation | Pure functions, table-driven tests. |
| `confluence-core::client` | Auth header (Bearer vs Basic), retry on 429/503, rate-limit gating, error mapping | `wiremock` crate (Rust equivalent of Python's `respx`). |
| `server::tools` | Each of the 7 tool handlers — input parsing, formatted-markdown output, error-path strings | `wiremock` for the underlying client; tools tested as async fns without going through `rmcp` stdio. |
| `configurator::claude_config` | Read existing config / preserve other `mcpServers` entries / atomic write / backup creation / malformed JSON recovery | `tempfile` for isolated FS sandboxes. |
| `configurator::installer` | Path resolution, writability probe, fallback chain | `tempfile` + permission manipulation. |
| `configurator::commands` | Tauri command handlers tested as plain async fns (the `#[tauri::command]` macro doesn't change the signature) | Same as above. |

### Not Auto-Tested

- Tauri webview / JS frontend. One window with a handful of fields — manual smoke test pre-release. Adding browser-driver tests is more maintenance burden than bug-prevention value at this scope.

### Parity Check During Port

For deterministic modules (URL parser, HTML stripper, formatted-markdown output for canned API responses), copy the existing Python test fixtures verbatim into `tests/fixtures/` and assert the Rust output bytes match. Catches subtle behavior drift.

### CI (GitHub Actions)

- Single job: `windows-latest`.
- Steps: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --workspace`, build release binaries via `scripts/build.ps1`.
- **Size-gate:** fail the job if `ConfluenceMCPSetup.exe` > 12 MB or `confluence-mcp-server.exe` > 6 MB. Ceilings adjusted after first real measurement; the gate prevents silent dep bloat.

## Build Pipeline

Single PowerShell script orchestrates the ordered build:

```powershell
# scripts/build.ps1
1. cargo build --release -p server              # produces confluence-mcp-server.exe
2. strip + UPX the server binary                # reduces to ~2-5 MB
3. copy to crates/configurator/resources/       # where include_bytes! reads from
4. cargo build --release -p configurator        # wizard picks up embedded server
5. strip + UPX the wizard binary                # reduces to ~6-9 MB
6. copy to dist/ConfluenceMCPSetup.exe
```

### Release Profile (Cargo.toml)

```toml
[profile.release]
opt-level = "z"        # optimize for size, not speed
lto = "fat"            # whole-program link-time optimization
codegen-units = 1      # better optimization at cost of compile time
strip = "symbols"      # remove debug symbols
panic = "abort"        # no unwind tables; smaller code
```

UPX is already downloaded to `tools/upx-4.2.4-win64/` in the current Python codebase; the script reuses it.

## Migration / Cutover from Python

Strategy: **no parallel-shipping period, direct cutover.**

- `rust-port` feature branch: Cargo workspace under `rust/` at repo root, leaving existing Python files untouched during development.
- Port tools in order of complexity (simplest first, each tool's Python tests translated as parity check):
  1. `list_spaces` — validates whole plumbing end-to-end.
  2. `search_confluence` — CQL pass-through.
  3. `get_page` — body format + HTML strip + truncation.
  4. `get_page_by_title` — reuses `get_page` formatting.
  5. `get_page_by_url` — depends on `url_parse` module.
  6. `get_comments` — child-content pattern.
  7. `get_attachments` — same pattern, closes parity.
- When Rust reaches parity: single PR that:
  1. Deletes Python code (`server.py`, `main.py`, `config.py`, `confluence_client.py`, `configurator/`, `tests/`, `build.py`).
  2. Moves `rust/` contents to repo root.
  3. Updates `README.md`, `CLAUDE.md`, `.gitignore`, and `requirements.txt` removal.

**Environment variable contract preserved exactly:** `CONFLUENCE_URL`, `CONFLUENCE_TOKEN`, `CONFLUENCE_USERNAME`, `CONFLUENCE_PASSWORD`, `CONFLUENCE_SSL_VERIFY`, `CONFLUENCE_CA_BUNDLE`, `CONFLUENCE_TIMEOUT`, `CONFLUENCE_RATE_LIMIT`, `MAX_CONTENT_LENGTH`, `DEFAULT_SEARCH_LIMIT`.

**Rollback:** git history only — no version tag. User has declared old users are not a concern.

## Distribution & Packaging

- **Output:** single `ConfluenceMCPSetup.exe`, ~6–9 MB after UPX.
- **No installer** (no MSI, NSIS, Inno Setup, or 7z SFX). User saves the exe anywhere and double-clicks.
- **No code signing.** Windows SmartScreen may warn on first download; users click "More info → Run anyway." Can be added later with an EV cert if the warning becomes a problem.
- **No auto-updater.** Manual download-and-re-run.
- **Release channel:** GitHub Releases, triggered by git tag `v*`. CI builds and attaches the exe.

## Size Targets (First Release)

| Artifact | Target | Ceiling (CI gate) |
|---|---|---|
| `confluence-mcp-server.exe` | ~2–5 MB | 6 MB |
| `ConfluenceMCPSetup.exe` | ~6–9 MB | 12 MB |

Both measured after strip + UPX. First build will establish real numbers; targets adjusted downward if achievable without heroics.

## Open Items (Out of Scope for This Spec)

- Future: evaluate `ureq` (blocking, smaller than `reqwest`) if size budget needs further squeeze after first release.
- Future: evaluate 7z SFX wrapper if every MB matters post-release.
- Future: Linux support — not requested. Trivial to add later given Tauri cross-platform nature.
