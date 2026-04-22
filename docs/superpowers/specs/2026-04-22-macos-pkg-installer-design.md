# macOS PKG Installer — Design Spec

**Date:** 2026-04-22  
**Status:** Approved

## Problem

The current macOS DMG distribution triggers a "damaged and can't be opened" Gatekeeper error for unsigned binaries downloaded from the internet. Non-technical users have no obvious recovery path.

## Goal

Replace `ConfluenceConnect.dmg` with `ConfluenceConnect.pkg` — a standard macOS installer package that:
- Presents a familiar double-click install wizard (no Terminal required)
- Installs the app to `/Applications`
- Automatically clears the quarantine attribute so the app opens without any Gatekeeper warning

## Architecture

### Build flow (unchanged up to `.app`)

Tauri still builds the `.app` bundle. The existing CI step that calls `tauri build` is kept as-is; only the packaging step after it changes.

### New packaging step: `scripts/build-mac-pkg.sh`

A shell script run in CI after the Tauri build produces `ConfluenceConnect.app`. It:

1. Creates a staging directory and copies the `.app` into it.
2. Writes a `postinstall` script:
   ```sh
   #!/bin/bash
   xattr -dr com.apple.quarantine /Applications/ConfluenceConnect.app
   ```
3. Runs `pkgbuild` to produce a component package:
   ```sh
   pkgbuild \
     --root staging/ \
     --install-location /Applications \
     --scripts scripts-dir/ \
     --identifier com.confluence-mcp-server.ConfluenceConnect \
     --version "${GITHUB_REF_NAME#v}" \   # strips leading 'v' from the git tag
     ConfluenceConnect-component.pkg
   ```
4. Runs `productbuild` to wrap into the final distribution package:
   ```sh
   productbuild \
     --package ConfluenceConnect-component.pkg \
     ConfluenceConnect.pkg
   ```
5. Copies `ConfluenceConnect.pkg` to `dist/`.

### CI changes (`release.yml`)

- `build-mac` job: replace the DMG copy step with a call to `scripts/build-mac-pkg.sh`.
- Upload artifact: `dist/ConfluenceConnect.pkg` (was `.dmg`).
- `publish` job: release asset filename changes from `ConfluenceConnect.dmg` to `ConfluenceConnect.pkg`.

### Landing page

The macOS download button href and any visible filename references change from `.dmg` to `.pkg`.

## User experience

1. User downloads `ConfluenceConnect.pkg`
2. macOS may show "cannot verify developer" — user right-clicks → Open, or goes to System Settings → Privacy & Security → Open Anyway (one-time, standard flow — much clearer than "damaged")
3. Standard macOS installer wizard opens — Introduction → Installation → Summary
4. App installs to `/Applications/ConfluenceConnect.app`
5. `postinstall` script runs automatically, clearing quarantine — app opens on first launch with no further warnings

## Files changed

| File | Change |
|------|--------|
| `scripts/build-mac-pkg.sh` | New — orchestrates pkgbuild + productbuild |
| `.github/workflows/release.yml` | Replace DMG step/artifact with PKG |
| Landing page (`gh-pages` branch) | Update download link `.dmg` → `.pkg` |

## Out of scope

- Code signing / notarization (no cert available)
- Windows installer changes
- Custom installer UI (welcome screen, license, background image)
