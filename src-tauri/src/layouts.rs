//! Dispositions : enregistrer, renommer, réordonner, supprimer, rappeler.
//! Le fichier et les calculs sont dans `bworkshop_core::layouts`.

use crate::state::AppState;
use crate::widgets::Widgets;
use bworkshop_core::layouts::{clean_name, LayoutsFile, LayoutsStore, MAX_LAYOUTS};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// Événement envoyé à la fenêtre principale et à la barre quand la liste change.
pub const EVENT_UPDATED: &str = "layouts-updated";
/// Demande d'ouvrir la fenêtre « Dispositions » (depuis le menu de la barre).
pub const EVENT_OPEN: &str = "open-layouts";
/// La fenêtre principale peut être en cours de création : la demande attend qu'elle la lise.
static OPEN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Affiche le launcher sur la fenêtre « Dispositions ».
pub fn request_open(app: &AppHandle) {
    OPEN_REQUESTED.store(true, Ordering::SeqCst);
    crate::state::show_main(app);
    let _ = app.emit_to(crate::state::MAIN_LABEL, EVENT_OPEN, ());
}

pub struct Layouts {
    store: LayoutsStore,
    file: Mutex<LayoutsFile>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutInfo {
    pub name: String,
    /// Nombre de versions web.
    pub widgets: usize,
    /// Leurs identifiants d'outils (dans l'ordre enregistré).
    pub tools: Vec<String>,
    pub toolbar: bool,
    /// Au moins une fenêtre placée par rapport à Dofus (elle suivra le jeu au rappel).
    pub follows_dofus: bool,
}

/// Élément « barre d'outils » pour `layout_remove_item`.
pub const TOOLBAR_ITEM: &str = ":toolbar";

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LayoutsView {
    pub layouts: Vec<LayoutInfo>,
    pub active: Option<usize>,
}

impl Layouts {
    pub fn new(store: LayoutsStore) -> Self {
        let file = store.load();
        Self { store, file: Mutex::new(file) }
    }

    pub fn view(&self) -> LayoutsView {
        let Ok(f) = self.file.lock() else { return LayoutsView::default() };
        LayoutsView {
            layouts: f
                .layouts
                .iter()
                .map(|l| LayoutInfo {
                    name: l.name.clone(),
                    widgets: l.widgets.len(),
                    tools: l.widgets.iter().map(|w| w.id.clone()).collect(),
                    toolbar: l.toolbar.is_some(),
                    follows_dofus: l.widgets.iter().any(|w| w.place.dofus.is_some())
                        || l.toolbar.as_ref().is_some_and(|t| t.dofus.is_some()),
                })
                .collect(),
            active: f.active,
        }
    }

    pub fn names(&self) -> Vec<String> {
        self.file.lock().map(|f| f.layouts.iter().map(|l| l.name.clone()).collect()).unwrap_or_default()
    }

    /// Modifie, enregistre, prévient les fenêtres et le menu de notification.
    fn change(&self, app: &AppHandle, f: impl FnOnce(&mut LayoutsFile) -> Result<(), String>) -> Result<LayoutsView, String> {
        {
            let mut file = self.file.lock().map_err(|_| "Dispositions indisponibles.")?;
            f(&mut file)?;
            let normalized = std::mem::take(&mut *file).normalized();
            *file = normalized;
            self.store.save(&file).map_err(|e| format!("Dispositions non enregistrées : {e}"))?;
        }
        let view = self.view();
        let _ = app.emit_to(crate::state::MAIN_LABEL, EVENT_UPDATED, view.clone());
        crate::tray::refresh(app);
        Ok(view)
    }
}

/// Fenêtre de Dofus connue (seulement avec l'option « Suivre Dofus »).
fn dofus_rect() -> Option<bworkshop_core::geometry::Rect> {
    crate::dofus::current_rect()
}

fn default_name(app: &AppHandle, n: usize) -> String {
    match app.state::<AppState>().settings().language {
        bworkshop_core::settings::Language::Fr => format!("Disposition {n}"),
        bworkshop_core::settings::Language::En => format!("Layout {n}"),
    }
}

/// Enregistre l'état actuel dans une nouvelle disposition.
pub fn save_new(app: &AppHandle, name: &str) -> Result<LayoutsView, String> {
    let count = app.state::<Layouts>().view().layouts.len();
    if count >= MAX_LAYOUTS {
        return Err(format!("{MAX_LAYOUTS} dispositions au plus : supprime-en une d'abord."));
    }
    let name = clean_name(name, &default_name(app, count + 1));
    let layout = app.state::<Widgets>().capture_layout(app, name, dofus_rect());
    app.state::<Layouts>().change(app, |f| {
        f.layouts.push(layout);
        f.active = Some(f.layouts.len() - 1);
        Ok(())
    })
}

/// Remplace une disposition par l'état actuel (en gardant son nom).
pub fn overwrite(app: &AppHandle, index: usize) -> Result<LayoutsView, String> {
    let name = app.state::<Layouts>().names().get(index).cloned().ok_or("Disposition introuvable.")?;
    let layout = app.state::<Widgets>().capture_layout(app, name, dofus_rect());
    app.state::<Layouts>().change(app, |f| {
        let slot = f.layouts.get_mut(index).ok_or("Disposition introuvable.")?;
        *slot = layout;
        f.active = Some(index);
        Ok(())
    })
}

pub fn rename(app: &AppHandle, index: usize, name: &str) -> Result<LayoutsView, String> {
    let fallback = default_name(app, index + 1);
    app.state::<Layouts>().change(app, |f| {
        let l = f.layouts.get_mut(index).ok_or("Disposition introuvable.")?;
        l.name = clean_name(name, &fallback);
        Ok(())
    })
}

pub fn move_to(app: &AppHandle, from: usize, to: usize) -> Result<LayoutsView, String> {
    app.state::<Layouts>().change(app, |f| {
        f.move_to(from, to);
        Ok(())
    })
}

pub fn delete(app: &AppHandle, index: usize) -> Result<LayoutsView, String> {
    app.state::<Layouts>().change(app, |f| {
        f.remove(index);
        Ok(())
    })
}

/// Retire un outil (ou la barre, `TOOLBAR_ITEM`) d'une disposition, sans toucher aux autres.
pub fn remove_item(app: &AppHandle, index: usize, item: &str) -> Result<LayoutsView, String> {
    app.state::<Layouts>().change(app, |f| {
        let l = f.layouts.get_mut(index).ok_or("Disposition introuvable.")?;
        if item == TOOLBAR_ITEM {
            l.toolbar = None;
        } else {
            l.widgets.retain(|w| w.id != item);
        }
        Ok(())
    })
}

/// Rappelle une disposition (sur un autre fil : ouvrir une version web prend un moment).
pub fn apply(app: &AppHandle, index: usize) -> Result<(), String> {
    let layout = {
        let layouts = app.state::<Layouts>();
        let f = layouts.file.lock().map_err(|_| "Dispositions indisponibles.")?;
        f.layouts.get(index).cloned().ok_or("Disposition introuvable.")?
    };
    let app2 = app.clone();
    std::thread::Builder::new()
        .name("disposition".into())
        .spawn(move || {
            app2.state::<Widgets>().apply_layout(&app2, &layout, dofus_rect());
            let _ = app2.state::<Layouts>().change(&app2, |f| {
                f.active = Some(index);
                Ok(())
            });
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Au démarrage, avec l'option « Rouvrir la dernière disposition ».
pub fn restore_at_startup(app: &AppHandle) {
    if !app.state::<AppState>().settings().reopen_last_layout {
        return;
    }
    let active = app.state::<Layouts>().view().active;
    if let Some(i) = active
        && let Err(e) = apply(app, i) {
            log::warn!("Dernière disposition non rouverte : {e}");
        }
}

// ---------- Commandes ----------

#[tauri::command]
pub fn get_layouts(app: AppHandle) -> LayoutsView {
    app.state::<Layouts>().view()
}

/// Vrai une seule fois après une demande d'ouverture de la fenêtre « Dispositions ».
#[tauri::command]
pub fn take_layouts_request() -> bool {
    OPEN_REQUESTED.swap(false, Ordering::SeqCst)
}

#[tauri::command]
pub fn layout_save(app: AppHandle, name: String) -> Result<LayoutsView, String> {
    save_new(&app, &name)
}

#[tauri::command]
pub fn layout_overwrite(app: AppHandle, index: usize) -> Result<LayoutsView, String> {
    overwrite(&app, index)
}

#[tauri::command]
pub fn layout_rename(app: AppHandle, index: usize, name: String) -> Result<LayoutsView, String> {
    rename(&app, index, &name)
}

#[tauri::command]
pub fn layout_move(app: AppHandle, from: usize, to: usize) -> Result<LayoutsView, String> {
    move_to(&app, from, to)
}

#[tauri::command]
pub fn layout_delete(app: AppHandle, index: usize) -> Result<LayoutsView, String> {
    delete(&app, index)
}

#[tauri::command]
pub fn layout_apply(app: AppHandle, index: usize) -> Result<(), String> {
    apply(&app, index)
}

#[tauri::command]
pub fn layout_remove_item(app: AppHandle, index: usize, item: String) -> Result<LayoutsView, String> {
    remove_item(&app, index, &item)
}
