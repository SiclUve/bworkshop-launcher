//! Réglages du launcher et favoris.
//!
//! Fichier : `%APPDATA%\BworkshopLauncher\settings.json`, JSON versionné.
//! - Champ absent : valeur par défaut (compatibilité ascendante).
//! - Champ inconnu : ignoré.
//! - Fichier illisible : mis de côté (`.corrupt-…`) et remplacé par les valeurs par défaut.

use crate::storage::{self, StorageError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

/// Version actuelle du format du fichier de réglages.
pub const SETTINGS_VERSION: u32 = 1;

/// Motif par défaut pour reconnaître la fenêtre de Dofus (par son titre).
pub const DEFAULT_DOFUS_PATTERN: &str = "Dofus";

/// Nombre de dispositions qui ont un raccourci.
pub const LAYOUT_HOTKEY_COUNT: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    Fr,
    En,
}

/// Raccourcis globaux. `None` = raccourci désactivé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HotkeyBindings {
    pub toggle_panel: Option<String>,
    pub toggle_hide_all: Option<String>,
    pub toggle_lock: Option<String>,
    /// Dispositions 1 à 4.
    pub layouts: Vec<Option<String>>,
    /// Raccourci d'un widget précis : identifiant de l'outil → raccourci.
    pub widgets: BTreeMap<String, String>,
}

impl Default for HotkeyBindings {
    fn default() -> Self {
        Self {
            toggle_panel: Some("Alt+B".into()),
            toggle_hide_all: Some("Alt+H".into()),
            toggle_lock: Some("Alt+L".into()),
            layouts: (1..=LAYOUT_HOTKEY_COUNT).map(|i| Some(format!("Alt+{i}"))).collect(),
            widgets: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub language: Language,
    /// Lancer au démarrage de Windows (désactivé par défaut).
    pub autostart: bool,
    /// Le titre de la fenêtre de Dofus contient ce texte.
    pub dofus_title_pattern: String,
    /// Suivre la fenêtre de Dofus (désactivé par défaut) : les widgets la suivent quand
    /// elle bouge, s'y aimantent, et peuvent se cacher quand le jeu n'est pas au premier plan.
    pub follow_dofus: bool,
    /// Aimantation aux bords de Dofus (seulement si `follow_dofus`).
    pub snapping: bool,
    /// Cacher widgets et barre quand Dofus n'est pas au premier plan (seulement si `follow_dofus`).
    pub only_when_dofus_foreground: bool,
    pub reopen_last_layout: bool,
    pub hotkeys: HotkeyBindings,
    /// Identifiants des outils favoris, dans l'ordre choisi.
    pub favorites: Vec<String>,
    pub tutorial_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            language: Language::Fr,
            autostart: false,
            dofus_title_pattern: DEFAULT_DOFUS_PATTERN.into(),
            follow_dofus: false,
            snapping: true,
            only_when_dofus_foreground: false,
            reopen_last_layout: false,
            hotkeys: HotkeyBindings::default(),
            favorites: Vec::new(),
            tutorial_done: false,
        }
    }
}

impl Settings {
    /// Corrige les valeurs incohérentes sans jamais échouer.
    pub fn normalized(mut self) -> Self {
        self.version = SETTINGS_VERSION;
        let pattern = self.dofus_title_pattern.trim();
        self.dofus_title_pattern = if pattern.is_empty() {
            DEFAULT_DOFUS_PATTERN.into()
        } else {
            pattern.into()
        };
        let mut seen = std::collections::HashSet::new();
        self.favorites.retain(|f| !f.is_empty() && seen.insert(f.clone()));
        self.hotkeys.layouts.resize(LAYOUT_HOTKEY_COUNT, None);
        self
    }
}

#[derive(Debug, Clone)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Charge les réglages. Ne plante jamais : au pire, valeurs par défaut.
    pub fn load(&self) -> Settings {
        match fs::read(&self.path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                log::info!("Aucun fichier de réglages : valeurs par défaut");
                Settings::default()
            }
            Err(e) => {
                // Fichier verrouillé ou droits insuffisants : on ne l'écrase pas.
                log::warn!("Lecture des réglages impossible ({e}) : valeurs par défaut");
                Settings::default()
            }
            Ok(bytes) => match parse(&bytes) {
                Ok(s) => s.normalized(),
                Err(e) => {
                    log::warn!("Réglages illisibles ({e}) : ancien fichier mis de côté, valeurs par défaut");
                    let defaults = Settings::default();
                    // Copie de côté ratée : on n'écrase pas l'original (il reste récupérable à la main).
                    match storage::backup_corrupt(&self.path) {
                        Err(e) => log::warn!("Impossible de mettre de côté le fichier corrompu ({e}) : il est laissé tel quel"),
                        Ok(_) => {
                            if let Err(e) = self.save(&defaults) {
                                log::warn!("Impossible d'écrire les réglages par défaut : {e}");
                            }
                        }
                    }
                    defaults
                }
            },
        }
    }

    pub fn save(&self, settings: &Settings) -> Result<(), StorageError> {
        storage::write_json_atomic(&self.path, settings)
    }
}

fn parse(bytes: &[u8]) -> Result<Settings, String> {
    let value: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let value = migrate(value)?;
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// Fait passer un ancien format au format actuel.
fn migrate(mut value: Value) -> Result<Value, String> {
    let obj = value.as_object_mut().ok_or("la racine n'est pas un objet")?;
    let version = obj.get("version").and_then(Value::as_u64).unwrap_or(0);
    if version > SETTINGS_VERSION as u64 {
        log::warn!(
            "Réglages écrits par une version plus récente (format {version}) : champs inconnus ignorés"
        );
    }
    // Format 0 → 1 : rien à transformer, seul le numéro de version apparaît.
    obj.insert("version".into(), Value::from(SETTINGS_VERSION));
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (tempfile::TempDir, SettingsStore) {
        let dir = tempfile::tempdir().unwrap();
        let s = SettingsStore::new(dir.path().join("settings.json"));
        (dir, s)
    }

    #[test]
    fn valeurs_par_defaut_du_cahier_des_charges() {
        let s = Settings::default();
        assert!(!s.autostart);
        assert!(!s.follow_dofus);
        assert_eq!(s.dofus_title_pattern, "Dofus");
        assert_eq!(s.hotkeys.toggle_panel.as_deref(), Some("Alt+B"));
        assert_eq!(s.hotkeys.toggle_hide_all.as_deref(), Some("Alt+H"));
        assert_eq!(s.hotkeys.toggle_lock.as_deref(), Some("Alt+L"));
        assert_eq!(s.hotkeys.layouts[3].as_deref(), Some("Alt+4"));
    }

    #[test]
    fn fichier_absent_donne_les_valeurs_par_defaut() {
        let (_d, st) = store();
        assert_eq!(st.load(), Settings::default());
    }

    #[test]
    fn sauvegarde_puis_relecture() {
        let (_d, st) = store();
        let s = Settings {
            language: Language::En,
            favorites: vec!["assistant-forge".into()],
            ..Settings::default()
        };
        st.save(&s).unwrap();
        assert_eq!(st.load(), s);
    }

    #[test]
    fn fichier_corrompu_remplace_par_les_valeurs_par_defaut() {
        let (d, st) = store();
        fs::write(st.path(), b"{\"language\": \"en\", oups").unwrap();
        assert_eq!(st.load(), Settings::default());
        // Le fichier corrompu est gardé de côté, et un fichier sain le remplace.
        let noms: Vec<String> = fs::read_dir(d.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(noms.iter().any(|n| n.starts_with("settings.json.corrupt-")));
        assert!(noms.iter().any(|n| n == "settings.json"));
        let relu: Settings = serde_json::from_slice(&fs::read(st.path()).unwrap()).unwrap();
        assert_eq!(relu, Settings::default());
    }

    #[test]
    fn mauvais_type_traite_comme_corrompu() {
        let (_d, st) = store();
        fs::write(st.path(), b"[1, 2, 3]").unwrap();
        assert_eq!(st.load(), Settings::default());
    }

    #[test]
    fn champs_absents_et_inconnus() {
        let (_d, st) = store();
        fs::write(st.path(), br#"{"language":"en","champFutur":42}"#).unwrap();
        let s = st.load();
        assert_eq!(s.language, Language::En);
        assert_eq!(s.version, SETTINGS_VERSION);
        assert_eq!(s.hotkeys, HotkeyBindings::default());
    }

    #[test]
    fn normalisation() {
        let s = Settings {
            dofus_title_pattern: "   ".into(),
            favorites: vec!["a".into(), "".into(), "a".into(), "b".into()],
            hotkeys: HotkeyBindings { layouts: vec![], ..HotkeyBindings::default() },
            ..Settings::default()
        }
        .normalized();
        assert_eq!(s.dofus_title_pattern, "Dofus");
        assert_eq!(s.favorites, vec!["a", "b"]);
        assert_eq!(s.hotkeys.layouts.len(), LAYOUT_HOTKEY_COUNT);
    }
}
