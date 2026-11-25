//! Détection des outils installés dans le registre (utilisateur seul).
//!
//! Les installateurs Inno Setup « pour l'utilisateur » écrivent dans
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{AppId}_is1`.
//! La source du registre est un trait : les tests utilisent une fausse source.

use serde::Serialize;

pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallEntry {
    /// Nom de la sous-clé, par exemple `{6F1B…}_is1`.
    pub key: String,
    pub display_name: Option<String>,
    pub display_version: Option<String>,
    pub install_location: Option<String>,
    pub uninstall_string: Option<String>,
    pub display_icon: Option<String>,
}

impl UninstallEntry {
    pub fn is_inno(&self) -> bool {
        self.key.to_ascii_lowercase().ends_with("_is1")
    }
}

pub trait RegistrySource: Send + Sync {
    fn entries(&self) -> Vec<UninstallEntry>;
}

/// Cherche un outil : d'abord par son AppId Inno, sinon par le nom affiché
/// (« Nom » ou « Nom version X », la forme qu'Inno écrit par défaut).
/// La recherche par nom couvre toutes les clés, car NSIS n'écrit pas de clé `_is1`.
/// À nom égal, une clé Inno passe avant les autres.
pub fn find<'a>(entries: &'a [UninstallEntry], app_id: Option<&str>, name: &str) -> Option<&'a UninstallEntry> {
    if let Some(id) = app_id {
        let key = format!("{id}_is1");
        if let Some(e) = entries.iter().find(|e| e.key.eq_ignore_ascii_case(&key)) {
            return Some(e);
        }
    }
    let wanted = name.trim().to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    let with_version = format!("{wanted} version ");
    let matches = |e: &&UninstallEntry| {
        e.display_name.as_deref().is_some_and(|d| {
            let d = d.trim().to_lowercase();
            d == wanted || d.starts_with(&with_version)
        })
    };
    entries
        .iter()
        .filter(|e| e.is_inno())
        .find(matches)
        .or_else(|| entries.iter().filter(|e| !e.is_inno()).find(matches))
}

/// Lecture réelle de `HKCU\…\Uninstall`.
pub struct WindowsRegistry;

#[cfg(windows)]
impl RegistrySource for WindowsRegistry {
    fn entries(&self) -> Vec<UninstallEntry> {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        use winreg::RegKey;

        let Ok(root) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(UNINSTALL_KEY, KEY_READ) else {
            return Vec::new();
        };
        let read = |k: &RegKey, name: &str| -> Option<String> {
            k.get_value::<String, _>(name).ok().filter(|v| !v.trim().is_empty())
        };
        root.enum_keys()
            .filter_map(Result::ok)
            .filter_map(|name| {
                let k = root.open_subkey_with_flags(&name, KEY_READ).ok()?;
                Some(UninstallEntry {
                    display_name: read(&k, "DisplayName"),
                    display_version: read(&k, "DisplayVersion"),
                    install_location: read(&k, "InstallLocation"),
                    uninstall_string: read(&k, "UninstallString"),
                    display_icon: read(&k, "DisplayIcon"),
                    key: name,
                })
            })
            .collect()
    }
}

#[cfg(not(windows))]
impl RegistrySource for WindowsRegistry {
    fn entries(&self) -> Vec<UninstallEntry> {
        Vec::new()
    }
}

/// Fausse source pour les tests.
pub struct StaticRegistry(pub Vec<UninstallEntry>);

impl RegistrySource for StaticRegistry {
    fn entries(&self) -> Vec<UninstallEntry> {
        self.0.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: &str, name: &str, version: &str) -> UninstallEntry {
        UninstallEntry {
            key: key.into(),
            display_name: Some(name.into()),
            display_version: Some(version.into()),
            install_location: Some(r"C:\Outils\X\".into()),
            uninstall_string: Some(r#""C:\Outils\X\unins000.exe""#.into()),
            display_icon: Some(r"C:\Outils\X\X.exe".into()),
        }
    }

    fn registre() -> StaticRegistry {
        StaticRegistry(vec![
            entry("{6F1B7D2A-3C4E-4B8F-9A10-F0A55157F0C9}_is1", "Assistant Forge", "0.1.0"),
            entry("{AAAA}_is1", "Grimoire Boss version 0.1.0", "0.1.0"),
            entry("{BBBB}_is1", "Grimoire version 0.2.0", "0.2.0"),
            entry("Bworklingo", "Bworklingo", "0.2.0"), // clé NSIS
            entry("Doublon", "Grimoire", "0.0.1"),      // même nom, pas Inno
        ])
    }

    #[test]
    fn par_app_id() {
        let e = registre().entries();
        let f = find(&e, Some("{6f1b7d2a-3c4e-4b8f-9a10-f0a55157f0c9}"), "Peu importe").unwrap();
        assert_eq!(f.display_version.as_deref(), Some("0.1.0"));
    }

    #[test]
    fn par_nom_affiche_sans_confusion_entre_prefixes() {
        let e = registre().entries();
        assert_eq!(find(&e, None, "Grimoire").unwrap().key, "{BBBB}_is1");
        assert_eq!(find(&e, None, "grimoire boss").unwrap().key, "{AAAA}_is1");
        assert!(find(&e, None, "Grim").is_none());
    }

    #[test]
    fn app_id_inconnu_puis_nom_puis_rien() {
        let e = registre().entries();
        // À nom égal, la clé Inno passe avant l'autre.
        assert_eq!(find(&e, Some("{ZZZZ}"), "Grimoire").unwrap().key, "{BBBB}_is1");
        assert!(find(&e, Some("{ZZZZ}"), "Inconnu").is_none());
    }

    #[test]
    fn cle_nsis_trouvee_par_le_nom() {
        let e = registre().entries();
        assert_eq!(find(&e, None, "Bworklingo").unwrap().key, "Bworklingo");
    }
}
