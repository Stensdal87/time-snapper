; Inno Setup-script til Windows-installeren. Bygges af GitHub Actions
; (.github/workflows/release.yml) med: ISCC /DAppVersion=x.y.z installer\time-snapper.iss
;
; Installerer kun for den aktuelle bruger, så der ikke kræves administratorrettigheder.
; Data i %APPDATA%\TimeSnapper bliver liggende ved afinstallation og opdatering.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
; AppId må aldrig ændres; den bruges til at genkende en eksisterende installation ved opdatering.
AppId={{421CB1D1-D7AC-49F7-8A0A-E05F04F75468}
AppName=Time Snapper
AppVersion={#AppVersion}
AppVerName=Time Snapper {#AppVersion}
AppPublisher=Stensdal87
AppPublisherURL=https://github.com/Stensdal87/time-snapper
DefaultDirName={localappdata}\Programs\Time Snapper
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=lowest
OutputDir=..\dist
OutputBaseFilename=TimeSnapper-Setup-{#AppVersion}
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\TimeSnapper.exe
UninstallDisplayName=Time Snapper
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Luk et kørende Time Snapper ved opdatering.
CloseApplications=yes

[Languages]
Name: "danish"; MessagesFile: "compiler:Languages\Danish.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "Start Time Snapper, når jeg logger på Windows"; GroupDescription: "Andet:"; Flags: unchecked

[Files]
Source: "..\target\release\time_snapper.exe"; DestDir: "{app}"; DestName: "TimeSnapper.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Time Snapper"; Filename: "{app}\TimeSnapper.exe"
Name: "{autodesktop}\Time Snapper"; Filename: "{app}\TimeSnapper.exe"; Tasks: desktopicon
Name: "{userstartup}\Time Snapper"; Filename: "{app}\TimeSnapper.exe"; Tasks: autostart

[Run]
Filename: "{app}\TimeSnapper.exe"; Description: "{cm:LaunchProgram,Time Snapper}"; Flags: nowait postinstall skipifsilent
