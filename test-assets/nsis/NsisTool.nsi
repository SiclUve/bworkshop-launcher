; Installateur NSIS de test pour les tests d'intégration du launcher.
; Installé pour l'utilisateur seul, sans droits admin, dans un dossier dédié.
; Construire avec build.ps1 (versions 1.0.0 et 1.1.0).

!ifndef VERSION
  !define VERSION "1.0.0"
!endif
!define NAME "Bworkshop NSIS Test Tool"
!define REGKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\BworkshopNsisTestTool"

Unicode true
Name "${NAME}"
OutFile "NsisTool_Setup_${VERSION}.exe"
RequestExecutionLevel user
InstallDir "$LOCALAPPDATA\BworkshopLauncherTest\NsisTool"
SetCompressor /SOLID lzma

Section
  SetOutPath "$INSTDIR"
  File "testtool.exe"
  WriteUninstaller "$INSTDIR\uninstall.exe"
  WriteRegStr HKCU "${REGKEY}" "DisplayName" "${NAME}"
  WriteRegStr HKCU "${REGKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${REGKEY}" "Publisher" "Bworkshop"
  WriteRegStr HKCU "${REGKEY}" "DisplayIcon" "$INSTDIR\testtool.exe"
  WriteRegStr HKCU "${REGKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${REGKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${REGKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\testtool.exe"
  Delete "$INSTDIR\uninstall.exe"
  RMDir "$INSTDIR"
  RMDir "$INSTDIR\.."
  DeleteRegKey HKCU "${REGKEY}"
SectionEnd
