; Inno Setup script for CrawlStat.
;
; Produces a single CrawlStat-<version>-windows-setup.exe: the familiar
; double-click installer, with Start-menu and optional desktop shortcuts and a
; proper entry in Add/Remove Programs.
;
; Why Inno Setup and not MSIX: an MSIX package *cannot be installed at all*
; without a trusted signature, so it is useless for testing before a
; certificate exists. An Inno .exe installs either way — unsigned it shows a
; SmartScreen warning the user can click past, signed it shows nothing.
;
; Inno Setup is preinstalled on GitHub's windows runners.
;
; Build:  iscc /DMyAppVersion=1.0.0 packaging\windows\crawlstat.iss

#ifndef MyAppVersion
  #define MyAppVersion "0.0.0"
#endif

#define MyAppName "CrawlStat"
#define MyAppPublisher "CrawlStat"
#define MyAppURL "https://crawlstat.com"
#define MyAppExeName "crawlstat.exe"

[Setup]
; A stable GUID: changing it would make an upgrade install alongside the old
; version instead of replacing it.
AppId={{8D3F6A21-5C47-4E8B-9A17-2F4B6C8E1D90}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/docs
AppUpdatesURL={#MyAppURL}/download
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
; Per-user install by default, so no UAC prompt and no administrator needed —
; one less thing between a tester and the app.
PrivilegesRequiredOverridesAllowed=dialog
PrivilegesRequired=lowest
OutputDir=..\..\dist
OutputBaseFilename={#MyAppName}-{#MyAppVersion}-windows-setup
SetupIconFile=..\..\app\windows\runner\resources\app_icon.ico
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
; `x64` rather than `x64compatible`: the latter needs Inno Setup 6.3+
; and the hosted runners do not reliably have it.
ArchitecturesAllowed=x64
ArchitecturesInstallIn64BitMode=x64
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a &desktop shortcut"; GroupDescription: "Shortcuts:"

[Files]
; The whole Flutter release folder: the .exe, the Flutter DLL, the Rust
; bridge DLL and the data directory. Missing any one of them and the app
; starts and then dies, so this is deliberately a whole-folder copy.
Source: "..\..\app\build\windows\x64\runner\Release\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "Launch {#MyAppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Crawl databases live in %APPDATA%\CrawlStat and are the user's data, so they
; are deliberately left alone. Only the installed program is removed.
Type: filesandordirs; Name: "{app}"
