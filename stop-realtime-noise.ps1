# ==============================================================================
# Clearcore / Hippocamp - Parar Somente Daemon (Windows PowerShell)
# ==============================================================================

[CmdletBinding()]
param()

$ErrorActionPreference = "SilentlyContinue"

Write-Host "Parando Clearcore Realtime Noise Daemon no Windows..."
Stop-Process -Name "realtime-noise-service" -Force -ErrorAction SilentlyContinue
Write-Host "Daemon finalizado."
