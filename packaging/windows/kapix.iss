; Kapix Windows installer (Inno Setup 6).
;
; Built by .github/workflows/release.yml:
;   ISCC /DAppVersion=0.1.1 /DSourceDir=<abs>\target\x86_64-pc-windows-msvc\release /DOutputDir=<abs>\dist packaging\windows\kapix.iss
;
; Relative paths in this script resolve from this folder, so pass absolute paths on the command line.
;
; Installs per user by default (no admin prompt) into %LOCALAPPDATA%\Programs\Kapix. The "for all
; users" choice on the first page installs into Program Files instead. Config in %APPDATA%\Kapix
; and saved screenshots are never touched by the installer or uninstaller.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\target\release"
#endif

[Setup]
; Never change AppId: Windows uses it to recognise upgrades and the uninstall entry.
AppId={{239BEAE6-D31D-4955-9AAD-4533151CF45E}
AppName=Kapix
AppVersion={#AppVersion}
AppVerName=Kapix {#AppVersion}
AppPublisher=Ignatius Karel
AppPublisherURL=https://github.com/Aitemaa/Kapix
AppSupportURL=https://github.com/Aitemaa/Kapix/issues
AppCopyright=Copyright (c) Ignatius Karel
VersionInfoVersion={#AppVersion}
VersionInfoDescription=Kapix Setup
DefaultDirName={autopf}\Kapix
DefaultGroupName=Kapix
DisableProgramGroupPage=yes
DisableReadyPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
SetupIconFile=..\..\assets\icons\icon.ico
UninstallDisplayIcon={app}\kapix.exe
UninstallDisplayName=Kapix
WizardStyle=modern
Compression=lzma2/ultra
SolidCompression=yes
OutputDir={#OutputDir}
OutputBaseFilename=kapix-{#AppVersion}-windows-x64-setup
; Kapix runs in the background (tray), so let Setup close it before replacing files.
CloseApplications=yes
RestartApplications=no

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
Name: "startup"; Description: "Start Kapix when I sign in (keeps Ctrl+Shift+S / Ctrl+Shift+O hotkeys ready)"; GroupDescription: "Startup:"

[Files]
Source: "{#SourceDir}\kapix.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion

[Icons]
; The Start menu entry is what makes Kapix show up in Windows search.
Name: "{autoprograms}\Kapix"; Filename: "{app}\kapix.exe"; Comment: "Instant screenshot, annotation & OCR tool"
Name: "{autodesktop}\Kapix"; Filename: "{app}\kapix.exe"; Comment: "Instant screenshot, annotation & OCR tool"; Tasks: desktopicon

[Registry]
; "kapix" in the Run dialog (Win+R).
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\App Paths\kapix.exe"; ValueType: string; ValueName: ""; ValueData: "{app}\kapix.exe"; Flags: uninsdeletekey
; Matches kapix-win::startup (HKCU\...\Run\Kapix in per-user installs) so the tray menu stays in sync.
Root: HKA; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Kapix"; ValueData: """{app}\kapix.exe"""; Flags: uninsdeletevalue; Tasks: startup

[Run]
Filename: "{app}\kapix.exe"; Description: "Launch Kapix"; Flags: nowait postinstall skipifsilent

[Code]
// Asks a running Kapix tray daemon to unregister hotkeys, remove its tray icon, and exit so its
// exe can be replaced or removed. `kapix quit` posts WM_CLOSE asynchronously, so wait briefly.
procedure QuitRunningKapix();
var
  ExePath: String;
  ResultCode: Integer;
begin
  ExePath := ExpandConstant('{app}\kapix.exe');
  if FileExists(ExePath) then
  begin
    Exec(ExePath, 'quit', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
    Sleep(700);
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  QuitRunningKapix();
  Result := '';
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
    QuitRunningKapix();
end;
