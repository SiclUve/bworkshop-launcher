//! Barre d'outils : une tuile par logiciel installé, façon barre de raccourcis.
//!
//! - Page locale `toolbar.html`, sans page distante.
//! - Toujours au premier plan, jamais activée par le launcher (même règle que les widgets).
//! - Sa taille suit le nombre de tuiles et leur taille, choisie en tirant un coin ;
//!   sa position est enregistrée par écran.
//! - Jamais rendue traversante par le verrouillage : elle doit rester cliquable en jeu.

use super::{monitors, native, Widgets, BACKGROUND};
use bworkshop_core::geometry::{self, Placement, Rect};
use bworkshop_core::widgets_store::{clamp_tile, OpenMode};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};
use windows::Win32::Foundation::HWND;

pub const LABEL: &str = "toolbar";
/// Clé de la barre dans la liste des éléments déplacés (« : » est interdit dans un identifiant d'outil).
pub(super) const MOVED_KEY: &str = ":toolbar";

/// Dimensions en pixels logiques (doivent correspondre à `Toolbar.svelte`).
/// La taille des tuiles, elle, se choisit en tirant un coin de la barre.
const GAP: f64 = 4.0;
const PAD: f64 = 6.0;
const HANDLE: f64 = 14.0;
const MAX_COLUMNS: usize = 6;
/// Bande de message sous les tuiles (deux lignes), et largeur minimale pour la lire.
const MESSAGE_HEIGHT: f64 = 40.0;
const MESSAGE_MIN_WIDTH: f64 = 240.0;

/// Coin tiré pour redimensionner : le coin opposé reste en place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Taille de la barre pour `count` tuiles (la tuile « + » comprise) de `tile` pixels,
/// avec ou sans message, en ligne (6 colonnes au plus) ou en colonne (6 lignes au plus).
pub fn size_for(count: usize, message: bool, vertical: bool, tile: u32) -> (f64, f64) {
    let tile = tile as f64;
    let n = count.max(1);
    let (cols, rows) = if vertical {
        let rows = n.min(MAX_COLUMNS);
        (n.div_ceil(rows), rows)
    } else {
        let cols = n.min(MAX_COLUMNS);
        (cols, n.div_ceil(cols))
    };
    let span = |k: usize| k as f64 * tile + (k.saturating_sub(1)) as f64 * GAP;
    let (w, h) = (2.0 * PAD + span(cols), HANDLE + 2.0 * PAD + span(rows));
    if message {
        (w.max(MESSAGE_MIN_WIDTH), h + MESSAGE_HEIGHT)
    } else {
        (w, h)
    }
}

const DEFAULT_POSITION: Placement = Placement { x: 40.0, y: 40.0, w: 0.0, h: 0.0 };

impl Widgets {
    fn toolbar_hwnd(&self) -> Option<HWND> {
        self.toolbar_hwnd.lock().ok().and_then(|h| *h).map(|h| HWND(h as _))
    }

    pub fn toolbar_open(&self) -> bool {
        self.toolbar_hwnd().is_some()
    }

    /// Ouvre la barre (ou la réaffiche). À appeler hors du fil principal.
    pub fn open_toolbar(&self, app: &AppHandle) -> Result<(), String> {
        if let Some(hwnd) = self.toolbar_hwnd() {
            if !self.hidden_all.load(Ordering::SeqCst) {
                native::show_no_activate(hwnd);
            }
            return Ok(());
        }
        let Some(_opening) = self.begin_opening(LABEL) else { return Ok(()) };
        let monitors = monitors(app);
        let (position, tile) = {
            let f = self.file.lock().map_err(|_| "État des widgets indisponible.")?;
            (f.toolbar.position.clone(), f.toolbar.tile_size())
        };
        let (w, h) = size_for(1, false, false, tile);
        let default = Placement { w, h, ..DEFAULT_POSITION };
        let start = position.restore(&monitors, default).unwrap_or(Rect::new(40, 40, w as u32, h as u32));

        let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("toolbar.html".into()))
            .additional_browser_args(crate::state::BROWSER_ARGS)
            .title("Barre d'outils · Bworkshop")
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .focused(false)
            .resizable(false)
            .background_color(BACKGROUND)
            .build()
            .map_err(|e| format!("Barre d'outils non créée : {e}"))?;
        let hwnd = match window.hwnd() {
            Ok(h) => h,
            Err(e) => {
                let _ = window.destroy();
                return Err(format!("Barre d'outils introuvable : {e}"));
            }
        };
        let raw = hwnd.0 as isize;
        if let Ok(mut h) = self.toolbar_hwnd.lock() {
            *h = Some(raw);
        }
        if let Ok(mut f) = self.file.lock() {
            f.toolbar.open = true;
        }
        let hidden = self.hidden_all.load(Ordering::SeqCst);
        let app2 = app.clone();
        app.run_on_main_thread(move || {
            let hwnd = HWND(raw as _);
            native::set_rect(hwnd, start);
            native::install_subclass(hwnd);
            native::apply_style(hwnd, 1.0, false);
            if !hidden {
                native::show_no_activate(hwnd);
            }
            app2.state::<Widgets>().emit_updated(&app2);
        })
        .map_err(|e| e.to_string())?;
        log::info!("Barre d'outils ouverte");
        self.touch();
        Ok(())
    }

    pub fn close_toolbar(&self, app: &AppHandle) {
        self.mark_moved(MOVED_KEY);
        self.flush(app);
        if let Ok(mut h) = self.toolbar_hwnd.lock()
            && let Some(raw) = h.take() {
                native::forget_size_rules(HWND(raw as _));
            }
        if let Ok(mut f) = self.file.lock() {
            f.toolbar.open = false;
        }
        if let Some(w) = app.get_webview_window(LABEL) {
            let _ = w.destroy();
        }
        log::info!("Barre d'outils fermée");
        self.touch();
        self.emit_updated(app);
    }

    /// Ajuste la taille au nombre de tuiles, en gardant le coin haut-gauche (et la barre dans l'écran).
    pub fn fit_toolbar(&self, app: &AppHandle, count: usize, message: bool) {
        self.fit_toolbar_from(app, count, message, Corner::BottomRight);
    }

    /// Ajuste la taille ; le coin opposé à `dragged` reste en place.
    fn fit_toolbar_from(&self, app: &AppHandle, count: usize, message: bool, dragged: Corner) {
        let Some(hwnd) = self.toolbar_hwnd() else { return };
        let Some(current) = native::window_rect(hwnd) else { return };
        let monitors = monitors(app);
        let Some(m) = geometry::monitor_for(current, &monitors) else { return };
        let (vertical, tile) = self.file.lock().map(|f| (f.toolbar.vertical, f.toolbar.tile_size())).unwrap_or((false, 44));
        let (w, h) = size_for(count, message, vertical, tile);
        let (w, h) = ((w * m.scale).round() as u32, (h * m.scale).round() as u32);
        let x = match dragged {
            Corner::TopLeft | Corner::BottomLeft => current.x + current.w as i32 - w as i32,
            _ => current.x,
        };
        let y = match dragged {
            Corner::TopLeft | Corner::TopRight => current.y + current.h as i32 - h as i32,
            _ => current.y,
        };
        let target = geometry::clamp(Rect::new(x, y, w, h), m);
        if target != current {
            native::set_rect(hwnd, target);
        }
    }

    pub(super) fn toolbar_rect(&self) -> Option<Rect> {
        self.toolbar_hwnd().and_then(native::window_rect)
    }

    pub(super) fn toolbar_hwnd_for_all(&self) -> Option<HWND> {
        self.toolbar_hwnd()
    }

    /// Raccourci (Alt+B) : ouvre la barre si elle est fermée, sinon l'affiche ou la cache,
    /// sans jamais prendre le focus au jeu. Afficher / cacher est instantané (fenêtre gardée).
    pub fn toggle_toolbar(app: &AppHandle) {
        let Some(hwnd) = app.state::<Widgets>().toolbar_hwnd() else {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = app.state::<Widgets>().open_toolbar(&app) {
                    log::warn!("Barre d'outils non ouverte : {e}");
                }
            });
            return;
        };
        let raw = hwnd.0 as isize;
        let _ = app.run_on_main_thread(move || {
            let hwnd = HWND(raw as _);
            if native::is_visible(hwnd) {
                native::hide(hwnd);
            } else {
                native::show_no_activate(hwnd);
            }
        });
    }

    /// Rouvre la barre si elle était ouverte à la dernière fermeture du launcher.
    pub fn restore_toolbar(app: &AppHandle) {
        let was_open = app.state::<Widgets>().file.lock().map(|f| f.toolbar.open).unwrap_or(false);
        if !was_open {
            return;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = app.state::<Widgets>().open_toolbar(&app) {
                log::warn!("Barre d'outils non rouverte : {e}");
            }
        });
    }
}

/// Réglages de la barre : tuiles retirées ou ajoutées, mode d'ouverture, ordre, orientation.
#[derive(Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolbarPrefs {
    pub hidden: Vec<String>,
    pub shown: Vec<String>,
    pub modes: std::collections::BTreeMap<String, OpenMode>,
    pub order: Vec<String>,
    pub vertical: bool,
    /// Taille des tuiles, en pixels logiques.
    pub tile: u32,
}

/// Événement envoyé à la barre quand ses réglages changent.
pub const EVENT_PREFS: &str = "toolbar-prefs";

impl Widgets {
    pub fn toolbar_prefs(&self) -> ToolbarPrefs {
        self.file
            .lock()
            .map(|f| ToolbarPrefs {
                hidden: f.toolbar.hidden.clone(),
                shown: f.toolbar.shown.clone(),
                modes: f.toolbar.modes.clone(),
                order: f.toolbar.order.clone(),
                vertical: f.toolbar.vertical,
                tile: f.toolbar.tile_size(),
            })
            .unwrap_or_default()
    }

    fn update_prefs(&self, app: &AppHandle, change: impl FnOnce(&mut bworkshop_core::widgets_store::ToolbarState)) {
        if let Ok(mut f) = self.file.lock() {
            change(&mut f.toolbar);
        }
        self.touch();
        use tauri::Emitter;
        // La fenêtre principale (onglet « Mes plugins ») suit aussi ces réglages.
        let prefs = self.toolbar_prefs();
        let _ = app.emit_to(LABEL, EVENT_PREFS, prefs.clone());
        let _ = app.emit_to(crate::state::MAIN_LABEL, EVENT_PREFS, prefs);
    }

    /// Mode choisi pour un outil (`None` : choix automatique).
    pub fn chosen_mode(&self, id: &str) -> Option<OpenMode> {
        self.file.lock().ok().and_then(|f| f.toolbar.modes.get(id).copied())
    }

    /// Met un outil dans la barre, ou l'en retire (installé ou web).
    pub fn set_tool_in_bar(&self, app: &AppHandle, id: &str, value: bool) {
        self.update_prefs(app, |t| t.set_in_bar(id, value));
    }

    /// Choisit comment ouvrir un outil. Si sa version web est ouverte, elle est rouverte
    /// avec la bonne adresse (overlay ou page complète).
    pub fn set_tool_mode(&self, app: &AppHandle, id: &str, mode: Option<OpenMode>) {
        let before = self.chosen_mode(id);
        self.update_prefs(app, |t| match mode {
            Some(m) => {
                t.modes.insert(id.to_string(), m);
            }
            None => {
                t.modes.remove(id);
            }
        });
        if before != mode && self.view().open.iter().any(|x| x == id) {
            self.close(app, id);
            if mode != Some(OpenMode::App)
                && let Err(e) = self.open(app, id) {
                    log::info!("Version web de {id} non rouverte : {e}");
                }
        }
    }

    /// Nouvel ordre des tuiles (identifiants d'outils, 200 au plus).
    pub fn set_toolbar_order(&self, app: &AppHandle, order: Vec<String>) {
        let order: Vec<String> = order.into_iter().take(200).collect();
        self.update_prefs(app, |t| t.order = order);
    }

    pub fn set_toolbar_vertical(&self, app: &AppHandle, vertical: bool) {
        self.update_prefs(app, |t| t.vertical = vertical);
    }

    /// Nouvelle taille des tuiles, en tirant le coin `dragged` : la fenêtre suit aussitôt.
    /// `done` : fin du glisser, la taille est enregistrée et annoncée.
    pub fn resize_toolbar(&self, app: &AppHandle, tile: u32, count: usize, message: bool, dragged: Corner, done: bool) {
        let tile = clamp_tile(tile);
        if done {
            self.update_prefs(app, |t| t.tile = tile);
        } else { if let Ok(mut f) = self.file.lock() {
            f.toolbar.tile = tile;
        }}
        self.fit_toolbar_from(app, count, message, dragged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taille_selon_le_nombre_de_tuiles() {
        // Une tuile (« + » seule) : une colonne, une ligne.
        assert_eq!(size_for(0, false, false, 44), (56.0, 70.0));
        assert_eq!(size_for(1, false, false, 44), (56.0, 70.0));
        // Six tuiles : une ligne pleine.
        assert_eq!(size_for(6, false, false, 44), (12.0 + 6.0 * 44.0 + 5.0 * 4.0, 70.0));
        // Sept tuiles : deux lignes, six colonnes.
        assert_eq!(size_for(7, false, false, 44), (12.0 + 6.0 * 44.0 + 5.0 * 4.0, 14.0 + 12.0 + 2.0 * 44.0 + 4.0));
        // Avec message : bande de 40 px, et 240 px de large au moins.
        assert_eq!(size_for(2, true, false, 44), (240.0, 70.0 + 40.0));
        // En colonne : sept tuiles = six lignes, deux colonnes.
        assert_eq!(size_for(7, false, true, 44), (12.0 + 2.0 * 44.0 + 4.0, 14.0 + 12.0 + 6.0 * 44.0 + 5.0 * 4.0));
        // Tuiles agrandies en tirant un coin.
        assert_eq!(size_for(2, false, false, 80), (12.0 + 2.0 * 80.0 + 4.0, 14.0 + 12.0 + 80.0));
    }
}
