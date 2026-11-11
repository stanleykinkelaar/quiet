; Inno Setup script for Quiet. Per-user install, no admin rights needed.
; Build the exe first (cargo build --release), then compile this file with
; Inno Setup 6:  "C:\Program Files (x86)\Inno Setup 6\ISCC.exe" installer\quiet.iss

#define AppName "Quiet"
#define AppVersion "0.1.0"
#define AppExe "quiet.exe"

[Setup]
AppId={{8F0B6C1E-4C2B-4F5D-9A63-3E7A1C2D9B10}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
DefaultDirName={localappdata}\Programs\{#AppName}
DisableDirPage=yes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; The app holds this mutex while it runs, so Setup can ask you to quit it.
AppMutex=QuietAppRunning
OutputDir=..\target\installer
OutputBaseFilename=QuietSetup-{#AppVersion}
UninstallDisplayIcon={app}\{#AppExe}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Files]
Source: "..\target\release\{#AppExe}"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
; The AppUserModelID matches the one the app registers, so reminders are
; shown as coming from Quiet.
Name: "{userprograms}\{#AppName}"; Filename: "{app}\{#AppExe}"; AppUserModelID: "Quiet.Tasks"

[Registry]
; The app adds itself to the Run key (start with Windows). Remove that on
; uninstall, together with its notification registration.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; ValueName: "Quiet"; Flags: uninsdeletevalue
Root: HKCU; Subkey: "Software\Classes\AppUserModelId\Quiet.Tasks"; Flags: uninsdeletekey

[Run]
Filename: "{app}\{#AppExe}"; Description: "Start {#AppName}"; Flags: nowait postinstall skipifsilent
