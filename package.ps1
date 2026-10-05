# Clearcore Desktop Application - Build & Package (PowerShell)
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

Write-Host "==========================================================" -ForegroundColor Cyan
Write-Host " Clearcore Desktop Application - Build & Package" -ForegroundColor Cyan
Write-Host "==========================================================" -ForegroundColor Cyan

$DaemonBin = Join-Path $ScriptDir "target\release\realtime-noise-service.exe"
if (-not (Test-Path $DaemonBin)) {
    if (Get-Command cargo -ErrorAction SilentlyContinue) {
        Write-Host "Building Rust realtime-noise-service daemon and filter C-API..." -ForegroundColor Yellow
        cargo build --release -p realtime-noise-service -p realtime-noise-filter-capi
    }
}

$DriverDir = Join-Path $ScriptDir "platform\windows\driver"
$DriverSys = Join-Path $DriverDir "x64\Release\RealtimeNoise.sys"
if (-not (Test-Path $DriverSys)) {
    if ((Get-Command nuget -ErrorAction SilentlyContinue) -and (Get-Command msbuild -ErrorAction SilentlyContinue)) {
        Write-Host "Restoring WDK NuGet and compiling WaveRT driver (RealtimeNoise.sys)..." -ForegroundColor Yellow
        Push-Location $DriverDir
        try {
            nuget restore packages.config -PackagesDirectory packages
            msbuild driver.vcxproj /p:Configuration=Release /p:Platform=x64
        } catch {
            Write-Warning "WDK driver compilation failed or skipped: $_"
        } finally {
            Pop-Location
        }
    }
}

Push-Location (Join-Path $ScriptDir "crates\app-tauri")
try {
    npm run package
} finally {
    Pop-Location
}

$OutDir = Join-Path $ScriptDir "release"

# Build NSIS and/or Inno Setup turnkey installers if tools are available
$InstallerDir = Join-Path $ScriptDir "platform\windows\installer"
$NsiScript = Join-Path $InstallerDir "Clearcore-Setup.nsi"
$IssScript = Join-Path $InstallerDir "Clearcore-Setup.iss"

if (Get-Command makensis -ErrorAction SilentlyContinue) {
    Write-Host "Building NSIS turnkey installer (Clearcore-Setup.exe)..." -ForegroundColor Yellow
    Push-Location $InstallerDir
    try {
        & makensis Clearcore-Setup.nsi
        Write-Host "✓ Clearcore-Setup.exe built successfully with NSIS." -ForegroundColor Green
    } catch {
        Write-Warning "NSIS build failed: $_"
    } finally {
        Pop-Location
    }
}

if (Get-Command iscc -ErrorAction SilentlyContinue) {
    Write-Host "Building Inno Setup installer (Clearcore-Inno-Setup.exe)..." -ForegroundColor Yellow
    Push-Location $InstallerDir
    try {
        & iscc /O"$OutDir" /F"Clearcore-Inno-Setup" Clearcore-Setup.iss
        Write-Host "✓ Clearcore-Inno-Setup.exe built successfully with Inno Setup." -ForegroundColor Green
    } catch {
        Write-Warning "Inno Setup build failed: $_"
    } finally {
        Pop-Location
    }
}

Write-Host ""
Write-Host "Packaging complete! Standalone packages located in: $OutDir" -ForegroundColor Green

