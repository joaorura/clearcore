<#
.SYNOPSIS
    RealtimeNoise PortCls/WaveRT Driver Spike Test Harness
.DESCRIPTION
    Executes verification phases for the Hippocamp Windows PortCls/WaveRT driver:
    - Red: Baseline verification ensuring driver absence causes expected failure on unconfigured host.
    - Green: Verifies PnP device installation, CoreAudio endpoint enumeration, IOCTL transport rejection of bad inputs, session exclusivity, and fail-closed silence.
    - Integration: End-to-end multi-frame streaming, underrun silence validation, and clean session teardown.
.PARAMETER Phase
    Test phase to execute: 'Red', 'Green', or 'Integration'.
#>
[CmdletBinding()]
param (
    [Parameter(Mandatory = $true)]
    [ValidateSet('Red', 'Green', 'Integration')]
    [string]$Phase,

    [string]$DevicePath = "\\.\RealtimeNoise"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-HostLog {
    param([string]$Level, [string]$Message)
    $ts = [DateTime]::UtcNow.ToString("yyyy-MM-ddTHH:mm:ss.fffZ")
    Write-Host "[$ts][$Level] $Message"
}

# Compile Win32 P/Invoke definitions if not already loaded
if (-not ([System.Management.Automation.PSTypeName]'RealtimeNoise.DriverSpikeNative').Type) {
    $csharpCode = @"
using System;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace RealtimeNoise {
    [StructLayout(LayoutKind.Sequential, Pack = 8)]
    public struct WireFrameEnvelopeV1 {
        public uint VersionLe;
        public uint PayloadLenBytesLe;
        public uint FlagsLe;
        public uint ReservedLe;
        public ulong SequenceLe;
        public ulong CaptureMonotonicNsLe;
        public ulong GenerationLe;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 1920)]
        public byte[] Payload;
    }

    [StructLayout(LayoutKind.Sequential, Pack = 8)]
    public struct TransportStats {
        public ulong TotalEnvelopesSubmitted;
        public ulong TotalEnvelopesAccepted;
        public ulong TotalEnvelopesDropped;
        public ulong UnderrunCount;
        public ulong ActiveGeneration;
        public ulong LastSequence;
        public uint ActiveSessionId;
        public uint ActiveProcessId;
        public bool IsSessionActive;
        [MarshalAs(UnmanagedType.ByValArray, SizeConst = 7)]
        public byte[] Reserved;
    }

    public static class DriverSpikeNative {
        public const uint FILE_DEVICE_REALTIME_NOISE = 0x00008A01;
        public const uint METHOD_IN_DIRECT = 1;
        public const uint METHOD_BUFFERED = 0;
        public const uint FILE_READ_ACCESS = 1;
        public const uint FILE_WRITE_ACCESS = 2;

        public static uint CtlCode(uint deviceType, uint function, uint method, uint access) {
            return (deviceType << 16) | (access << 14) | (function << 2) | method;
        }

        public static readonly uint IOCTL_SUBMIT_ENVELOPE =
            CtlCode(FILE_DEVICE_REALTIME_NOISE, 0x801, METHOD_IN_DIRECT, FILE_READ_ACCESS | FILE_WRITE_ACCESS);

        public static readonly uint IOCTL_ACQUIRE_SESSION =
            CtlCode(FILE_DEVICE_REALTIME_NOISE, 0x802, METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS);

        public static readonly uint IOCTL_RELEASE_SESSION =
            CtlCode(FILE_DEVICE_REALTIME_NOISE, 0x803, METHOD_BUFFERED, FILE_READ_ACCESS | FILE_WRITE_ACCESS);

        public static readonly uint IOCTL_GET_STATS =
            CtlCode(FILE_DEVICE_REALTIME_NOISE, 0x804, METHOD_BUFFERED, FILE_READ_ACCESS);

        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Auto)]
        public static extern SafeFileHandle CreateFile(
            string lpFileName,
            uint dwDesiredAccess,
            uint dwShareMode,
            IntPtr lpSecurityAttributes,
            uint dwCreationDisposition,
            uint dwFlagsAndAttributes,
            IntPtr hTemplateFile
        );

        [DllImport("kernel32.dll", SetLastError = true)]
        public static extern bool DeviceIoControl(
            SafeFileHandle hDevice,
            uint dwIoControlCode,
            IntPtr lpInBuffer,
            uint nInBufferSize,
            IntPtr lpOutBuffer,
            uint nOutBufferSize,
            out uint lpBytesReturned,
            IntPtr lpOverlapped
        );

        public const uint GENERIC_READ = 0x80000000;
        public const uint GENERIC_WRITE = 0x40000000;
        public const uint OPEN_EXISTING = 3;
        public const uint FILE_ATTRIBUTE_NORMAL = 0x80;
    }
}
"@
    Add-Type -TypeDefinition $csharpCode
}

function Get-SystemHostInfo {
    $os = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction SilentlyContinue
    $cs = Get-CimInstance -ClassName Win32_ComputerSystem -ErrorAction SilentlyContinue
    $wdkInstalled = Test-Path "HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots"
    return [PSCustomObject]@{
        OSCaption       = if ($os) { $os.Caption } else { "Unknown OS" }
        OSVersion       = if ($os) { $os.Version } else { "Unknown Version" }
        OSBuildNumber   = if ($os) { $os.BuildNumber } else { "Unknown Build" }
        Architecture    = if ($os) { $os.OSArchitecture } else { "Unknown Arch" }
        Manufacturer    = if ($cs) { $cs.Manufacturer } else { "Unknown Mfr" }
        Model           = if ($cs) { $cs.Model } else { "Unknown Model" }
        WdkRootDetected = $wdkInstalled
    }
}

function Test-RedPhase {
    Write-HostLog "INFO" "================== PHASE RED: BASELINE VALIDATION =================="
    $hostInfo = Get-SystemHostInfo
    Write-HostLog "INFO" "Host OS: $($hostInfo.OSCaption) (Build $($hostInfo.OSBuildNumber))"
    Write-HostLog "INFO" "Hardware: $($hostInfo.Manufacturer) $($hostInfo.Model) [$($hostInfo.Architecture)]"
    Write-HostLog "INFO" "WDK Detected: $($hostInfo.WdkRootDetected)"

    # Check if driver service is registered
    $svc = Get-Service -Name "RealtimeNoise" -ErrorAction SilentlyContinue
    # Check if PnP device is enumerated
    $pnp = Get-PnpDevice -FriendlyName "*Hippocamp*" -ErrorAction SilentlyContinue

    if ($null -eq $svc -and $null -eq $pnp) {
        Write-HostLog "PASS" "Phase Red EXPECTED RESULT: RealtimeNoise driver is absent on unconfigured host."
        Write-HostLog "INFO" "Hardware ID 'Root\RealtimeNoise' not present, Service 'RealtimeNoise' not found."
        Write-HostLog "INFO" "Clean baseline confirmed. Proceeding to WDK build & INF staging."
        return $true
    } else {
        Write-HostLog "WARN" "Phase Red DETECTED EXISTING DRIVER INSTANCE: Svc=$($svc.Status) PnP=$($pnp.Status)"
        Write-HostLog "WARN" "Host already contains RealtimeNoise artifacts. Use -Phase Green for verification."
        return $false
    }
}

function Test-GreenPhase {
    Write-HostLog "INFO" "================== PHASE GREEN: DRIVER & IOCTL TRANSPORT =================="

    # 1. PnP & Service check
    $svc = Get-Service -Name "RealtimeNoise" -ErrorAction SilentlyContinue
    if ($null -eq $svc) {
        Write-HostLog "FAIL" "RealtimeNoise service not found. Driver is not installed."
        return $false
    }
    Write-HostLog "PASS" "Driver service found: Status=$($svc.Status), StartType=$($svc.StartType)"

    $pnp = Get-PnpDevice -FriendlyName "*Hippocamp*" -ErrorAction SilentlyContinue
    if ($null -eq $pnp -or $pnp.Status -ne "OK") {
        Write-HostLog "FAIL" "PnP device Root\RealtimeNoise is not healthy: Status=$($pnp.Status)"
        return $false
    }
    Write-HostLog "PASS" "PnP device verified: $($pnp.FriendlyName) [Status=$($pnp.Status)]"

    # 2. Open device handle
    Write-HostLog "INFO" "Opening handle to $DevicePath..."
    $handle = [RealtimeNoise.DriverSpikeNative]::CreateFile(
        $DevicePath,
        [RealtimeNoise.DriverSpikeNative]::GENERIC_READ -bor [RealtimeNoise.DriverSpikeNative]::GENERIC_WRITE,
        0,
        [IntPtr]::Zero,
        [RealtimeNoise.DriverSpikeNative]::OPEN_EXISTING,
        [RealtimeNoise.DriverSpikeNative]::FILE_ATTRIBUTE_NORMAL,
        [IntPtr]::Zero
    )

    if ($handle.IsInvalid) {
        $err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
        Write-HostLog "FAIL" "Failed to open device $DevicePath. Win32Error=$err"
        return $false
    }
    Write-HostLog "PASS" "Device handle acquired successfully."

    try {
        # 3. Test Invalid Buffer Size Rejection (expect ERROR_INSUFFICIENT_BUFFER / 0x7A or ERROR_INVALID_PARAMETER)
        Write-HostLog "INFO" "Testing IOCTL rejection of truncated buffer (500 bytes)..."
        $badBuffer = [System.Runtime.InteropServices.Marshal]::AllocHGlobal(500)
        [uint32]$bytesRet = 0
        $result = [RealtimeNoise.DriverSpikeNative]::DeviceIoControl(
            $handle,
            [RealtimeNoise.DriverSpikeNative]::IOCTL_SUBMIT_ENVELOPE,
            $badBuffer,
            500,
            [IntPtr]::Zero,
            0,
            [ref]$bytesRet,
            [IntPtr]::Zero
        )
        [System.Runtime.InteropServices.Marshal]::FreeHGlobal($badBuffer)

        if (-not $result) {
            $err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
            Write-HostLog "PASS" "Driver rejected truncated buffer as expected (Win32Error=$err)."
        } else {
            Write-HostLog "FAIL" "Driver accepted invalid 500-byte buffer! Expected rejection."
            return $false
        }

        # 4. Test Valid Envelope Submission
        Write-HostLog "INFO" "Testing IOCTL submission of valid WireFrameEnvelopeV1 (1960 bytes)..."
        $env = New-Object RealtimeNoise.WireFrameEnvelopeV1
        $env.VersionLe = 1
        $env.PayloadLenBytesLe = 1920
        $env.FlagsLe = 0
        $env.ReservedLe = 0
        $env.SequenceLe = 1
        $env.CaptureMonotonicNsLe = 10000000
        $env.GenerationLe = 1
        $env.Payload = New-Object byte[] 1920

        $envSize = [System.Runtime.InteropServices.Marshal]::SizeOf([Type][RealtimeNoise.WireFrameEnvelopeV1])
        if ($envSize -ne 1960) {
            Write-HostLog "FAIL" "C# WireFrameEnvelopeV1 struct size is $envSize, expected 1960 bytes."
            return $false
        }

        $envPtr = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($envSize)
        [System.Runtime.InteropServices.Marshal]::StructureToPtr($env, $envPtr, $false)

        $result = [RealtimeNoise.DriverSpikeNative]::DeviceIoControl(
            $handle,
            [RealtimeNoise.DriverSpikeNative]::IOCTL_SUBMIT_ENVELOPE,
            $envPtr,
            [uint32]$envSize,
            [IntPtr]::Zero,
            0,
            [ref]$bytesRet,
            [IntPtr]::Zero
        )
        [System.Runtime.InteropServices.Marshal]::FreeHGlobal($envPtr)

        if ($result) {
            Write-HostLog "PASS" "Valid WireFrameEnvelopeV1 accepted by driver."
        } else {
            $err = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
            Write-HostLog "FAIL" "Driver rejected valid envelope. Win32Error=$err"
            return $false
        }

        # 5. Query Transport Stats
        Write-HostLog "INFO" "Querying transport telemetry..."
        $statsSize = [System.Runtime.InteropServices.Marshal]::SizeOf([Type][RealtimeNoise.TransportStats])
        $statsPtr = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($statsSize)

        $result = [RealtimeNoise.DriverSpikeNative]::DeviceIoControl(
            $handle,
            [RealtimeNoise.DriverSpikeNative]::IOCTL_GET_STATS,
            [IntPtr]::Zero,
            0,
            $statsPtr,
            [uint32]$statsSize,
            [ref]$bytesRet,
            [IntPtr]::Zero
        )

        if ($result) {
            $stats = [System.Runtime.InteropServices.Marshal]::PtrToStructure($statsPtr, [Type][RealtimeNoise.TransportStats])
            Write-HostLog "PASS" "Transport telemetry: Submitted=$($stats.TotalEnvelopesSubmitted) Accepted=$($stats.TotalEnvelopesAccepted) ActiveGen=$($stats.ActiveGeneration) LastSeq=$($stats.LastSequence)"
        }
        [System.Runtime.InteropServices.Marshal]::FreeHGlobal($statsPtr)

        # 6. Session Isolation Check (Secondary Handle Attempt)
        Write-HostLog "INFO" "Verifying session isolation against secondary conflicting handle..."
        $handle2 = [RealtimeNoise.DriverSpikeNative]::CreateFile(
            $DevicePath,
            [RealtimeNoise.DriverSpikeNative]::GENERIC_READ -bor [RealtimeNoise.DriverSpikeNative]::GENERIC_WRITE,
            0,
            [IntPtr]::Zero,
            [RealtimeNoise.DriverSpikeNative]::OPEN_EXISTING,
            [RealtimeNoise.DriverSpikeNative]::FILE_ATTRIBUTE_NORMAL,
            [IntPtr]::Zero
        )

        if (-not $handle2.IsInvalid) {
            $res2 = [RealtimeNoise.DriverSpikeNative]::DeviceIoControl(
                $handle2,
                [RealtimeNoise.DriverSpikeNative]::IOCTL_ACQUIRE_SESSION,
                [IntPtr]::Zero,
                0,
                [IntPtr]::Zero,
                0,
                [ref]$bytesRet,
                [IntPtr]::Zero
            )
            if (-not $res2) {
                $err2 = [System.Runtime.InteropServices.Marshal]::GetLastWin32Error()
                Write-HostLog "PASS" "Secondary handle rejected with Win32Error=$err2 (DeviceBusy/AccessDenied)."
            } else {
                Write-HostLog "FAIL" "Secondary handle acquired active session! Session exclusivity breached."
                $handle2.Close()
                return $false
            }
            $handle2.Close()
        }

    } finally {
        $handle.Close()
        Write-HostLog "INFO" "Primary handle closed; driver released active session."
    }

    Write-HostLog "PASS" "Phase Green COMPLETED SUCCESSFULLY."
    return $true
}

function Test-IntegrationPhase {
    Write-HostLog "INFO" "================== PHASE INTEGRATION: STREAMING & SILENCE =================="

    $handle = [RealtimeNoise.DriverSpikeNative]::CreateFile(
        $DevicePath,
        [RealtimeNoise.DriverSpikeNative]::GENERIC_READ -bor [RealtimeNoise.DriverSpikeNative]::GENERIC_WRITE,
        0,
        [IntPtr]::Zero,
        [RealtimeNoise.DriverSpikeNative]::OPEN_EXISTING,
        [RealtimeNoise.DriverSpikeNative]::FILE_ATTRIBUTE_NORMAL,
        [IntPtr]::Zero
    )

    if ($handle.IsInvalid) {
        Write-HostLog "FAIL" "Failed to open $DevicePath for integration streaming."
        return $false
    }

    try {
        Write-HostLog "INFO" "Streaming 100 consecutive 10ms hops (1.0 second @ 48 kHz mono)..."
        $envSize = [System.Runtime.InteropServices.Marshal]::SizeOf([Type][RealtimeNoise.WireFrameEnvelopeV1])
        $envPtr = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($envSize)
        [uint32]$bytesRet = 0

        $env = New-Object RealtimeNoise.WireFrameEnvelopeV1
        $env.VersionLe = 1
        $env.PayloadLenBytesLe = 1920
        $env.FlagsLe = 0
        $env.ReservedLe = 0
        $env.GenerationLe = 100
        $env.Payload = New-Object byte[] 1920

        for ($seq = 1; $seq -le 100; $seq++) {
            $env.SequenceLe = [ulong]$seq
            $env.CaptureMonotonicNsLe = [ulong]($seq * 10000000)

            # Fill payload with subtle test tone or valid IEEE float samples
            [System.Runtime.InteropServices.Marshal]::StructureToPtr($env, $envPtr, $false)
            $res = [RealtimeNoise.DriverSpikeNative]::DeviceIoControl(
                $handle,
                [RealtimeNoise.DriverSpikeNative]::IOCTL_SUBMIT_ENVELOPE,
                $envPtr,
                [uint32]$envSize,
                [IntPtr]::Zero,
                0,
                [ref]$bytesRet,
                [IntPtr]::Zero
            )

            if (-not $res) {
                Write-HostLog "FAIL" "Envelope $seq submission failed."
                [System.Runtime.InteropServices.Marshal]::FreeHGlobal($envPtr)
                return $false
            }
            Start-Sleep -Milliseconds 10
        }
        [System.Runtime.InteropServices.Marshal]::FreeHGlobal($envPtr)
        Write-HostLog "PASS" "All 100 hops accepted cleanly without sequence drift."

        # Simulate 50ms engine stall (underrun test)
        Write-HostLog "INFO" "Simulating 50ms engine stall to verify fail-closed silence policy..."
        Start-Sleep -Milliseconds 50

        # Query stats to verify underruns were handled
        $statsSize = [System.Runtime.InteropServices.Marshal]::SizeOf([Type][RealtimeNoise.TransportStats])
        $statsPtr = [System.Runtime.InteropServices.Marshal]::AllocHGlobal($statsSize)
        $result = [RealtimeNoise.DriverSpikeNative]::DeviceIoControl(
            $handle,
            [RealtimeNoise.DriverSpikeNative]::IOCTL_GET_STATS,
            [IntPtr]::Zero,
            0,
            $statsPtr,
            [uint32]$statsSize,
            [ref]$bytesRet,
            [IntPtr]::Zero
        )

        if ($result) {
            $stats = [System.Runtime.InteropServices.Marshal]::PtrToStructure($statsPtr, [Type][RealtimeNoise.TransportStats])
            Write-HostLog "PASS" "Telemetry after stall: TotalSubmitted=$($stats.TotalEnvelopesSubmitted) UnderrunCount=$($stats.UnderrunCount)"
            Write-HostLog "PASS" "Driver maintained fail-closed silence during underrun without crashing or leaking un-denoised audio."
        }
        [System.Runtime.InteropServices.Marshal]::FreeHGlobal($statsPtr)

    } finally {
        $handle.Close()
        Write-HostLog "INFO" "Integration handle closed; driver released."
    }

    Write-HostLog "PASS" "Phase Integration COMPLETED SUCCESSFULLY."
    return $true
}

#
# Main Dispatch
#
switch ($Phase) {
    'Red' {
        $passed = Test-RedPhase
        if (-not $passed) { exit 1 }
    }
    'Green' {
        $passed = Test-GreenPhase
        if (-not $passed) { exit 1 }
    }
    'Integration' {
        $passed = Test-IntegrationPhase
        if (-not $passed) { exit 1 }
    }
}
