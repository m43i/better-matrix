; The Windows installer (Inno Setup 6): `ISCC /DVersion=1.2.3 packaging\windows\better-matrix.iss`
; after a release build, as package.ps1 runs it. It installs for the current user only, into the
; folder the self-update replaces the executable in (it must stay writable: no Program Files),
; adds the Start menu entry, claims `matrix:` links and registers an uninstaller.

#ifndef Version
  #error Pass the version: ISCC /DVersion=1.2.3
#endif
; Windows' file version is numeric: 1.2.3-rc.1 becomes 1.2.3.
#define FileVersion Copy(Version, 1, Pos("-", Version + "-") - 1)
#define Root "..\.."

[Setup]
; Never change the AppId: it ties upgrades and the uninstaller to earlier installs. The self-update
; (apps/desktop/src/platform/updates/install.ps1) writes the version under the same ID.
AppId={{DB776B2C-C930-4C78-B82D-3BACD4B141C9}
AppName=Better Matrix
AppVersion={#Version}
AppVerName=Better Matrix {#Version}
AppPublisher=Better Matrix contributors
AppPublisherURL=https://github.com/m43i/better-matrix
AppSupportURL=https://github.com/m43i/better-matrix/issues
AppUpdatesURL=https://github.com/m43i/better-matrix/releases
AppCopyright=Copyright (c) 2026 Malte Eiting and the Better Matrix contributors
VersionInfoVersion={#FileVersion}
; The text versions keep a pre-release, like the executable's (apps/desktop/build.rs); code
; signing checks the product name and version (packaging/windows/signpath/installer.xml).
VersionInfoTextVersion={#Version}
VersionInfoProductName=Better Matrix
VersionInfoProductTextVersion={#Version}
; Per user, no administrator rights; {autopf} is then %LOCALAPPDATA%\Programs, the folder the
; README's earlier install line used, so running the installer adopts such an install.
PrivilegesRequired=lowest
DefaultDirName={autopf}\Better Matrix
DisableDirPage=auto
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
SetupIconFile={#Root}\packaging\icons\better-matrix.ico
UninstallDisplayIcon={app}\better-matrix.ico
UninstallDisplayName=Better Matrix
WizardStyle=modern
; A running app is closed (through the Restart Manager) before its executable is replaced.
CloseApplications=yes
RestartApplications=no
Compression=lzma2/max
SolidCompression=yes
OutputDir={#Root}\dist\out
OutputBaseFilename=better-matrix-windows-x86_64-setup
#ifdef Sign
; package.ps1 defines Sign and passes the signtool command (/Ssigntool=…) when a certificate is set:
; the installer and its uninstaller are then signed while they are built.
SignTool=signtool
SignedUninstaller=yes
#endif
ShowLanguageDialog=auto

; The app's own languages; the wizard follows the system's.
[Languages]
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "de"; MessagesFile: "compiler:Languages\German.isl"
Name: "fr"; MessagesFile: "compiler:Languages\French.isl"

[CustomMessages]
en.MatrixLink=Matrix link
de.MatrixLink=Matrix-Link
fr.MatrixLink=Lien Matrix

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

; The same files as the ZIP the self-update downloads.
[Files]
Source: "{#Root}\target\release\better-matrix.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\packaging\icons\better-matrix.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Root}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Better Matrix"; Filename: "{app}\better-matrix.exe"; IconFilename: "{app}\better-matrix.ico"
Name: "{autodesktop}\Better Matrix"; Filename: "{app}\better-matrix.exe"; IconFilename: "{app}\better-matrix.ico"; Tasks: desktopicon

; `matrix:` links open the app (ADR 0001). Uninstalling removes them in [Code], only while they
; still open this install.
[Registry]
Root: HKCU; Subkey: "Software\Classes\matrix"; ValueType: string; ValueData: "URL:{cm:MatrixLink}"
Root: HKCU; Subkey: "Software\Classes\matrix"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\matrix\DefaultIcon"; ValueType: string; ValueData: """{app}\better-matrix.ico"""
Root: HKCU; Subkey: "Software\Classes\matrix\shell\open\command"; ValueType: string; ValueData: """{app}\better-matrix.exe"" ""%1"""

[Run]
Filename: "{app}\better-matrix.exe"; Description: "{cm:LaunchProgram,Better Matrix}"; Flags: nowait postinstall skipifsilent

; An interrupted self-update can leave its staging folder behind. Accounts, history and keys live
; in the user's profile, not here, and stay.
[UninstallDelete]
Type: filesandordirs; Name: "{app}\.better-matrix-update-*"

[Code]
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
begin
  // Another app may have claimed matrix: links since; leave its handler alone.
  if (CurUninstallStep = usUninstall) and
    RegQueryStringValue(HKCU, 'Software\Classes\matrix\shell\open\command', '', Command) and
    (CompareText(Command, '"' + ExpandConstant('{app}\better-matrix.exe') + '" "%1"') = 0) then
    RegDeleteKeyIncludingSubkeys(HKCU, 'Software\Classes\matrix');
end;
