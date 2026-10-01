# ==============================================================================
# Clearcore / Hippocamp - Configurador de Autostart (Windows PowerShell)
# ==============================================================================

[CmdletBinding()]
param(
    [ValidateSet('enable', 'disable', 'status')]
    [string]$Action = "status"
)

$ErrorActionPreference = "Stop"

$RegPath = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$ValueName = "ClearcoreRealtimeNoise"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RepoRoot = Resolve-Path "$ScriptDir\.."
$AppDir = "$RepoRoot\crates\app-tauri"
$ExecCommand = "npm.cmd start --prefix `"$AppDir`" -- --tray"

switch ($Action) {
    "enable" {
        Set-ItemProperty -Path $RegPath -Name $ValueName -Value $ExecCommand
        Write-Host "[OK] Inicializacao automatica ativada no Windows!"
        Write-Host "     O Clearcore iniciara minimizado na bandeja ao fazer login."
        Write-Host "     Registro: $RegPath\$ValueName"
    }
    "disable" {
        if (Get-ItemProperty -Path $RegPath -Name $ValueName -ErrorAction SilentlyContinue) {
            Remove-ItemProperty -Path $RegPath -Name $ValueName
            Write-Host "[OK] Inicializacao automatica desativada no Windows."
        } else {
            Write-Host "[INFO] Inicializacao automatica ja estava desativada."
        }
    }
    "status" {
        $val = Get-ItemProperty -Path $RegPath -Name $ValueName -ErrorAction SilentlyContinue
        if ($val) {
            Write-Host "[ATIVADO] Clearcore esta configurado para iniciar na bandeja do Windows."
            Write-Host "          Comando: $($val.$ValueName)"
        } else {
            Write-Host "[DESATIVADO] Inicializacao automatica nao esta configurada no Windows."
            Write-Host "             Para ativar, execute: .\scripts\setup-autostart.ps1 -Action enable"
        }
    }
}
