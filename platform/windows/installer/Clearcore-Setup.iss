; ==============================================================================
; Clearcore Desktop Application - Inno Setup Script
; ==============================================================================
#define MyAppName "Clearcore"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "Clearcore Team"
#define MyAppExeName "Clearcore.exe"

[Setup]
AppId={{5E979F88-124F-4E3F-B5DE-7681C30E39F0}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
OutputDir=..\..\..\release
OutputBaseFilename=Clearcore-Setup
Compression=lzma
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=admin
ArchitecturesInstallIn64BitMode=x64

[Languages]
Name: "brazilianportuguese"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "Iniciar automaticamente com o Windows (na bandeja)"; GroupDescription: "Inicialização:"

[Files]
Source: "..\..\..\release\Clearcore-win32-x64\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Parameters: "--tray"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Parameters: "--tray"; Tasks: desktopicon

[Registry]
; Autostart with Windows if selected
Root: HKLM; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#MyAppName}"; ValueData: """{app}\{#MyAppExeName}"" --tray"; Tasks: autostart

[Run]
; Auto-install WaveRT virtual audio driver via pnputil
Filename: "pnputil.exe"; Parameters: "/add-driver ""{app}\resources\driver\RealtimeNoise.inf"" /install"; Flags: runhidden waituntilterminated; StatusMsg: "Instalando driver de áudio virtual Clearcore WaveRT..."

; Verify and create virtual microphone device
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\resources\scripts\check-virtual-mic-windows.ps1"" -Recreate"; Flags: runhidden waituntilterminated; StatusMsg: "Configurando microfone virtual..."

; Launch on finish
Filename: "{app}\{#MyAppExeName}"; Parameters: "--tray"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "taskkill.exe"; Parameters: "/F /IM {#MyAppExeName} /T"; Flags: runhidden
Filename: "taskkill.exe"; Parameters: "/F /IM realtime-noise-service.exe /T"; Flags: runhidden
Filename: "powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{app}\resources\scripts\uninstall-windows.ps1"""; Flags: runhidden
Filename: "pnputil.exe"; Parameters: "/delete-driver ""{app}\resources\driver\RealtimeNoise.inf"" /uninstall /force"; Flags: runhidden
