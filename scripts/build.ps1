#Requires -Version 5.1
# End-to-end build pipeline for the Rust rewrite.
# Produces dist/ConfluenceMCPSetup.exe

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$Dist = Join-Path $Root "dist"
$Upx  = Join-Path $Root "tools/upx-4.2.4-win64/upx.exe"

if (-not (Test-Path $Upx)) {
    Write-Error "UPX not found at $Upx. Download: https://github.com/upx/upx/releases (extract to tools/upx-4.2.4-win64/)"
}

Write-Host "== 1/5 Building server crate (release) =="
Push-Location $Root
cargo build --release -p server
Pop-Location

$ServerBin = Join-Path $Root "target/release/confluence-mcp-server.exe"
if (-not (Test-Path $ServerBin)) { Write-Error "server binary missing at $ServerBin" }

Write-Host "== 2/5 UPX-compressing server binary =="
& $Upx --best $ServerBin

Write-Host "== 3/5 Copying server binary into configurator resources =="
$Resources = Join-Path $Root "crates/configurator/resources"
New-Item -ItemType Directory -Force -Path $Resources | Out-Null
Copy-Item $ServerBin (Join-Path $Resources "confluence-mcp-server.exe") -Force

Write-Host "== 4/5 Building configurator crate (release) =="
Push-Location $Root
cargo build --release -p configurator
Pop-Location

$WizardBin = Join-Path $Root "target/release/ConfluenceMCPSetup.exe"
if (-not (Test-Path $WizardBin)) { Write-Error "wizard binary missing at $WizardBin" }

Write-Host "== 5/5 UPX-compressing wizard binary =="
& $Upx --best $WizardBin

New-Item -ItemType Directory -Force -Path $Dist | Out-Null
Copy-Item $WizardBin (Join-Path $Dist "ConfluenceMCPSetup.exe") -Force

$finalSize = (Get-Item (Join-Path $Dist "ConfluenceMCPSetup.exe")).Length
Write-Host ""
Write-Host ("Final ConfluenceMCPSetup.exe: {0:N0} bytes ({1:N2} MB)" -f $finalSize, ($finalSize / 1MB))
Write-Host "Output: $Dist\ConfluenceMCPSetup.exe"
