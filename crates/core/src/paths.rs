//! Emplacements des fichiers du launcher.
//!
//! - Réglages et dispositions : `%APPDATA%\BworkshopLauncher\`
//! - Journal, téléchargements, cache : `%LOCALAPPDATA%\BworkshopLauncher\`

use std::env;
use std::path::{Path, PathBuf};

pub const APP_DIR: &str = "BworkshopLauncher";

#[derive(Debug, thiserror::Error)]
pub enum PathsError {
    #[error("variable d'environnement {0} absente")]
    MissingVar(&'static str),
}

#[derive(Debug, Clone)]
pub struct AppPaths {
    /// Dossier itinérant (réglages, dispositions).
    pub config_dir: PathBuf,
    /// Dossier local (journal, téléchargements, cache).
    pub data_dir: PathBuf,
}

impl AppPaths {
    /// Lit `%APPDATA%` et `%LOCALAPPDATA%`.
    pub fn from_env() -> Result<Self, PathsError> {
        let roaming = env::var_os("APPDATA").ok_or(PathsError::MissingVar("APPDATA"))?;
        let local = env::var_os("LOCALAPPDATA").ok_or(PathsError::MissingVar("LOCALAPPDATA"))?;
        Ok(Self::from_roots(Path::new(&roaming), Path::new(&local)))
    }

    /// Construit les chemins à partir de racines données (utile pour les tests).
    pub fn from_roots(roaming: &Path, local: &Path) -> Self {
        Self {
            config_dir: roaming.join(APP_DIR),
            data_dir: local.join(APP_DIR),
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn layouts_file(&self) -> PathBuf {
        self.config_dir.join("layouts.json")
    }

    pub fn widgets_file(&self) -> PathBuf {
        self.config_dir.join("widgets.json")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.data_dir.join("downloads")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chemins_attendus() {
        let p = AppPaths::from_roots(Path::new(r"C:\R"), Path::new(r"C:\L"));
        assert_eq!(p.settings_file(), Path::new(r"C:\R\BworkshopLauncher\settings.json"));
        assert_eq!(p.logs_dir(), Path::new(r"C:\L\BworkshopLauncher\logs"));
        assert_eq!(p.downloads_dir(), Path::new(r"C:\L\BworkshopLauncher\downloads"));
    }
}
