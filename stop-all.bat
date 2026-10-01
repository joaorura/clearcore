@echo off
REM ==============================================================================
REM Clearcore / Hippocamp - Parar Tudo (Windows Batch Launcher)
REM ==============================================================================

powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0stop-all.ps1" %*
