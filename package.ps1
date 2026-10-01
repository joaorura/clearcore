# Clearcore Desktop Application - Build & Package (PowerShell)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Clearcore Desktop Application - Build & Package" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

Push-Location (Join-Path $ScriptDir "crates\app-tauri")
try {
    npm run package
} finally {
    Pop-Location
}

Write-Host "`nPackaging complete! Standalone packages located in: $ScriptDir\release" -ForegroundColor Green
