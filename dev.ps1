# Clearcore Desktop - Starting Development Environment (PowerShell)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Clearcore Desktop - Starting Development Environment" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$appDir = Join-Path $ScriptDir "crates\app-tauri"
$electronBin = Join-Path $appDir "node_modules\.bin\electron.cmd"
if (-not (Test-Path $electronBin)) {
    Write-Host "📦 Dependencies missing. Running npm install..." -ForegroundColor Yellow
    Push-Location $appDir
    npm install
    Pop-Location
}

Push-Location $appDir
try {
    npm run dev
} finally {
    Pop-Location
}
