# ==============================================================================
# Clearcore / Hippocamp - Iniciar Tudo (Windows PowerShell)
# Daemon de Supressao de Ruido + Microfone Virtual WaveRT + Desktop Companion Tray
# ==============================================================================

[CmdletBinding()]
param()

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$BinService = "$ScriptDir\target\release\realtime-noise-service.exe"
$BinCli = "$ScriptDir\target\release\realtime-noise-app-tauri.exe"
$AppDir = "$ScriptDir\crates\app-tauri"

Write-Host "=========================================================="
Write-Host " Iniciando Orca / Clearcore no Windows (Daemon + App UI)"
Write-Host "=========================================================="

# 1. Compilar binários automaticamente se faltarem
if (-not (Test-Path $BinService) -or -not (Test-Path $BinCli)) {
    Write-Host "[1/4] Compilando binarios Rust (release)..."
    cargo build --release -p realtime-noise-service -p realtime-noise-app-tauri
}

# 2. Verificar / Criar Microfone Virtual WaveRT
Write-Host "[2/4] Verificando e criando microfone virtual WaveRT..."
& "$ScriptDir\scripts\check-virtual-mic-windows.ps1"

# 3. Iniciar Daemon em segundo plano
Write-Host "[3/4] Iniciando daemon de audio em segundo plano..."
$existingDaemon = Get-Process -Name "realtime-noise-service" -ErrorAction SilentlyContinue
if ($existingDaemon) {
    Write-Host "Daemon ja esta em execucao (PID: $($existingDaemon.Id))."
} else {
    Start-Process -FilePath $BinService -ArgumentList "--run" -WindowStyle Hidden
    Start-Sleep -Seconds 1
    Write-Host "Daemon iniciado com sucesso."
}

# 4. Iniciar Desktop Companion (Electron Tray)
Write-Host "[4/4] Iniciando Desktop Companion (Bandeja do Windows)..."
$distHtml = "$AppDir\dist\index.html"
if (-not (Test-Path $distHtml)) {
    Write-Host "Compilando frontend React para producao..."
    Push-Location $AppDir
    try {
        npm run build | Out-Null
    } finally {
        Pop-Location
    }
}

$existingElectron = Get-Process -Name "electron" -ErrorAction SilentlyContinue
if ($existingElectron) {
    Write-Host "Desktop Companion ja esta em execucao na bandeja."
} else {
    Start-Process -FilePath "npm.cmd" -ArgumentList "start -- --tray" -WorkingDirectory $AppDir -WindowStyle Hidden
    Start-Sleep -Seconds 1
}

Write-Host "=========================================================="
Write-Host " Tudo pronto e em execucao no Windows!"
Write-Host " - Daemon Audio:       Ativo via Win32 Named Pipe (\\.\pipe\realtime-noise-control-v1)"
Write-Host " - Microfone Virtual:  WaveRT Driver Ativo (Root\RealtimeNoise)"
Write-Host " - Bandeja do Sistema: Icone ativo na bandeja (System Tray)"
Write-Host " - Para parar tudo:    .\stop-all.bat ou .\stop-all.ps1"
Write-Host "=========================================================="
