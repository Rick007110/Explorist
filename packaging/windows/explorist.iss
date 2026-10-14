; Inno Setup script for the Windows installer. The release workflow runs
; it with VERSION set; paths are relative to this file.

#define AppVersion GetEnv("VERSION")

[Setup]
AppId={{8F3C2A61-5B7E-4D2A-9C1E-6A2F0B7D4E19}
AppName=Explorist
AppVersion={#AppVersion}
AppVerName=Explorist {#AppVersion}
AppPublisher=Rick007110
AppPublisherURL=https://github.com/Rick007110/Explorist
; Per-user, in %LOCALAPPDATA%\Programs: no administrator prompt, and the
; built-in updater can replace the exe.
PrivilegesRequired=lowest
DefaultDirName={autopf}\Explorist
DisableProgramGroupPage=yes
OutputDir=..\..\dist
OutputBaseFilename=explorist-{#AppVersion}-windows-x86_64-setup
SetupIconFile=..\..\assets\icon\icon.ico
UninstallDisplayIcon={app}\explorist.exe
UninstallDisplayName=Explorist
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
CloseApplications=yes

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "..\..\target\release\explorist.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\assets\fonts\OFL.txt"; DestDir: "{app}"; DestName: "FONTS-OFL.txt"
Source: "..\..\assets\icons\lucide\LICENSE"; DestDir: "{app}"; DestName: "ICONS-LICENSE.txt"

[Icons]
Name: "{autoprograms}\Explorist"; Filename: "{app}\explorist.exe"
Name: "{autodesktop}\Explorist"; Filename: "{app}\explorist.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\explorist.exe"; Description: "{cm:LaunchProgram,Explorist}"; Flags: nowait postinstall skipifsilent
