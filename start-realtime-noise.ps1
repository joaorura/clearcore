# ==============================================================================
# Clearcore / Hippocamp - Iniciar Somente Daemon (Windows PowerShell)
# ==============================================================================

[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BinService = "$ScriptDir\target\release\realtime-noise-service.exe"

if (-not (Test-Path $BinService)) {
    Write-Error "Binario nao encontrado em '$BinService'. Execute 'cargo build --release -p realtime-noise-service'."
    exit 1
}

Write-Host "=========================================================="
Write-Host " Iniciando Clearcore Realtime Noise Daemon (Windows)"
Write-Host "=========================================================="

# Checar se ja esta rodando
$existingDaemon = Get-Process -Name "realtime-noise-service" -ErrorAction SilentlyContinue
if ($existingDaemon) {
    Write-Host "Daemon ja esta em execucao (PID: $($existingDaemon.Id))."
    & "$ScriptDir\scripts\check-virtual-mic-windows.ps1"
    exit 0
}

# Iniciar em segundo plano
Start-Process -FilePath $BinService -ArgumentList "--run" -WindowStyle Hidden
Start-Sleep -Seconds 1

# Verificar microfone virtual
& "$ScriptDir\scripts\check-virtual-mic-windows.ps1"

Write-Host "Status: RUNNING"
Write-Host "Para parar: .\stop-realtime-noise.bat ou .\stop-realtime-noise.ps1"
Write-Host "=========================================================="
