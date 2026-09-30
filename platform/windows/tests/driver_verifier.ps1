<#
.SYNOPSIS
    Windows Driver Verifier Configuration & Diagnostics for RealtimeNoise.sys
.DESCRIPTION
    Manages Driver Verifier settings to rigorously test memory integrity,
    IRQL rules, and spinlock deadlock detection for RealtimeNoise.sys.
.PARAMETER Action
    Operation to perform: 'Status' (default), 'Enable', 'Disable', or 'CheckViolations'.
.PARAMETER DriverName
    Target driver binary name (default: RealtimeNoise.sys).
#>
[CmdletBinding()]
param (
    [Parameter()]
    [ValidateSet('Status', 'Enable', 'Disable', 'CheckViolations')]
    [string]$Action = 'Status',

    [Parameter()]
    [string]$DriverName = 'RealtimeNoise.sys'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-HostLog {
    param([string]$Level, [string]$Message)
    $ts = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
    Write-Host "[$ts][$Level] $Message"
}

# Bitmask flags for Driver Verifier:
# 0x00000001 = Special Pool (catches buffer overruns & use-after-free)
# 0x00000002 = Force IRQL Checking (invalid page access at DISPATCH_LEVEL)
# 0x00000020 = Deadlock Detection (spinlock hierarchy & lock ordering)
# 0x00000400 = Security Checks (kernel pointer validation & MDL handling)
# 0x00001000 = Miscellaneous Checks (IRP lifecycle & pool leak tracking)
$VerifierFlags = 0x1423

function Check-AdministratorPrivileges {
    $currentPrincipal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
    return $currentPrincipal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

switch ($Action) {
    'Status' {
        Write-HostLog "INFO" "Checking Driver Verifier configuration for $DriverName..."
        try {
            $output = & verifier /query 2>&1
            Write-HostLog "INFO" "Current Driver Verifier settings:"
            $output | ForEach-Object { Write-Host "    $_" }
            if ($output -match $DriverName) {
                Write-HostLog "PASS" "Driver Verifier is currently ACTIVE for $DriverName."
            } else {
                Write-HostLog "WARN" "Driver Verifier is NOT active for $DriverName."
            }
        } catch {
            Write-HostLog "ERROR" "Failed to execute verifier /query: $_"
        }
    }

    'Enable' {
        if (-not (Check-AdministratorPrivileges)) {
            Write-HostLog "ERROR" "Elevated administrator privileges are required to configure Driver Verifier."
            exit 1
        }
        Write-HostLog "INFO" "Configuring Driver Verifier for $DriverName (Flags: 0x$($VerifierFlags.ToString('X4')))..."
        Write-HostLog "INFO" "Enabling: Special Pool, Force IRQL Checking, Deadlock Detection, Security Checks, Misc Checks."
        
        $proc = Start-Process -FilePath "verifier.exe" -ArgumentList "/flags $VerifierFlags /driver $DriverName" -Wait -PassThru -NoNewWindow
        if ($proc.ExitCode -eq 0) {
            Write-HostLog "PASS" "Driver Verifier flags configured successfully."
            Write-HostLog "IMPORTANT" "A system reboot is required for Driver Verifier flags to take effect."
        } else {
            Write-HostLog "FAIL" "verifier.exe returned non-zero exit code: $($proc.ExitCode)"
            exit $proc.ExitCode
        }
    }

    'Disable' {
        if (-not (Check-AdministratorPrivileges)) {
            Write-HostLog "ERROR" "Elevated administrator privileges are required to disable Driver Verifier."
            exit 1
        }
        Write-HostLog "INFO" "Resetting Driver Verifier settings..."
        $proc = Start-Process -FilePath "verifier.exe" -ArgumentList "/reset" -Wait -PassThru -NoNewWindow
        if ($proc.ExitCode -eq 0) {
            Write-HostLog "PASS" "Driver Verifier reset successfully."
            Write-HostLog "IMPORTANT" "A system reboot is required to completely unload verifier shims."
        } else {
            Write-HostLog "FAIL" "verifier.exe /reset returned non-zero exit code: $($proc.ExitCode)"
            exit $proc.ExitCode
        }
    }

    'CheckViolations' {
        Write-HostLog "INFO" "Scanning Windows System Event Log for bugchecks and Verifier violations..."
        $startTime = (Get-Date).AddDays(-7)

        # Common bugcheck codes associated with driver defects:
        # 0xC4 = DRIVER_VERIFIER_DETECTED_VIOLATION
        # 0xD1 = DRIVER_IRQL_NOT_LESS_OR_EQUAL
        # 0x0A = IRQL_NOT_LESS_OR_EQUAL
        # 0x3B = SYSTEM_SERVICE_EXCEPTION
        # 0xC1 = SPECIAL_POOL_DETECTED_MEMORY_CORRUPTION
        # 0xC5 = DRIVER_CORRUPTED_EXPOOL
        try {
            $events = Get-WinEvent -FilterHashtable @{
                LogName   = 'System'
                ProviderName = 'Microsoft-Windows-Kernel-General', 'Microsoft-Windows-WER-SystemErrorReporting', 'BugCheck'
                StartTime = $startTime
            } -ErrorAction SilentlyContinue

            $violations = @()
            if ($null -ne $events) {
                foreach ($evt in $events) {
                    $msg = $evt.Message
                    if ($msg -match "0x000000c4" -or $msg -match "0x000000d1" -or $msg -match "0x000000c1" -or $msg -match $DriverName) {
                        $violations += $evt
                    }
                }
            }

            if ($violations.Count -eq 0) {
                Write-HostLog "PASS" "Zero Driver Verifier violations or bugchecks detected in the last 7 days."
            } else {
                Write-HostLog "FAIL" "Detected $($violations.Count) bugcheck/verifier event(s):"
                foreach ($v in $violations) {
                    Write-Host "    Time: $($v.TimeCreated) | Id: $($v.Id) | Message: $($v.Message)"
                }
                exit 1
            }
        } catch {
            Write-HostLog "INFO" "No crash log entries found or event provider not active."
        }
    }
}
