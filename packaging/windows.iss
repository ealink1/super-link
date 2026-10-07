#ifndef Version
  #error Version is required
#endif
#ifndef Architecture
  #error Architecture is required
#endif
#ifndef SourceDir
  #error SourceDir is required
#endif
#ifndef OutputDirectory
  #error OutputDirectory is required
#endif
#ifndef IconFile
  #error IconFile is required
#endif
[Setup]
AppId={{D7106E93-B1AC-4CDF-B73C-A2506E7B37D1}
AppName=SuperLink
AppVersion={#Version}
AppPublisher=SuperLink
AppPublisherURL=https://github.com/ealink1/super-link
AppSupportURL=https://github.com/ealink1/super-link/issues
AppUpdatesURL=https://github.com/ealink1/super-link/releases
DefaultDirName={localappdata}\Programs\SuperLink
DefaultGroupName=SuperLink
PrivilegesRequired=lowest
#if Architecture == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif
OutputDir={#OutputDirectory}
OutputBaseFilename=SuperLink-{#Version}-windows-{#Architecture}-setup
SetupIconFile={#IconFile}
UninstallDisplayIcon={app}\superlink.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes
RestartApplications=no
[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs
Source: "{#IconFile}"; DestDir: "{app}"; DestName: "superlink.ico"; Flags: ignoreversion
[Icons]
Name: "{group}\SuperLink"; Filename: "{app}\superlink.exe"; WorkingDir: "{app}"; IconFilename: "{app}\superlink.ico"
Name: "{autodesktop}\SuperLink"; Filename: "{app}\superlink.exe"; WorkingDir: "{app}"; IconFilename: "{app}\superlink.ico"; Tasks: desktopicon
[Run]
Filename: "{app}\superlink.exe"; Description: "Launch SuperLink"; Flags: nowait postinstall skipifsilent
