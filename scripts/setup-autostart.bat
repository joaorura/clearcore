@echo off
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0setup-autostart.ps1" -Action %1
