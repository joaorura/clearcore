# Clearcore Desktop Application - Build & Package (PowerShell)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Clearcore Desktop Application - Build & Package" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$DaemonBin = Join-Path $ScriptDir "target\release\realtime-noise-service.exe"
if (-not (Test-Path $DaemonBin)) {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Host "🔨 Building Rust realtime-noise-service daemon and filter C-API..." -ForegroundColor Yellow
        cargo build --release -p realtime-noise-service -p realtime-noise-filter-capi
    }
}

Push-Location (Join-Path $ScriptDir "crates\app-tauri")
try {
    npm run package
} finally {
    Pop-Location
}

Write-Host "`nPackaging complete! Standalone packages located in: $ScriptDir\release" -ForegroundColor Green
