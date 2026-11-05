; Instalador de Rukes (Inno Setup 6). Instalación por usuario, sin administrador (RNF-033).
; Lo genera scripts/package.ps1:  ISCC /DAppVersion=1.0.0 /Odist packaging\rukes.iss

#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif
#ifndef ExePath
  #define ExePath "..\target\release\rukes.exe"
#endif

[Setup]
AppId={{6D3B0F0E-7C55-4E1B-9A5E-52554B455331}
AppName=Rukes
AppVersion={#AppVersion}
AppVerName=Rukes {#AppVersion}
AppPublisher=OscarP29
DefaultDirName={localappdata}\Programs\Rukes
DefaultGroupName=Rukes
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
OutputBaseFilename=Rukes-{#AppVersion}-setup-x64
SetupIconFile=..\assets\rukes.ico
UninstallDisplayIcon={app}\rukes.exe
Compression=lzma2/max
SolidCompression=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19045
WizardStyle=modern
LicenseFile=..\LICENSE-MIT
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Files]
Source: "{#ExePath}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\usuario.md"; DestDir: "{app}"; DestName: "GUIA.md"; Flags: ignoreversion
Source: "..\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Rukes"; Filename: "{app}\rukes.exe"

[Registry]
; El inicio con Windows lo gestiona Rukes (start_with_windows en config.toml, RF-008); al
; desinstalar se quita la entrada si existe.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: none; \
  ValueName: "Rukes"; Flags: uninsdeletevalue

[Run]
Filename: "{app}\rukes.exe"; Description: "{cm:LaunchProgram,Rukes}"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM rukes.exe /F"; Flags: runhidden; RunOnceId: "StopRukes"

; La configuración del usuario (%APPDATA%\Rukes) no se borra al desinstalar.
