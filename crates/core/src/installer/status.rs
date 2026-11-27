//! État stable d'un outil (hors opérations en cours, suivies par l'application) :
//! Non installé, Installé, ou Mise à jour disponible.

use super::known_apps;
use super::registry::{self, UninstallEntry};
use super::version::is_newer;
use crate::catalog::Tool;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum InstallStatus {
    NotInstalled,
    Installed { version: Option<String> },
    UpdateAvailable { installed: String, available: String },
}

pub fn status_of(entry: Option<&UninstallEntry>, catalog_version: Option<&str>) -> InstallStatus {
    let Some(entry) = entry else {
        return InstallStatus::NotInstalled;
    };
    let installed = entry.display_version.clone();
    match (installed.as_deref(), catalog_version) {
        (Some(i), Some(a)) if is_newer(a, i) => {
            InstallStatus::UpdateAvailable { installed: i.to_string(), available: a.to_string() }
        }
        _ => InstallStatus::Installed { version: installed },
    }
}

/// Entrée du registre correspondant à un outil du catalogue.
/// Mac : par l'identifiant d'application de l'outil, sinon par le nom affiché.
pub fn entry_for<'a>(entries: &'a [UninstallEntry], tool: &Tool) -> Option<&'a UninstallEntry> {
    if cfg!(target_os = "macos") {
        let key = super::mac::bundle_id_for(&tool.id).map(super::mac::key_for);
        if let Some(e) = key.and_then(|k| entries.iter().find(|e| e.key == k)) {
            return Some(e);
        }
        return registry::find(entries, None, &tool.name);
    }
    let app_id = known_apps::find(&tool.id).map(|a| a.app_id);
    registry::find(entries, app_id, &tool.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(v: Option<&str>) -> UninstallEntry {
        UninstallEntry { key: "{X}_is1".into(), display_version: v.map(String::from), ..Default::default() }
    }

    #[test]
    fn etats() {
        assert_eq!(status_of(None, Some("1.0.0")), InstallStatus::NotInstalled);
        assert_eq!(
            status_of(Some(&e(Some("0.9.0"))), Some("0.10.0")),
            InstallStatus::UpdateAvailable { installed: "0.9.0".into(), available: "0.10.0".into() }
        );
        assert_eq!(
            status_of(Some(&e(Some("1.0.0"))), Some("1.0.0")),
            InstallStatus::Installed { version: Some("1.0.0".into()) }
        );
        // Version locale plus récente que le catalogue : pas de « mise à jour » vers l'arrière.
        assert_eq!(
            status_of(Some(&e(Some("2.0.0"))), Some("1.0.0")),
            InstallStatus::Installed { version: Some("2.0.0".into()) }
        );
        assert_eq!(status_of(Some(&e(None)), Some("1.0.0")), InstallStatus::Installed { version: None });
    }
}
