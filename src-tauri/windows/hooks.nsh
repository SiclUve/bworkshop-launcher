; Ajouts au programme d'installation (NSIS) du launcher.

; Après la désinstallation : retirer le lancement au démarrage de Windows, s'il était activé.
; (Sinon Windows chercherait au démarrage un programme qui n'existe plus.)
; Lors d'une mise à jour, le launcher le remet lui-même au démarrage suivant si le réglage est actif.
!macro NSIS_HOOK_POSTUNINSTALL
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Bworkshop Launcher"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "bworkshop-launcher"
!macroend
