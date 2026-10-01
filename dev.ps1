# Clearcore Desktop - Starting Development Environment (PowerShell)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Clearcore Desktop - Starting Development Environment" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

Push-Location (Join-Path $ScriptDir "crates\app-tauri")
try {
    npm run dev
} finally {
    Pop-Location
}
