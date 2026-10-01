; ==============================================================================
; Clearcore Desktop Application - NSIS Installer Script
; ==============================================================================
!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

Name "Clearcore Realtime AI Noise Suppression"
OutFile "..\..\..\release\Clearcore-Setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Clearcore"
InstallDirRegKey HKCU "Software\Clearcore" "Install_Dir"
RequestExecutionLevel admin

; ------------------------------------------------------------------------------
; Interface Configuration
; ------------------------------------------------------------------------------
!define MUI_ABORTWARNING
!define MUI_ICON "..\..\..\crates\app-tauri\assets\icon.ico"
!define MUI_UNICON "..\..\..\crates\app-tauri\assets\icon.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

!define MUI_FINISHPAGE_RUN "$INSTDIR\Clearcore.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--tray"
!define MUI_FINISHPAGE_RUN_TEXT "Iniciar o Clearcore agora na bandeja do sistema"
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "PortugueseBR"
!insertmacro MUI_LANGUAGE "English"

; ------------------------------------------------------------------------------
; Installer Section
; ------------------------------------------------------------------------------
Section "Clearcore Core Application" SecCore
  SetOutPath "$INSTDIR"

  ; Stop running instance if any
  nsExec::Exec 'taskkill /F /IM Clearcore.exe /T'
  nsExec::Exec 'taskkill /F /IM realtime-noise-service.exe /T'
  Sleep 1000

  ; Install packaged files from release\Clearcore-win32-x64
  File /r "..\..\..\release\Clearcore-win32-x64\*.*"

  ; Store installation path
  WriteRegStr HKCU "Software\Clearcore" "Install_Dir" "$INSTDIR"

  ; Add/Remove Programs integration
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore" "DisplayName" "Clearcore Realtime Noise Suppression"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore" "DisplayIcon" "$INSTDIR\Clearcore.exe"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore" "DisplayVersion" "0.1.0"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore" "Publisher" "Clearcore Team"
  WriteRegStr HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore" "NoModify" 1
  WriteRegDWORD HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore" "NoRepair" 1

  ; Write Uninstaller
  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Start Menu Shortcuts
  CreateDirectory "$SMPROGRAMS\Clearcore"
  CreateShortcut "$SMPROGRAMS\Clearcore\Clearcore.lnk" "$INSTDIR\Clearcore.exe" "--tray" "$INSTDIR\Clearcore.exe" 0
  CreateShortcut "$SMPROGRAMS\Clearcore\Desinstalar Clearcore.lnk" "$INSTDIR\uninstall.exe" "" "$INSTDIR\uninstall.exe" 0
  CreateShortcut "$DESKTOP\Clearcore.lnk" "$INSTDIR\Clearcore.exe" "--tray" "$INSTDIR\Clearcore.exe" 0

  ; Install and Register WaveRT Virtual Audio Driver
  DetailPrint "Instalando driver de áudio virtual Clearcore WaveRT..."
  IfFileExists "$INSTDIR\resources\driver\RealtimeNoise.inf" 0 SkipDriver
    ExecWait 'pnputil.exe /add-driver "$INSTDIR\resources\driver\RealtimeNoise.inf" /install'
  SkipDriver:

  ; Ensure virtual microphone is registered via PowerShell helper
  IfFileExists "$INSTDIR\resources\scripts\check-virtual-mic-windows.ps1" 0 SkipMicCheck
    ExecWait 'powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$INSTDIR\resources\scripts\check-virtual-mic-windows.ps1" -Recreate'
  SkipMicCheck:

SectionEnd

; ------------------------------------------------------------------------------
; Uninstaller Section
; ------------------------------------------------------------------------------
Section "Uninstall"
  ; Terminate running processes
  nsExec::Exec 'taskkill /F /IM Clearcore.exe /T'
  nsExec::Exec 'taskkill /F /IM realtime-noise-service.exe /T'
  Sleep 1000

  ; Remove shortcuts
  Delete "$DESKTOP\Clearcore.lnk"
  Delete "$SMPROGRAMS\Clearcore\Clearcore.lnk"
  Delete "$SMPROGRAMS\Clearcore\Desinstalar Clearcore.lnk"
  RMDir "$SMPROGRAMS\Clearcore"

  ; Remove Registry keys
  DeleteRegKey HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\Clearcore"
  DeleteRegKey HKCU "Software\Clearcore"

  ; Remove driver registration if inf present
  IfFileExists "$INSTDIR\resources\driver\RealtimeNoise.inf" 0 SkipDriverUninstall
    ExecWait 'pnputil.exe /delete-driver "$INSTDIR\resources\driver\RealtimeNoise.inf" /uninstall /force'
  SkipDriverUninstall:

  ; Remove application files
  RMDir /r "$INSTDIR"
SectionEnd
