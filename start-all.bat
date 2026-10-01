@echo off
REM ==============================================================================
REM Clearcore / Hippocamp - Iniciar Tudo (Windows Batch Launcher)
REM ==============================================================================

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-all.ps1" %*
if %ERRORLEVEL% neq 0 (
    echo [ERRO] Falha ao iniciar Clearcore.
    pause
)
