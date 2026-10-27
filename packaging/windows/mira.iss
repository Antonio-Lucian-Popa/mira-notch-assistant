#ifndef MiraVersion
  #define MiraVersion "0.3.0"
#endif
[Setup]
AppId={{A7D45C2D-6D0E-4C4E-9B20-122C365E49E0}
AppName=Mira
AppVersion={#MiraVersion}
AppPublisher=Mira contributors
DefaultDirName={localappdata}\Programs\Mira
DefaultGroupName=Mira
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=Output
OutputBaseFilename=mira_{#MiraVersion}_windows_x64_setup
Compression=lzma2
SolidCompression=yes
CloseApplications=yes
UninstallDisplayIcon={app}\mira-win.exe

[Tasks]
Name: "autostart"; Description: "Start Mira when I sign in"; Flags: unchecked

[Files]
Source: "..\..\target\release\mira-win.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"

[Icons]
Name: "{group}\Mira"; Filename: "{app}\mira-win.exe"
Name: "{group}\Uninstall Mira"; Filename: "{uninstallexe}"
Name: "{userstartup}\Mira"; Filename: "{app}\mira-win.exe"; Tasks: autostart

[Run]
Filename: "{app}\mira-win.exe"; Description: "Launch Mira"; Flags: nowait postinstall skipifsilent
