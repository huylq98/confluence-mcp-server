# macOS PKG Installer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the macOS DMG distribution with a PKG installer that auto-clears Gatekeeper quarantine, enabling non-technical users to install without Terminal commands.

**Architecture:** Tauri continues to build the `.app` bundle. A new shell script (`scripts/build-mac-pkg.sh`) wraps the `.app` using macOS built-in `pkgbuild` + `productbuild` tools to produce a standard installer package with a `postinstall` script that removes quarantine. CI uploads `ConfluenceConnect.pkg` instead of the DMG. The landing page download link is updated accordingly.

**Tech Stack:** Bash, macOS `pkgbuild`, macOS `productbuild`, GitHub Actions, Tauri 2

---

### Task 1: Create `scripts/build-mac-pkg.sh`

**Files:**
- Create: `scripts/build-mac-pkg.sh`

- [ ] **Step 1: Create the script**

```bash
#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ConfluenceConnect"
APP_SRC="target/release/bundle/macos/${APP_NAME}.app"
STAGING_DIR="$(mktemp -d)"
PKG_SCRIPTS_DIR="$(mktemp -d)"
COMPONENT_PKG="/tmp/${APP_NAME}-component.pkg"
DIST_DIR="dist"
OUTPUT_PKG="${DIST_DIR}/${APP_NAME}.pkg"
VERSION="${GITHUB_REF_NAME:-0.0.0}"
VERSION="${VERSION#v}"   # strip leading 'v' from git tag
IDENTIFIER="io.github.huylq98.confluence-mcp-server"

echo "==> Staging app bundle"
cp -R "${APP_SRC}" "${STAGING_DIR}/"

echo "==> Writing postinstall script"
cat > "${PKG_SCRIPTS_DIR}/postinstall" <<'SCRIPT'
#!/bin/bash
xattr -dr com.apple.quarantine "/Applications/ConfluenceConnect.app" 2>/dev/null || true
exit 0
SCRIPT
chmod +x "${PKG_SCRIPTS_DIR}/postinstall"

echo "==> Building component package"
pkgbuild \
  --root "${STAGING_DIR}" \
  --install-location /Applications \
  --scripts "${PKG_SCRIPTS_DIR}" \
  --identifier "${IDENTIFIER}" \
  --version "${VERSION}" \
  "${COMPONENT_PKG}"

echo "==> Building distribution package"
mkdir -p "${DIST_DIR}"
productbuild \
  --package "${COMPONENT_PKG}" \
  "${OUTPUT_PKG}"

echo "==> Done: ${OUTPUT_PKG}"
```

Save to `scripts/build-mac-pkg.sh`.

- [ ] **Step 2: Make it executable**

```bash
chmod +x scripts/build-mac-pkg.sh
```

- [ ] **Step 3: Verify the script is syntactically valid**

```bash
bash -n scripts/build-mac-pkg.sh
```

Expected: no output (exit 0 means no syntax errors).

- [ ] **Step 4: Commit**

```bash
git add scripts/build-mac-pkg.sh
git commit -m "feat(mac): add PKG installer build script"
```

---

### Task 2: Update `release.yml` — swap DMG for PKG

**Files:**
- Modify: `.github/workflows/release.yml`

- [ ] **Step 1: Replace the Mac build packaging step**

In `.github/workflows/release.yml`, find the `Build Mac distribution` step (currently around line 88):

```yaml
      - name: Build Mac distribution
        run: |
          cd crates/configurator
          tauri build
          mkdir -p ../../dist
          find ../../target/release/bundle/dmg -name "*.dmg" -maxdepth 1 \
            -exec cp {} ../../dist/ConfluenceConnect.dmg \;
          [ -f ../../dist/ConfluenceConnect.dmg ] || { echo "ERROR: no .dmg found in target/release/bundle/dmg"; exit 1; }
```

Replace with:

```yaml
      - name: Build Mac distribution
        run: |
          cd crates/configurator
          tauri build
          cd ../..
          bash scripts/build-mac-pkg.sh
          [ -f dist/ConfluenceConnect.pkg ] || { echo "ERROR: ConfluenceConnect.pkg not found in dist/"; exit 1; }
```

- [ ] **Step 2: Update the Upload Mac artifact step**

Find (around line 99):

```yaml
      - name: Upload Mac artifact
        uses: actions/upload-artifact@v4
        with:
          name: mac-dist
          path: dist/ConfluenceConnect.dmg
```

Replace with:

```yaml
      - name: Upload Mac artifact
        uses: actions/upload-artifact@v4
        with:
          name: mac-dist
          path: dist/ConfluenceConnect.pkg
```

- [ ] **Step 3: Update the publish job — download artifact path**

Find (around line 116):

```yaml
      - uses: actions/download-artifact@v4
        with:
          name: mac-dist
          path: dist/mac
```

No change needed here — artifact name stays `mac-dist`.

- [ ] **Step 4: Update the release asset filename**

Find the `Create GitHub Release` step files list (around line 122):

```yaml
          files: |
            dist/windows/ConfluenceConnect.exe
            dist/mac/ConfluenceConnect.dmg
```

Replace with:

```yaml
          files: |
            dist/windows/ConfluenceConnect.exe
            dist/mac/ConfluenceConnect.pkg
```

- [ ] **Step 5: Verify the full workflow file looks correct**

```bash
cat .github/workflows/release.yml
```

Check that:
- No remaining `.dmg` references exist in the file
- The `build-mac` job calls `bash scripts/build-mac-pkg.sh`
- The upload path is `dist/ConfluenceConnect.pkg`
- The release files list has `ConfluenceConnect.pkg`

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: replace macOS DMG with PKG in release workflow"
```

---

### Task 3: Update landing page — `.dmg` → `.pkg`

**Files:**
- Modify: `docs/index.html`

- [ ] **Step 1: Update the Mac download button href (line 468)**

Find:

```html
        <a class="btn-dl btn-secondary-dl" href="https://github.com/huylq98/confluence-mcp-server/releases/latest/download/ConfluenceConnect.dmg" target="_blank" rel="noopener noreferrer">
```

Replace with:

```html
        <a class="btn-dl btn-secondary-dl" href="https://github.com/huylq98/confluence-mcp-server/releases/latest/download/ConfluenceConnect.pkg" target="_blank" rel="noopener noreferrer">
```

- [ ] **Step 2: Update the install instructions text (line 488)**

Find:

```html
          <p>Download <code>ConfluenceConnect.exe</code> (Windows) or <code>ConfluenceConnect.dmg</code> (Mac) and run it. No installer, no runtime required.</p>
```

Replace with:

```html
          <p>Download <code>ConfluenceConnect.exe</code> (Windows) or <code>ConfluenceConnect.pkg</code> (Mac) and run it. No runtime required.</p>
```

- [ ] **Step 3: Verify no remaining `.dmg` references**

```bash
grep -n "dmg\|DMG" docs/index.html
```

Expected: no output.

- [ ] **Step 4: Commit**

```bash
git add docs/index.html
git commit -m "docs(landing): update macOS download link from DMG to PKG"
```
