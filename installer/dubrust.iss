; DubRust installer script (Inno Setup 6)
; Built by scripts/package.ps1, which stages files into dist/payload first.

#define MyAppName "DubRust"
#define MyAppPublisher "Arcis (arcisq)"
#define MyAppURL "https://github.com/arcisq/dubrust"
#define MyAppExeName "dubrust.exe"
#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif

[Setup]
AppId={{9F2B7C41-6D3A-4E58-9A21-7C4E8D5B10F3}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}/issues
AppUpdatesURL={#MyAppURL}/releases
VersionInfoVersion={#MyAppVersion}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
LicenseFile=..\LICENSE
OutputDir=..\dist
OutputBaseFilename=DubRust-{#MyAppVersion}-windows-x64-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName} {#MyAppVersion}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
AllowNoIcons=yes
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
; Application binary (static CRT: no Visual C++ Redistributable needed)
Source: "..\dist\payload\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; Bundled ffmpeg tools so the app works out of the box, no PATH setup
Source: "..\dist\payload\ffmpeg.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\payload\ffprobe.exe"; DestDir: "{app}"; Flags: ignoreversion
; Documentation and licenses
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\CHANGELOG.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\THIRD-PARTY-LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\dist\payload\LICENSE-ffmpeg.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Only build leftovers; downloaded model weights in %APPDATA%\dubrust stay untouched
Type: filesandordirs; Name: "{app}\ffmpeg"
