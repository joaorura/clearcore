# unregister-user-service.ps1
# Unregisters Realtime Noise Suppression service scheduled task.

[CmdletBinding()]
param(
    [string]$TaskName = "RealtimeNoiseService"
)

$ErrorActionPreference = "Stop"

Write-Host "Unregistering scheduled task '$TaskName'..."

try {
    Stop-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
    Write-Host "Successfully unregistered scheduled task '$TaskName'."
} catch {
    Write-Warning "Task '$TaskName' could not be found or was already unregistered: $_"
}
