@echo off
setlocal
echo ==========================================================
echo  Clearcore Desktop Application - Build ^& Package
echo ==========================================================

cd /d "%~dp0crates\app-tauri"
call npm run package

echo.
echo Packaging complete! Standalone packages located in: %~dp0release
endlocal
