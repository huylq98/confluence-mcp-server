# v0.2 → v0.4 Roadmap — Design

**Date:** 2026-04-17
**Status:** Design — pending user approval
**Author:** Brainstorming session (Tech Lead + Claude Code)

## Problem

Confluence Connect is at v0.1.4: 7 read-only MCP tools, a Tauri 2 wizard with Setup + Monitor tabs, ~2.8 MB single-exe Windows distribution, Windows-only. The core works. The gaps are on the edges:

- **First-run confusion.** Non-technical teammates don't know what a Personal Access Token is, where to get one, or whether their URL is right. Errors are accurate but terse.
- **Monitor tab feels thin.** PID and memory only. No signal for "is it actually talking to Confluence?", no usage data, no recent-error view.
- **Updates are manual.** Teammates drift onto old versions with no nudge.
- **GitHub distribution is a barrier.** Many target teammates aren't familiar with GitHub's Releases UI. They need a one-button download page.

## Non-Goals (Deferred)

Constraints from existing project memory: no paid signing certificate, target users have no dev tools installed, company-specific tools live in a private repo.

- macOS build (Gatekeeper warning on every launch for unsigned binaries — poor UX).
- Windows code-signing / Authenticode EV certs — unavailable to this project.
- App Store / Microsoft Store publishing — requires verified publisher.
- Write-action MCP tools (page create/update, comment, attachment upload). Candidate for a later roadmap; keeps blast radius small while UX work happens.
- HTTP transport for other MCP clients (Cursor, Cline, VS Code). Tracked as a future option.
- Any in-app mechanism that sends user data to external APIs by default. The LLM-powered analyzer (v0.4 candidate) is opt-in BYOK only.

## Target Outcome

A teammate clicks a link on an internal Confluence page (or the GitHub Pages landing), downloads `ConfluenceConnect.exe`, finishes Setup in under 60 seconds, and every future release surfaces as a one-click banner inside the wizard. The Monitor tab tells them how much Confluence content they're pulling into Claude's context and nudges them toward better queries when patterns suggest it.

## Sequencing — Phased, UX First

Three releases. Each is independently valuable; each informs the next.

| Release | Theme | Ships |
|---|---|---|
| **v0.2** | Wizard clarity + Monitor depth | PAT deep-link, URL validation, post-save guidance, token-usage chart, rule-based analyzer, recent-errors drawer, Test live button |
| **v0.3** | Distribution + auto-updater | `huylq98.github.io/confluence-mcp-server` landing page, update banner inside the wizard, CI tag-to-release automation |
| **v0.4** | Polish | Post-feedback items; LLM-powered analyzer (tier C) candidate; possible Vietnamese copy |

## v0.2 Design

### Wizard clarity (Setup tab)

Four concrete changes in `crates/configurator/ui/index.html` + `app.js`:

1. **"Get your token →" deep link.** Next to the PAT input, show a small link computed from the Confluence URL field. Formula: `{url}/plugins/personalaccesstokens/usertokens.action`. Clicking opens the user's own PAT settings page via Tauri's `shell.open`. Grayed out until the URL field contains a valid URL.
2. **URL validation badge.** Inline badge next to the URL field: ✓ HTTPS / ⚠ HTTP / ✗ malformed. Optional enhancement: a lightweight unauthenticated ping to `/rest/api/space` that confirms the endpoint responds with 401 (proves it's Confluence, not a typo).
3. **Post-save "You're set!" panel.** After successful Save, replace the auto-jump-to-Monitor with a final card:
   - (i) *Fully quit Claude Desktop from the tray icon.*
   - (ii) *Reopen Claude Desktop.*
   - (iii) *Try one of these:* three copyable example prompts — "Find pages about onboarding", a paste-a-URL example, "List spaces I can access".
   A Continue button advances to Monitor.
4. **Smarter proxy/network error hints.** Extend the existing pattern in `crates/configurator/src/commands.rs:format_error_chain` to cover: "connection timed out with proxy set" (suggest clearing Proxy field if the Confluence host is internal), "CA bundle invalid" (show path and first-line parse error).

### Copy fix (drive-by)

Replace "wiki" with "Confluence" in user-facing strings. Specifically:
- `crates/configurator/ui/index.html` line 57 hero copy
- Any hint text that says "wiki"

Reason: teammates don't recognize "wiki" as a category word; "Confluence" is specific and unambiguous.

### Monitor depth (Monitor tab)

**Test live connection** button next to the status header. Runs the same `list_spaces(limit=5)` the Setup tab's Test Connection already runs, but against the running config. Reports latency + space count. Proves the full path (server + network + Confluence + auth) works at the moment of click.

**Today tile.** Three counters: calls / tokens / errors. Computed by reading `history.jsonl` and `errors.jsonl` and filtering to today.

**Token usage · last 7 days** column chart. One bar per day, rendered as inline SVG (no chart library). Caption: *"Confluence content returned to Claude, estimated from character count (chars ÷ 4)."* Token estimate is intentionally approximate — good enough for trend signal, zero tokenizer dependency.

**Recent errors drawer.** Collapsible `<details>` showing the last entries from `errors.jsonl`. Each entry: timestamp, tool name, status code, first line of message.

**Rule-based analyzer sidebar (tier B of the three we considered).** A compact sidebar below the chart. Reads `history.jsonl`, runs pure-function heuristics, shows 0–3 actionable tips. Initial rules:

- **Repeated page fetch.** Same `page_id` requested ≥5 times in 7 days → "Consider pinning page X in your Claude prompt instead of re-fetching it."
- **Oversized output.** Any tool call produced >20k estimated tokens → "Narrow your CQL (add `space=...`), or call `get_page` with `include_body=false` first to preview."
- **High error rate.** Errors/calls > 10% in the last 7 days → "Check the Recent errors list — the same endpoint keeps failing."
- **Frequent 403s.** ≥3 403s on the same page_id → "That page is restricted; ask your Confluence admin for access or use a different page."

Plus a **Copy diagnostics for Claude** button: serializes the last ~100 history entries + recent errors to a markdown block and copies to clipboard. User pastes into their existing Claude Desktop session ("analyze this") for deeper on-demand analysis — zero new LLM plumbing, privacy-preserving (user chooses to paste), leverages the Claude Desktop the user already has running.

**Claude Desktop log shortcut.** A small link below the action buttons: "Open Claude Desktop log →". Opens `%APPDATA%\Claude\logs\` in Explorer via `shell.open`. Drop-in aid for deeper debugging.

### v0.2 data plane

Two new files written to the install directory (alongside `confluence-mcp-server.exe`):

- **`history.jsonl`** — append-only. One line per tool call: `{ts, tool, out_chars, tokens_est, status}`. Server truncates to the last 1000 lines on startup and once every 100 writes (read-tail + rewrite; file is small, operation is fast).
- **`errors.jsonl`** — append-only. One line per error: `{ts, tool, status, message}`. Truncated to the last 20 entries on the same cadence.

Both files are read-only from the configurator side (uses `fs::read_to_string` + line-by-line JSON parse). Writes happen only in the server. No IPC, no locking primitives — short append-writes on a local filesystem are effectively atomic for a single writer.

### v0.2 code organization

- **Server side.** A tiny `record(...)` helper in `crates/server/src/handler.rs` wraps each existing tool call site. Measures the formatted output length, writes one line to each file on error/success. Estimated diff: ~30 added lines in `handler.rs`, one new ~80-line `recorder.rs` module in the server crate. No change to `confluence-core`.
- **Configurator side.** Two new modules:
  - `stats.rs` — reads the two JSONL files, buckets by day, exposes counters + daily token totals to the UI via Tauri commands.
  - `analyzer.rs` — pure functions over a parsed history slice; returns a `Vec<Tip>` for the UI to render. Easy to unit-test.
  - `commands.rs` gets new commands: `get_stats()`, `get_recommendations()`, `copy_diagnostics()`. This prevents `commands.rs` (already 440 lines) from widening further — the new logic lives in its own modules and `commands.rs` just exposes thin Tauri wrappers.
- **UI.** New markup in `crates/configurator/ui/index.html` for the Today tile, chart, analyzer sidebar, errors drawer; new rendering functions in `app.js`. SVG generated client-side from JSON returned by `get_stats`.

## v0.3 Design

### Landing page

Static HTML/CSS in `docs/` on the `main` branch. GitHub Pages serves it at `huylq98.github.io/confluence-mcp-server`. No Jekyll, no build step — just commit HTML + CSS.

Page structure:

- **Hero.** Tagline: *"Connect Claude Desktop to Confluence."* Subtitle: one-sentence description. Big primary button: **↓ Download for Windows** → `https://github.com/huylq98/confluence-mcp-server/releases/latest/download/ConfluenceConnect.exe`. Version + size label above the button. Subtle disclaimer: "Unsigned build — SmartScreen may warn" with a link to the FAQ answer.
- **Three-step quickstart.** Download → Enter your wiki URL + PAT → Restart Claude Desktop. One screenshot of the Setup tab below.
- **FAQ.** Four `<details>` entries: SmartScreen safety, where to get a PAT, corporate proxy, how to uninstall.
- **Footer.** License, GitHub link.

Aesthetic follows the existing wizard's "Precise Editorial" feel — Fraunces for display, IBM Plex Sans for body, muted warm palette.

### Auto-updater

Wizard-side only. Single HTTP call on each wizard launch:

```
GET https://api.github.com/repos/huylq98/confluence-mcp-server/releases/latest
User-Agent: ConfluenceConnect/{CARGO_PKG_VERSION}
Timeout: 3s
```

Behavior:

- On network failure, rate-limit, or non-200 response: silently proceed. Never block UI.
- On success: parse `tag_name` (`vX.Y.Z`), strip leading `v`, compare to `env!("CARGO_PKG_VERSION")` with the `semver` crate.
- If newer: render a slim amber banner at the top of both Setup and Monitor tabs: *"v0.3.1 is available. You're on v0.3.0."* with two links: **Download →** (opens `https://huylq98.github.io/confluence-mcp-server/` in the default browser) and **Dismiss** (hides banner for the session; re-appears next launch).
- Server binary does NOT check for updates. Only the wizard does. The server runs silently under Claude Desktop; updates surface where the user can see them.

No in-app download-and-replace. Reasons: avoids the "running exe replacing itself on Windows" complexity, keeps the updater's attack surface at literally one JSON GET, matches user expectation that "installers come from a download page, not from the app".

### CI automation

Extend the existing `.github/workflows/` setup (or add a new workflow):

- On `push` of a tag matching `v*.*.*`: run the existing `scripts/build.ps1` on a `windows-latest` runner, then `gh release create {tag} dist/ConfluenceConnect.exe --notes "..."`.
- The landing page's Download button URL (`releases/latest/download/...`) resolves automatically to whatever release is marked "latest" — no page edit per release.

### v0.3 code organization

- New module `crates/configurator/src/updater.rs` — pure function `check_for_update(current: Version) -> Option<UpdateInfo>` plus a Tauri command wrapper. Easy to unit-test with `wiremock`.
- Banner markup in `index.html` (hidden by default); shown by `app.js` when the Tauri command returns `Some`.
- `Cargo.toml` adds the `semver` crate (1 dep, ~15 KB in the release binary — acceptable given the distribution-size constraint).

## v0.4 Polish (Placeholder)

Intentionally unscoped at design time. Post-v0.3 release, collect concrete friction points from teammate usage and prioritize 3–5 items. Likely candidates:

- Additional analyzer rules once real `history.jsonl` patterns are visible.
- **Tier-C LLM-powered analyzer** becomes a real candidate if the v0.2 rule-based tips prove too shallow. Design outline if pursued: BYOK Anthropic API key stored via Tauri's secure store; periodic summarization of `history.jsonl` + recent errors; rendered narrative advice in the same sidebar slot; explicit opt-in toggle with "data leaves this machine" warning. Its own full-release cycle.
- Vietnamese UI strings — low effort, high relevance for Viettel teammates.
- Any cut items from v0.2 or v0.3 that earned their way back in.

## Testing

### Unit tests

- `analyzer.rs` — synthetic `history.jsonl` inputs, assert expected `Tip` outputs (for each rule and for the none-triggered case).
- `stats.rs` — day-bucketing logic, handling of empty/malformed lines, ring-buffer truncation.
- `updater.rs` — version compare (including malformed tag strings, prerelease suffixes, equal versions).
- Recorder (server) — write + read round-trip, truncation after 1000 lines, concurrent-appends via `tokio::spawn` don't corrupt the file.

### Integration tests

- `wiremock`-backed test for the GitHub Releases API: 200 with newer version, 200 with same version, 404, 403 (rate-limit), network timeout. Asserts updater behavior (None vs Some, silent failure).
- End-to-end recorder test: start a mocked MCP server, invoke each tool, assert `history.jsonl` and `errors.jsonl` contents.

### Manual smoke

- Wizard renders update banner when a fixture with `tag_name: v99.0.0` is injected.
- Monitor tab renders empty chart gracefully on a fresh install (no `history.jsonl` yet).
- Copy diagnostics produces readable markdown; paste into Claude Desktop works end-to-end.
- GitHub Pages site renders in Chrome, Edge, and Safari (via a remote colleague) — no Jekyll/layout surprises.

Follows existing project convention: Rust unit + integration tests are expected to pass in CI; UI-level testing stays manual — consistent with the current approach and with the CLAUDE.md note that the distribution must work standalone.

## Out-of-Scope / Explicitly Deferred

| Item | Why deferred | Possible home |
|---|---|---|
| macOS build | Unsigned Gatekeeper warning is worse UX than Windows SmartScreen; notarization requires $99/yr Apple Developer | Never, unless the cert constraint changes |
| Write tools (create/update page, comment) | Increases blast radius; wants auth-scope review and per-tool confirmation UX | Future roadmap after v0.4 |
| HTTP transport mode | Unclear demand; Claude Desktop is the shipped consumer | Future roadmap |
| LLM-powered analyzer (tier C) | BYOK + API spend + corporate data policy review | v0.4 candidate |
| Multi-language UI | Low-effort but not the critical path now | v0.4 candidate |
| In-app update download & replace | Windows self-replace pain; scope creep | Revisit only if manual-download step proves to be a real friction point |

## Risks and Mitigations

- **GitHub API rate limit on updater.** Unauthenticated calls are 60/hour per IP — plenty for one call per wizard launch, but note that shared corporate NAT could hit the limit. Mitigation: silent-fail is already in the design; no user-visible impact.
- **`history.jsonl` write on hot path.** Every tool call writes a short line. On slow disks or network-mounted home dirs this could add noticeable latency. Mitigation: the write is `fs::OpenOptions::append` + small line; should stay well under 1 ms on local NTFS. If benchmarks show otherwise, switch to an in-memory channel + batched writer.
- **Stale analyzer advice.** Heuristics that made sense at v0.2 may misfire in unusual usage patterns. Mitigation: tips are informational, never blocking; easy to tune/remove in v0.4.
- **Landing page drift from app behavior.** FAQ text can go stale if the app changes. Mitigation: keep the landing page short — big changes go in release notes, not the page body.

## Open Questions

1. **GitHub account for release artifacts.** The landing page lives at `huylq98.github.io/confluence-mcp-server` (user-confirmed). Current git commits are authored by `huylq33`. Does the repo itself move to the `huylq98` account (so the landing page and the releases API share one owner), or does the landing page stay on `huylq98` while releases are pulled from `huylq33`? Decision affects the updater's API URL and the landing page's Download button URL. Recommendation: put both under `huylq98` for consistency — simpler mental model, one GitHub account to manage.

2. Otherwise no blockers. User confirmed: phased sequencing (A), download host = GitHub Pages, analyzer tier = B, hero copy = "Connect Claude Desktop to Confluence", single column chart only, no tool-calls chart, no avg latency metric, banner → external download (no in-app replace).
