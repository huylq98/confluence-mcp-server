#Requires -Version 5.1
# End-to-end build pipeline for the Rust rewrite.
# Produces dist/ConfluenceConnect.exe
#
# Usage:
#   ./scripts/build.ps1           # Default: no UPX (friendlier to Windows Defender).
#   ./scripts/build.ps1 -UseUpx   # Opt-in UPX compression (~47% smaller, but
#                                   trips AV heuristics on unsigned builds).

param(
    [switch]$UseUpx = $false
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist"
$Upx  = Join-Path $Root "tools/upx-4.2.4-win64/upx.exe"

if ($UseUpx -and -not (Test-Path $Upx)) {
    Write-Error "UPX not found at $Upx. Download: https://github.com/upx/upx/releases (extract to tools/upx-4.2.4-win64/)"
}

Write-Host "== 1/4 Building server crate (release) =="
Push-Location $Root
cargo build --release -p server
Pop-Location

$ServerBin = Join-Path $Root "target/release/confluence-mcp-server.exe"
if (-not (Test-Path $ServerBin)) { Write-Error "server binary missing at $ServerBin" }

if ($UseUpx) {
    Write-Host "== 1b. UPX-compressing server binary =="
    & $Upx --best $ServerBin
}

Write-Host "== 2/4 Copying server binary into configurator resources =="
$Resources = Join-Path $Root "crates/configurator/resources"
New-Item -ItemType Directory -Force -Path $Resources | Out-Null
Copy-Item $ServerBin (Join-Path $Resources "confluence-mcp-server.exe") -Force

Write-Host "== 3/4 Building configurator crate (release) =="
Push-Location $Root
cargo build --release -p configurator
Pop-Location

$WizardBin = Join-Path $Root "target/release/ConfluenceConnect.exe"
if (-not (Test-Path $WizardBin)) { Write-Error "wizard binary missing at $WizardBin" }

if ($UseUpx) {
    Write-Host "== 3b. UPX-compressing wizard binary =="
    & $Upx --best $WizardBin
}

Write-Host "== 4/4 Copying final exe to dist/ =="
New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Copy-Item $WizardBin (Join-Path $Dist "ConfluenceConnect.exe") -Force

$finalSize = (Get-Item (Join-Path $Dist "ConfluenceConnect.exe")).Length
Write-Host ""
if ($UseUpx) {
    Write-Host ("Final ConfluenceConnect.exe (UPX): {0:N0} bytes ({1:N2} MB)" -f $finalSize, ($finalSize / 1MB))
} else {
    Write-Host ("Final ConfluenceConnect.exe: {0:N0} bytes ({1:N2} MB)" -f $finalSize, ($finalSize / 1MB))
}
Write-Host "Output: $Dist\ConfluenceConnect.exe"
