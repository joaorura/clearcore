@echo off
setlocal
echo ==========================================================
echo  Clearcore Desktop - Starting Development Environment
echo ==========================================================

cd /d "%~dp0crates\app-tauri"
call npm run dev
endlocal
