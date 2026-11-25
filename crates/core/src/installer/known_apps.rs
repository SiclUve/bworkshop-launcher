//! Table de correspondance identifiant du catalogue → AppId Inno Setup.
//!
//! Pour ajouter un outil : récupérer son `AppId` dans le script Inno (.iss), ou
//! chercher la clé `{…}_is1` dans
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall` après l'avoir installé.
//! Sans entrée ici, le launcher cherche l'outil par son nom affiché.

pub struct KnownApp {
    pub id: &'static str,
    /// AppId avec ses accolades, tel qu'il apparaît dans la clé `{AppId}_is1`.
    pub app_id: &'static str,
    /// Exécutable principal, relatif au dossier d'installation, si `DisplayIcon` ne suffit pas.
    pub main_exe: Option<&'static str>,
}

pub const KNOWN_APPS: &[KnownApp] = &[
    KnownApp { id: "simulateur-harebourg", app_id: "{B7C2D1E4-5F63-4A8B-9C10-2E7A4D6F8B31}", main_exe: None },
    KnownApp { id: "assistant-forge", app_id: "{6F1B7D2A-3C4E-4B8F-9A10-F0A55157F0C9}", main_exe: None },
    KnownApp { id: "grimoire-boss", app_id: "{4E1B7C2A-9D35-4F80-B6A2-7C51E0D93F48}", main_exe: None },
    // Relevés le 24/11/2026 dans le registre d'une machine où ces outils étaient installés.
    KnownApp { id: "grimoire", app_id: "{8D6B2A1E-5B17-4A84-9F1A-2B9E6D4C1201}", main_exe: None },
    KnownApp { id: "skin-helper", app_id: "{8E9B7D4C-4B21-4F7D-A49E-2D8F1C9E4A10}", main_exe: None },
];

pub fn find(id: &str) -> Option<&'static KnownApp> {
    KNOWN_APPS.iter().find(|a| a.id == id)
}
