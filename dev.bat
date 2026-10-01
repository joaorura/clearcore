@echo off
setlocal
echo ==========================================================
echo  Clearcore Desktop - Starting Development Environment
echo ==========================================================

cd /d "%~dp0crates\app-tauri"
if not exist "node_modules\.bin\electron.cmd" (
    echo 📦 Dependencies missing. Running npm install...
    call npm install
)
call npm run dev
endlocal
