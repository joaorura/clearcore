@echo off
setlocal
echo ==========================================================
echo  Clearcore - Desinstalador Completo (Windows CMD)
echo ==========================================================
set SCRIPT_DIR=%~dp0
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%uninstall-windows.ps1"
echo.
pause
