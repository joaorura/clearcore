# ==============================================================================
# Clearcore / Hippocamp - Parar Tudo (Windows PowerShell)
# ==============================================================================

[CmdletBinding()]
param()

$ErrorActionPreference = "SilentlyContinue"

Write-Host "=========================================================="
Write-Host " Parando ClearCore no Windows (Daemon + App UI)"
Write-Host "=========================================================="

# 1. Parar o daemon de audio
Stop-Process -Name "realtime-noise-service" -Force -ErrorAction SilentlyContinue
Write-Host "Daemon de audio finalizado."

# 2. Parar o Desktop Companion (Electron)
Stop-Process -Name "electron" -Force -ErrorAction SilentlyContinue
Get-Process | Where-Object { $_.CommandLine -like "*electron*main.cjs*" } | Stop-Process -Force -ErrorAction SilentlyContinue
Write-Host "Desktop Companion finalizado."

Write-Host "Todos os servicos e companion da bandeja foram finalizados com sucesso."
Write-Host "=========================================================="
