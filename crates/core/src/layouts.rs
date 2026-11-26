//! Dispositions enregistrées : quelles versions web sont ouvertes, où, à quelle taille,
//! avec quelle opacité, repliées ou non, et où est la barre d'outils.
//!
//! - Fichier : `%APPDATA%\BworkshopLauncher\layouts.json`, JSON versionné, écriture atomique.
//! - Une fenêtre posée sur Dofus (ou collée à ses bords) est aussi enregistrée **par rapport
//!   à Dofus** : rappelée alors que le jeu a bougé, elle retrouve sa place sur le jeu.
//!   Chaque axe s'accroche au bord le plus proche (gauche ou droite, haut ou bas), pour
//!   qu'un widget collé à droite le reste si le jeu change de taille.
//! - Les quatre premières ont un raccourci (Alt+1 à Alt+4 par défaut).

use crate::geometry::{PerMonitor, Rect};
use crate::storage::{self, StorageError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const LAYOUTS_VERSION: u32 = 1;
/// Garde-fous contre un fichier trafiqué.
pub const MAX_LAYOUTS: usize = 20;
pub const MAX_NAME_CHARS: usize = 40;
const MAX_WIDGETS_PER_LAYOUT: usize = 32;

/// Bord auquel un axe est accroché.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Start,
    End,
}

/// Position d'une fenêtre par rapport à Dofus (pixels physiques).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relative {
    pub x_edge: Edge,
    /// Écart entre le bord choisi du jeu et le même bord de la fenêtre.
    pub dx: i32,
    pub y_edge: Edge,
    pub dy: i32,
    pub w: u32,
    pub h: u32,
}

/// Place enregistrée : toujours sur l'écran, et en plus par rapport à Dofus si elle y était.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Place {
    pub screen: PerMonitor,
    pub dofus: Option<Relative>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LayoutWidget {
    pub id: String,
    pub collapsed: bool,
    pub opacity: f64,
    pub place: Place,
}

impl Default for LayoutWidget {
    fn default() -> Self {
        Self { id: String::new(), collapsed: false, opacity: 1.0, place: Place::default() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Layout {
    pub name: String,
    pub widgets: Vec<LayoutWidget>,
    /// Place de la barre d'outils, si elle était ouverte.
    pub toolbar: Option<Place>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LayoutsFile {
    pub version: u32,
    pub layouts: Vec<Layout>,
    /// Dernière disposition rappelée ou enregistrée (rang dans `layouts`).
    pub active: Option<usize>,
}

impl Default for LayoutsFile {
    fn default() -> Self {
        Self { version: LAYOUTS_VERSION, layouts: Vec::new(), active: None }
    }
}

/// Nom propre : sans espaces autour, 40 caractères au plus, jamais vide.
pub fn clean_name(name: &str, fallback: &str) -> String {
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(MAX_NAME_CHARS).collect();
    if name.trim().is_empty() { fallback.to_string() } else { name.trim().to_string() }
}

impl LayoutsFile {
    pub fn normalized(mut self) -> Self {
        self.version = LAYOUTS_VERSION;
        self.layouts.truncate(MAX_LAYOUTS);
        for (i, l) in self.layouts.iter_mut().enumerate() {
            l.name = clean_name(&l.name, &format!("Disposition {}", i + 1));
            l.widgets.truncate(MAX_WIDGETS_PER_LAYOUT);
            l.widgets.retain(|w| !w.id.is_empty());
            for w in &mut l.widgets {
                w.opacity = crate::widgets_store::clamp_opacity(w.opacity);
            }
        }
        if self.active.is_some_and(|a| a >= self.layouts.len()) {
            self.active = None;
        }
        self
    }

    /// Déplace une disposition (l'active suit).
    pub fn move_to(&mut self, from: usize, to: usize) {
        if from >= self.layouts.len() || to >= self.layouts.len() || from == to {
            return;
        }
        let l = self.layouts.remove(from);
        self.layouts.insert(to, l);
        self.active = self.active.map(|a| {
            if a == from {
                to
            } else if from < a && a <= to {
                a - 1
            } else if to <= a && a < from {
                a + 1
            } else {
                a
            }
        });
    }

    pub fn remove(&mut self, index: usize) {
        if index >= self.layouts.len() {
            return;
        }
        self.layouts.remove(index);
        self.active = match self.active {
            Some(a) if a == index => None,
            Some(a) if a > index => Some(a - 1),
            a => a,
        };
    }
}

/// La fenêtre est-elle posée sur Dofus ou collée à ses bords (à `margin` près) ?
pub fn touches(window: Rect, dofus: Rect, margin: i32) -> bool {
    let m = margin.max(0);
    let grown = Rect::new(dofus.x - m, dofus.y - m, dofus.w + 2 * m as u32, dofus.h + 2 * m as u32);
    window.intersection_area(&grown) > 0
}

/// Position par rapport à Dofus, chaque axe accroché au bord le plus proche.
pub fn to_relative(window: Rect, dofus: Rect) -> Relative {
    let axis = |start: i32, size: u32, d_start: i32, d_size: u32| -> (Edge, i32) {
        let center = start as i64 + size as i64 / 2;
        let d_center = d_start as i64 + d_size as i64 / 2;
        if center <= d_center {
            (Edge::Start, start - d_start)
        } else {
            (Edge::End, (d_start + d_size as i32) - (start + size as i32))
        }
    };
    let (x_edge, dx) = axis(window.x, window.w, dofus.x, dofus.w);
    let (y_edge, dy) = axis(window.y, window.h, dofus.y, dofus.h);
    Relative { x_edge, dx, y_edge, dy, w: window.w, h: window.h }
}

/// Rectangle d'une position relative, pour Dofus à `dofus`.
pub fn from_relative(rel: Relative, dofus: Rect) -> Rect {
    let x = match rel.x_edge {
        Edge::Start => dofus.x + rel.dx,
        Edge::End => dofus.x + dofus.w as i32 - rel.dx - rel.w as i32,
    };
    let y = match rel.y_edge {
        Edge::Start => dofus.y + rel.dy,
        Edge::End => dofus.y + dofus.h as i32 - rel.dy - rel.h as i32,
    };
    Rect::new(x, y, rel.w, rel.h)
}

#[derive(Debug, Clone)]
pub struct LayoutsStore {
    path: PathBuf,
}

impl LayoutsStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Ne plante jamais : fichier absent ou illisible → aucune disposition
    /// (un fichier illisible est mis de côté).
    pub fn load(&self) -> LayoutsFile {
        match fs::read(&self.path) {
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    log::warn!("Lecture des dispositions impossible ({e}) : aucune disposition");
                }
                LayoutsFile::default()
            }
            Ok(bytes) => match serde_json::from_slice::<LayoutsFile>(&bytes) {
                Ok(f) => f.normalized(),
                Err(e) => {
                    log::warn!("Dispositions illisibles ({e}) : ancien fichier mis de côté");
                    if let Err(e) = storage::backup_corrupt(&self.path) {
                        log::warn!("Impossible de mettre de côté le fichier corrompu : {e}");
                    }
                    LayoutsFile::default()
                }
            },
        }
    }

    pub fn save(&self, file: &LayoutsFile) -> Result<(), StorageError> {
        storage::write_json_atomic(&self.path, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::MonitorInfo;

    const JEU: Rect = Rect { x: 100, y: 50, w: 1600, h: 900 };

    #[test]
    fn position_relative_a_dofus() {
        // Widget en haut à droite du jeu : accroché à droite et en haut.
        let w = Rect::new(1380, 60, 300, 400);
        let rel = to_relative(w, JEU);
        assert_eq!((rel.x_edge, rel.dx, rel.y_edge, rel.dy), (Edge::End, 20, Edge::Start, 10));
        assert_eq!(from_relative(rel, JEU), w);
        // Le jeu bouge : le widget suit exactement.
        let moved = Rect::new(300, 150, 1600, 900);
        assert_eq!(from_relative(rel, moved), Rect::new(1580, 160, 300, 400));
        // Le jeu s'élargit : le widget reste à 20 px du bord droit.
        let wider = Rect::new(100, 50, 1900, 900);
        assert_eq!(from_relative(rel, wider), Rect::new(1680, 60, 300, 400));
        // Widget en bas à gauche : accroché à gauche et en bas.
        let rel = to_relative(Rect::new(110, 700, 200, 200), JEU);
        assert_eq!((rel.x_edge, rel.y_edge, rel.dy), (Edge::Start, Edge::End, 50));
    }

    #[test]
    fn contact_avec_le_jeu() {
        assert!(touches(Rect::new(500, 300, 100, 100), JEU, 12));
        // Collé dehors, à 10 px du bord droit.
        assert!(touches(Rect::new(1710, 300, 100, 100), JEU, 12));
        assert!(!touches(Rect::new(1750, 300, 100, 100), JEU, 12));
    }

    #[test]
    fn sauvegarde_puis_restauration() {
        let dir = tempfile::tempdir().unwrap();
        let store = LayoutsStore::new(dir.path().join("layouts.json"));
        assert_eq!(store.load(), LayoutsFile::default());

        let ecran = MonitorInfo { key: "A 1920x1080".into(), work: Rect::new(0, 0, 1920, 1040), scale: 1.0, primary: true };
        let rect = Rect::new(1380, 60, 300, 400);
        let mut screen = PerMonitor::default();
        screen.remember(rect, std::slice::from_ref(&ecran));
        let layout = Layout {
            name: "  Donjon  ".into(),
            widgets: vec![LayoutWidget {
                id: "forge".into(),
                collapsed: false,
                opacity: 0.8,
                place: Place { screen, dofus: Some(to_relative(rect, JEU)) },
            }],
            toolbar: None,
        };
        let file = LayoutsFile { version: 1, layouts: vec![layout], active: Some(0) }.normalized();
        store.save(&file).unwrap();
        let back = store.load();
        assert_eq!(back, file);
        assert_eq!(back.layouts[0].name, "Donjon");
        let w = &back.layouts[0].widgets[0];
        assert_eq!(w.place.screen.restore(&[ecran], crate::widgets_store::DEFAULT_WIDGET), Some(rect));
        assert_eq!(from_relative(w.place.dofus.unwrap(), JEU), rect);
    }

    #[test]
    fn fichier_corrompu_et_garde_fous() {
        let dir = tempfile::tempdir().unwrap();
        let store = LayoutsStore::new(dir.path().join("layouts.json"));
        fs::write(dir.path().join("layouts.json"), b"{ oups").unwrap();
        assert_eq!(store.load(), LayoutsFile::default());
        let f = LayoutsFile {
            version: 0,
            layouts: (0..30).map(|_| Layout { name: "".into(), ..Layout::default() }).collect(),
            active: Some(25),
        }
        .normalized();
        assert_eq!(f.layouts.len(), MAX_LAYOUTS);
        assert_eq!(f.layouts[2].name, "Disposition 3");
        assert_eq!(f.active, None);
    }

    #[test]
    fn reordonner_et_supprimer() {
        let named = |n: &str| Layout { name: n.into(), ..Layout::default() };
        let mut f = LayoutsFile { version: 1, layouts: vec![named("a"), named("b"), named("c")], active: Some(2) };
        f.move_to(2, 0);
        assert_eq!(f.layouts.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["c", "a", "b"]);
        assert_eq!(f.active, Some(0));
        f.move_to(1, 2);
        assert_eq!(f.active, Some(0));
        f.remove(0);
        assert_eq!(f.active, None);
        assert_eq!(f.layouts.len(), 2);
    }
}
