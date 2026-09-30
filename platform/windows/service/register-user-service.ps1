# register-user-service.ps1
# Registers Realtime Noise Suppression service as a per-user scheduled task at logon.
# Ensures per-user ownership and session separation (RL LIMITED).

[CmdletBinding()]
param(
    [string]$BinaryPath = "$PSScriptRoot\realtime-noise-service.exe",
    [string]$TaskName = "RealtimeNoiseService"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $BinaryPath)) {
    Write-Warning "Binary not found at '$BinaryPath'. Please specify -BinaryPath with the correct path to realtime-noise-service.exe."
}

Write-Host "Registering scheduled task '$TaskName' for current user on logon..."

$Action = New-ScheduledTaskAction -Execute $BinaryPath -Argument "--run"
$Trigger = New-ScheduledTaskTrigger -AtLogOn
$Settings = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit (New-TimeSpan -Days 0) -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)
$Principal = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited

Register-ScheduledTask -TaskName $TaskName -Action $Action -Trigger $Trigger -Settings $Settings -Principal $Principal -Force

Write-Host "Successfully registered scheduled task '$TaskName'."
