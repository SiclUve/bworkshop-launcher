; Installateur Inno de test pour les tests d'intégration du launcher.
; Installé pour l'utilisateur seul, sans droits admin, dans un dossier dédié.
; Construire avec build.ps1 (versions 1.0.0 et 1.1.0).

#ifndef AppVersion
  #define AppVersion "1.0.0"
#endif

[Setup]
AppId={{0B5E7A31-6C2D-4E8F-9A10-B4C3D2E1F007}
AppName=Bworkshop Test Tool
AppVersion={#AppVersion}
AppPublisher=Bworkshop
DefaultDirName={localappdata}\BworkshopLauncherTest\TestTool
PrivilegesRequired=lowest
DisableDirPage=yes
DisableProgramGroupPage=yes
DisableReadyPage=yes
CreateUninstallRegKey=yes
UninstallDisplayIcon={app}\testtool.exe
CloseApplications=yes
OutputDir=.
OutputBaseFilename=TestTool_Setup_{#AppVersion}
Compression=lzma2/max
SolidCompression=yes

[Files]
Source: "testtool.exe"; DestDir: "{app}"; Flags: ignoreversion
