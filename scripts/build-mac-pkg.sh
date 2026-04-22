#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ConfluenceConnect"
APP_SRC="target/release/bundle/macos/${APP_NAME}.app"
STAGING_DIR="$(mktemp -d)"
PKG_SCRIPTS_DIR="$(mktemp -d)"
COMPONENT_PKG="${STAGING_DIR}/${APP_NAME}-component.pkg"
DIST_DIR="dist"
OUTPUT_PKG="${DIST_DIR}/${APP_NAME}.pkg"
VERSION="${GITHUB_REF_NAME:-0.0.0}"
VERSION="${VERSION#v}"   # strip leading 'v' from git tag
IDENTIFIER="io.github.huylq98.confluence-mcp-server"

trap 'rm -rf "${STAGING_DIR}" "${PKG_SCRIPTS_DIR}"' EXIT

[ -d "${APP_SRC}" ] || { echo "ERROR: .app not found at ${APP_SRC}"; exit 1; }

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
  --ownership recommended \
  "${COMPONENT_PKG}"

echo "==> Building distribution package"
mkdir -p "${DIST_DIR}"
productbuild \
  --package "${COMPONENT_PKG}" \
  "${OUTPUT_PKG}"

echo "==> Done: ${OUTPUT_PKG}"
