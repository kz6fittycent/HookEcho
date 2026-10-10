; Hook Echo Windows installer (Inno Setup 6). Built by desktop-builds.yml:
;   iscc /DAppVersion=x.y.z /DSourceDir=..\..\target\release packaging\windows\installer.iss
; Per-user install (no administrator password), like Desktop Scriptures.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\target\release"
#endif

[Setup]
AppId={{B6865259-AFBE-5ABF-A2CF-A7D856A4C3B2}
AppName=Hook Echo
AppVersion={#AppVersion}
AppPublisher=Hook Echo contributors
AppPublisherURL=https://github.com/kz6fittycent/HookEcho
DefaultDirName={localappdata}\Programs\Hook Echo
DefaultGroupName=Hook Echo
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
OutputDir=..\..\dist
OutputBaseFilename=Hook-Echo-{#AppVersion}-windows-x64-setup
SetupIconFile=hookecho.ico
UninstallDisplayIcon={app}\hookecho.exe
LicenseFile=..\..\LICENSE
Compression=lzma2
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern

[Files]
Source: "{#SourceDir}\hookecho.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\THIRD_PARTY_LICENSES.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Hook Echo"; Filename: "{app}\hookecho.exe"
Name: "{userdesktop}\Hook Echo"; Filename: "{app}\hookecho.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Run]
Filename: "{app}\hookecho.exe"; Description: "Launch Hook Echo"; Flags: nowait postinstall skipifsilent
