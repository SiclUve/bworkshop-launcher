//! État sauvegardé des widgets : ouverts ou non, repliés en pastille, opacité,
//! position par écran (widget et pastille), verrouillage global.
//!
//! Fichier : `%APPDATA%\BworkshopLauncher\widgets.json`, JSON versionné, écriture atomique.

use crate::geometry::{Placement, PerMonitor};
use crate::storage::{self, StorageError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

pub const WIDGETS_VERSION: u32 = 1;
pub const MIN_OPACITY: f64 = 0.3;
pub const MAX_OPACITY: f64 = 1.0;

/// Taille et position par défaut d'un nouveau widget (logique, relative à l'écran principal).
pub const DEFAULT_WIDGET: Placement = Placement { x: 40.0, y: 80.0, w: 380.0, h: 520.0 };
/// Taille minimale d'un widget (logique).
pub const MIN_WIDGET_SIZE: (f64, f64) = (220.0, 140.0);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WidgetState {
    /// Ouvert à la dernière sauvegarde (rouvert au démarrage).
    pub open: bool,
    /// Replié en pastille.
    pub collapsed: bool,
    pub opacity: f64,
    pub window: PerMonitor,
    pub pill: PerMonitor,
}

impl Default for WidgetState {
    fn default() -> Self {
        Self { open: false, collapsed: false, opacity: 1.0, window: PerMonitor::default(), pill: PerMonitor::default() }
    }
}

pub fn clamp_opacity(v: f64) -> f64 {
    if v.is_nan() { MAX_OPACITY } else { v.clamp(MIN_OPACITY, MAX_OPACITY) }
}

/// Ce qu'ouvre un outil (tuile de la barre, raccourci, bouton de sa carte).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OpenMode {
    /// Le logiciel Windows installé.
    App,
    /// La version web compacte, faite pour flotter au-dessus du jeu.
    Overlay,
    /// La page web complète.
    Web,
}

/// Ce qui existe pour un outil.
#[derive(Debug, Clone, Copy, Default)]
pub struct Available {
    pub app: bool,
    pub overlay: bool,
    pub web: bool,
}

impl Available {
    fn has(self, m: OpenMode) -> bool {
        match m {
            OpenMode::App => self.app,
            OpenMode::Overlay => self.overlay,
            OpenMode::Web => self.web,
        }
    }
}

/// Mode retenu : celui choisi s'il existe encore ; sinon le logiciel s'il est installé,
/// puis l'overlay, puis la page web. `None` : rien à ouvrir.
pub fn effective_mode(chosen: Option<OpenMode>, a: Available) -> Option<OpenMode> {
    chosen
        .filter(|m| a.has(*m))
        .or_else(|| [OpenMode::App, OpenMode::Overlay, OpenMode::Web].into_iter().find(|m| a.has(*m)))
}

/// Mode utilisé pour la **fenêtre web** d'un outil : la page complète seulement si elle a
/// été choisie ; sinon l'overlay s'il existe.
pub fn web_mode(chosen: Option<OpenMode>, a: Available) -> Option<OpenMode> {
    match chosen {
        Some(OpenMode::Web) if a.web => Some(OpenMode::Web),
        _ if a.overlay => Some(OpenMode::Overlay),
        _ if a.web => Some(OpenMode::Web),
        _ => None,
    }
}

/// Barre d'outils.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ToolbarState {
    /// Ouverte à la dernière sauvegarde (rouverte au démarrage).
    pub open: bool,
    /// Position par écran (la taille dépend du nombre de tuiles).
    pub position: PerMonitor,
    /// Outils retirés de la barre (toujours installés).
    pub hidden: Vec<String>,
    /// Outils web (non installés) ajoutés à la barre depuis « Mes plugins ».
    pub shown: Vec<String>,
    /// Mode choisi par outil (sinon : `effective_mode`).
    pub modes: BTreeMap<String, OpenMode>,
    /// Ordre choisi en glissant les tuiles (les outils absents de la liste viennent après).
    pub order: Vec<String>,
    /// Tuiles en colonne plutôt qu'en ligne.
    pub vertical: bool,
    /// Taille des tuiles en pixels logiques, choisie en tirant un coin de la barre
    /// (0 : taille par défaut). Toujours lue avec `tile_size()`.
    pub tile: u32,
}

/// Tailles possibles d'une tuile de la barre, en pixels logiques.
pub const TILE_MIN: u32 = 28;
pub const TILE_MAX: u32 = 112;
pub const TILE_DEFAULT: u32 = 44;

/// Taille de tuile ramenée dans les bornes (0 ou absente : taille par défaut).
pub fn clamp_tile(tile: u32) -> u32 {
    if tile == 0 { TILE_DEFAULT } else { tile.clamp(TILE_MIN, TILE_MAX) }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WidgetsFile {
    pub version: u32,
    /// Widgets verrouillés : clic traversant.
    pub locked: bool,
    /// Identifiant de l'outil → état de son widget.
    pub widgets: BTreeMap<String, WidgetState>,
    pub toolbar: ToolbarState,
}

impl Default for WidgetsFile {
    fn default() -> Self {
        Self { version: WIDGETS_VERSION, locked: false, widgets: BTreeMap::new(), toolbar: ToolbarState::default() }
    }
}

impl ToolbarState {
    pub fn tile_size(&self) -> u32 {
        clamp_tile(self.tile)
    }

    /// L'outil a-t-il une tuile ? Installé : oui sauf s'il a été retiré. Web seulement :
    /// s'il a été ajouté.
    pub fn in_bar(&self, id: &str, installed: bool) -> bool {
        !self.hidden.iter().any(|h| h == id) && (installed || self.shown.iter().any(|s| s == id))
    }

    pub fn set_in_bar(&mut self, id: &str, value: bool) {
        self.hidden.retain(|h| h != id);
        self.shown.retain(|s| s != id);
        if value {
            self.shown.push(id.to_string());
        } else {
            self.hidden.push(id.to_string());
        }
    }
}

impl WidgetsFile {
    pub fn normalized(mut self) -> Self {
        self.version = WIDGETS_VERSION;
        for w in self.widgets.values_mut() {
            w.opacity = clamp_opacity(w.opacity);
            if !w.open {
                w.collapsed = false;
            }
        }
        self
    }

    pub fn widget_mut(&mut self, id: &str) -> &mut WidgetState {
        self.widgets.entry(id.to_string()).or_default()
    }
}

#[derive(Debug, Clone)]
pub struct WidgetsStore {
    path: PathBuf,
}

impl WidgetsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Charge l'état. Ne plante jamais : fichier absent ou illisible → état vide.
    pub fn load(&self) -> WidgetsFile {
        match fs::read(&self.path) {
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("Lecture de l'état des widgets impossible ({e}) : état vide");
                }
                WidgetsFile::default()
            }
            Ok(bytes) => match serde_json::from_slice::<WidgetsFile>(&bytes) {
                Ok(f) => f.normalized(),
                Err(e) => {
                    log::warn!("État des widgets illisible ({e}) : ancien fichier mis de côté");
                    if let Err(e) = storage::backup_corrupt(&self.path) {
                        log::warn!("Impossible de mettre de côté le fichier corrompu : {e}");
                    }
                    WidgetsFile::default()
                }
            },
        }
    }

    pub fn save(&self, file: &WidgetsFile) -> Result<(), StorageError> {
        storage::write_json_atomic(&self.path, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{MonitorInfo, Rect};

    #[test]
    fn sauvegarde_puis_restauration() {
        let dir = tempfile::tempdir().unwrap();
        let store = WidgetsStore::new(dir.path().join("widgets.json"));
        let ecrans = [MonitorInfo { key: "A".into(), work: Rect::new(0, 0, 1920, 1040), scale: 1.25, primary: true }];
        let mut f = WidgetsFile { locked: true, ..Default::default() };
        let w = f.widget_mut("assistant-forge");
        w.open = true;
        w.collapsed = true;
        w.opacity = 0.6;
        w.window.remember(Rect::new(100, 200, 500, 625), &ecrans);
        f.toolbar.open = true;
        f.toolbar.position.remember(Rect::new(10, 10, 200, 60), &ecrans);
        store.save(&f).unwrap();
        let relu = store.load();
        assert_eq!(relu, f);
        assert_eq!(
            relu.widgets["assistant-forge"].window.restore(&ecrans, DEFAULT_WIDGET),
            Some(Rect::new(100, 200, 500, 625))
        );
    }

    #[test]
    fn valeurs_corrigees_et_fichier_corrompu() {
        let dir = tempfile::tempdir().unwrap();
        let store = WidgetsStore::new(dir.path().join("widgets.json"));
        fs::write(dir.path().join("widgets.json"), br#"{"widgets":{"a":{"opacity":0.05,"collapsed":true}}}"#).unwrap();
        let f = store.load();
        assert_eq!(f.widgets["a"].opacity, MIN_OPACITY);
        assert!(!f.widgets["a"].collapsed, "un widget fermé n'est pas replié");
        fs::write(dir.path().join("widgets.json"), b"{oups").unwrap();
        assert_eq!(store.load(), WidgetsFile::default());
        assert_eq!(clamp_opacity(3.0), 1.0);
        assert_eq!(clamp_opacity(f64::NAN), 1.0);
    }

    #[test]
    fn modes_d_ouverture() {
        let tout = Available { app: true, overlay: true, web: true };
        let web_seul = Available { app: false, overlay: false, web: true };
        let sans_app = Available { app: false, overlay: true, web: true };
        assert_eq!(effective_mode(None, tout), Some(OpenMode::App));
        assert_eq!(effective_mode(None, sans_app), Some(OpenMode::Overlay));
        assert_eq!(effective_mode(None, web_seul), Some(OpenMode::Web));
        assert_eq!(effective_mode(Some(OpenMode::Web), tout), Some(OpenMode::Web));
        // Choix devenu impossible (logiciel désinstallé) : on retombe sur l'overlay.
        assert_eq!(effective_mode(Some(OpenMode::App), sans_app), Some(OpenMode::Overlay));
        assert_eq!(effective_mode(None, Available::default()), None);
        // Fenêtre web : overlay par défaut, page complète seulement si choisie.
        assert_eq!(web_mode(Some(OpenMode::App), tout), Some(OpenMode::Overlay));
        assert_eq!(web_mode(Some(OpenMode::Web), tout), Some(OpenMode::Web));
        assert_eq!(web_mode(None, web_seul), Some(OpenMode::Web));
    }

    #[test]
    fn tuiles_de_la_barre() {
        let mut t = ToolbarState::default();
        assert!(t.in_bar("forge", true));
        assert!(!t.in_bar("bworklingo", false));
        t.set_in_bar("bworklingo", true);
        assert!(t.in_bar("bworklingo", false));
        t.set_in_bar("forge", false);
        assert!(!t.in_bar("forge", true));
        t.set_in_bar("forge", true);
        assert!(t.in_bar("forge", true));
        assert_eq!(t.hidden, Vec::<String>::new());
    }

    #[test]
    fn taille_des_tuiles() {
        // Fichier d'une version précédente, sans taille : taille par défaut.
        let t: ToolbarState = serde_json::from_str(r#"{"open":true}"#).unwrap();
        assert_eq!(t.tile_size(), TILE_DEFAULT);
        assert_eq!(clamp_tile(5), TILE_MIN);
        assert_eq!(clamp_tile(500), TILE_MAX);
        assert_eq!(clamp_tile(60), 60);
    }
}
