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
$ExecCommand = $null

# 1. Procurar Clearcore.exe empacotado (relativo ao script)
$CandidateExes = @(
    (Join-Path $ScriptDir "..\..\Clearcore.exe"),
    (Join-Path $ScriptDir "..\Clearcore.exe"),
    (Join-Path $ScriptDir "Clearcore.exe"),
    (Join-Path $ScriptDir "..\release\Clearcore-win32-x64\Clearcore.exe")
)

foreach ($cand in $CandidateExes) {
    if (Test-Path $cand) {
        $fullExe = (Resolve-Path $cand).Path
        $ExecCommand = "`"$fullExe`" --tray"
        break
    }
}

# 2. Se nao encontrou o binario empacotado, checar se esta rodando do repositorio fonte
if (-not $ExecCommand) {
    $RepoRoot = Resolve-Path "$ScriptDir\.." -ErrorAction SilentlyContinue
    if ($RepoRoot) {
        $AppDir = "$RepoRoot\crates\app-tauri"
        if (Test-Path "$AppDir\package.json") {
            $ExecCommand = "npm.cmd start --prefix `"$AppDir`" -- --tray"
        }
    }
}

# 3. Fallback para caminho padrao de instalacao ou PATH
if (-not $ExecCommand) {
    $DefaultInstall = "$env:ProgramFiles\Clearcore\Clearcore.exe"
    if (Test-Path $DefaultInstall) {
        $ExecCommand = "`"$DefaultInstall`" --tray"
    } else {
        $ExecCommand = "Clearcore.exe --tray"
    }
}

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
