//! Widgets : versions web des outils dans de petites fenêtres au-dessus du jeu.
//!
//! Une fenêtre de widget contient deux vues :
//! - `wbar-<id>` : la barre du launcher (page locale, seules les commandes `widget_*`) ;
//! - `wweb-<id>` : la page de l'outil (distante, **aucune** capacité : ni IPC, ni
//!   fichiers, ni shell). Navigation limitée au domaine de l'outil, pas de fenêtre
//!   surgissante, pas de téléchargement, autorisations (micro, caméra, position) refusées.
//!
//! La fenêtre n'est jamais activée par le launcher (voir `native`).

pub mod commands;
pub(crate) mod native;
mod follow;
mod recall;
pub mod toolbar;
pub mod toolbar_menu;

use crate::state::{self, AppState};
use bworkshop_core::geometry::{self, MonitorInfo, Rect};
use bworkshop_core::security::domains;
use bworkshop_core::settings::Language;
use bworkshop_core::widgets_store::{self, web_mode, Available, OpenMode, WidgetsFile, WidgetsStore, DEFAULT_WIDGET, MIN_WIDGET_SIZE};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::webview::{DownloadEvent, NewWindowResponse, WebviewBuilder};
use tauri::window::{Color, WindowBuilder};
use tauri::{AppHandle, Emitter, EventTarget, Manager, PhysicalPosition, PhysicalSize, Url, WebviewUrl, Window, WindowEvent};
use tauri_plugin_opener::OpenerExt;

/// Hauteur de la barre, en pixels logiques (24 : cibles cliquables d'au moins 24 px).
const BAR_HEIGHT: f64 = 24.0;
/// Marge autour des vues : c'est là que la souris attrape le bord pour redimensionner.
const INSET: f64 = 4.0;
/// Taille de la pastille (fenêtre repliée), en pixels logiques.
const PILL_SIZE: (f64, f64) = (190.0, BAR_HEIGHT + 2.0 * INSET);
/// Hauteur du menu de réglages qui s'ouvre sous la barre.
const SETTINGS_HEIGHT: f64 = 104.0;
const BACKGROUND: Color = Color(0x19, 0x1d, 0x17, 0xff);
/// Délai sans mouvement avant d'enregistrer les positions.
const SAVE_DELAY: Duration = Duration::from_millis(400);
/// Une page ne peut pas ouvrir plus d'un lien externe par intervalle.
const EXTERNAL_OPEN_INTERVAL: Duration = Duration::from_secs(2);

const WINDOW_PREFIX: &str = "widget-";
const BAR_PREFIX: &str = "wbar-";
const WEB_PREFIX: &str = "wweb-";

/// Vers la fenêtre principale : liste des widgets ouverts, verrouillage, masquage.
pub const EVENT_UPDATED: &str = "widgets-updated";
/// Vers une barre de widget : son état.
pub const EVENT_BAR: &str = "widget-bar-state";

fn window_label(id: &str) -> String {
    format!("{WINDOW_PREFIX}{id}")
}
fn bar_label(id: &str) -> String {
    format!("{BAR_PREFIX}{id}")
}
fn web_label(id: &str) -> String {
    format!("{WEB_PREFIX}{id}")
}

pub fn id_of_window(label: &str) -> Option<&str> {
    label.strip_prefix(WINDOW_PREFIX)
}

pub fn id_of_bar(label: &str) -> Option<&str> {
    label.strip_prefix(BAR_PREFIX)
}

/// Les identifiants du catalogue deviennent des étiquettes de fenêtre : on les restreint.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetsView {
    pub open: Vec<String>,
    pub locked: bool,
    pub hidden_all: bool,
    pub toolbar_open: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BarState {
    pub id: String,
    pub name: String,
    pub opacity: f64,
    pub collapsed: bool,
    pub locked: bool,
    pub settings_open: bool,
    pub language: Language,
}

/// Widget ouvert.
struct Live {
    name: String,
    hwnd: isize,
    settings_open: bool,
}

pub struct Widgets {
    store: WidgetsStore,
    file: Mutex<WidgetsFile>,
    live: Mutex<BTreeMap<String, Live>>,
    hidden_all: AtomicBool,
    /// Widgets déplacés ou redimensionnés depuis le dernier enregistrement.
    moved: Mutex<BTreeSet<String>>,
    saver: Mutex<Option<mpsc::Sender<()>>>,
    /// Fenêtre de la barre d'outils, si elle est ouverte.
    toolbar_hwnd: Mutex<Option<isize>>,
    /// Fenêtres cachées parce que Dofus n'est plus au premier plan (option « Suivre Dofus »).
    away: Mutex<Vec<isize>>,
    /// Fenêtres en cours de création : un second appel simultané ne crée pas de doublon.
    opening: Mutex<BTreeSet<String>>,
    /// Tenu pendant l'écriture du fichier (voir `flush`).
    saving: Mutex<()>,
}

/// Réservation d'une fenêtre en cours de création, rendue à la fin (succès ou erreur).
pub(crate) struct Opening<'a> {
    set: &'a Mutex<BTreeSet<String>>,
    label: String,
}

impl Drop for Opening<'_> {
    fn drop(&mut self) {
        if let Ok(mut s) = self.set.lock() {
            s.remove(&self.label);
        }
    }
}

impl Widgets {
    pub fn new(path: PathBuf) -> Self {
        let store = WidgetsStore::new(path);
        let file = store.load();
        Self {
            store,
            file: Mutex::new(file),
            live: Mutex::new(BTreeMap::new()),
            hidden_all: AtomicBool::new(false),
            moved: Mutex::new(BTreeSet::new()),
            saver: Mutex::new(None),
            toolbar_hwnd: Mutex::new(None),
            away: Mutex::new(Vec::new()),
            opening: Mutex::new(BTreeSet::new()),
            saving: Mutex::new(()),
        }
    }

    /// Réserve la création de la fenêtre `label` ; `None` si elle est déjà en cours.
    pub(crate) fn begin_opening(&self, label: &str) -> Option<Opening<'_>> {
        let mut s = self.opening.lock().ok()?;
        s.insert(label.to_string()).then(|| Opening { set: &self.opening, label: label.to_string() })
    }

    // ---------- Lecture ----------

    pub fn view(&self) -> WidgetsView {
        WidgetsView {
            open: self.live.lock().map(|l| l.keys().cloned().collect()).unwrap_or_default(),
            locked: self.file.lock().map(|f| f.locked).unwrap_or(false),
            hidden_all: self.hidden_all.load(Ordering::SeqCst),
            toolbar_open: self.toolbar_open(),
        }
    }

    fn hwnd_of(&self, id: &str) -> Option<windows::Win32::Foundation::HWND> {
        let live = self.live.lock().ok()?;
        live.get(id).map(|l| windows::Win32::Foundation::HWND(l.hwnd as _))
    }

    fn live_hwnds(&self) -> Vec<(String, windows::Win32::Foundation::HWND)> {
        self.live
            .lock()
            .map(|l| l.iter().map(|(id, w)| (id.clone(), windows::Win32::Foundation::HWND(w.hwnd as _))).collect())
            .unwrap_or_default()
    }

    pub fn bar_state(&self, app: &AppHandle, id: &str) -> Option<BarState> {
        let (name, settings_open) = {
            let live = self.live.lock().ok()?;
            let l = live.get(id)?;
            (l.name.clone(), l.settings_open)
        };
        let (opacity, collapsed, locked) = {
            let f = self.file.lock().ok()?;
            let st = f.widgets.get(id).cloned().unwrap_or_default();
            (st.opacity, st.collapsed, f.locked)
        };
        let language = app.state::<AppState>().settings().language;
        Some(BarState { id: id.to_string(), name, opacity, collapsed, locked, settings_open, language })
    }

    fn emit_bar(&self, app: &AppHandle, id: &str) {
        if let Some(s) = self.bar_state(app, id) {
            let _ = app.emit_to(EventTarget::webview(bar_label(id)), EVENT_BAR, s);
        }
    }

    pub fn emit_all_bars(&self, app: &AppHandle) {
        for (id, _) in self.live_hwnds() {
            self.emit_bar(app, &id);
        }
    }

    fn emit_updated(&self, app: &AppHandle) {
        let view = self.view();
        let _ = app.emit_to(state::MAIN_LABEL, EVENT_UPDATED, view.clone());
        // La barre d'outils allume les tuiles des versions web ouvertes.
        let _ = app.emit_to(toolbar::LABEL, EVENT_UPDATED, view);
    }

    // ---------- Enregistrement ----------

    /// Lance le fil d'enregistrement différé (positions, opacité…).
    pub fn start(app: &AppHandle) {
        let (tx, rx) = mpsc::channel::<()>();
        if let Ok(mut s) = app.state::<Widgets>().saver.lock() {
            *s = Some(tx);
        }
        let app = app.clone();
        let spawned = std::thread::Builder::new().name("widgets-saver".into()).spawn(move || {
            while rx.recv().is_ok() {
                // On attend que ça ne bouge plus.
                while rx.recv_timeout(SAVE_DELAY).is_ok() {}
                app.state::<Widgets>().flush(&app);
            }
        });
        if let Err(e) = spawned {
            log::error!("Enregistrement des widgets indisponible : {e}");
        }
    }

    fn touch(&self) {
        if let Ok(s) = self.saver.lock()
            && let Some(tx) = s.as_ref() {
                let _ = tx.send(());
            }
    }

    fn mark_moved(&self, id: &str) {
        if let Ok(mut m) = self.moved.lock() {
            m.insert(id.to_string());
        }
        self.touch();
    }

    /// Mémorise les positions des widgets déplacés et écrit le fichier.
    pub fn flush(&self, app: &AppHandle) {
        let moved: Vec<String> = self.moved.lock().map(|mut m| std::mem::take(&mut *m).into_iter().collect()).unwrap_or_default();
        if !moved.is_empty() {
            // Aucun verrou tenu pendant les appels à Tauri.
            let monitors = monitors(app);
            let toolbar_rect = if moved.iter().any(|k| k == toolbar::MOVED_KEY) { self.toolbar_rect() } else { None };
            let rects: Vec<(String, Rect)> = moved
                .iter()
                .filter(|k| k.as_str() != toolbar::MOVED_KEY)
                .filter_map(|id| self.hwnd_of(id).and_then(native::window_rect).map(|r| (id.clone(), r)))
                .collect();
            if let Ok(mut f) = self.file.lock() {
                if let Some(r) = toolbar_rect {
                    f.toolbar.position.remember(r, &monitors);
                }
                for (id, r) in rects {
                    let st = f.widget_mut(&id);
                    if st.collapsed {
                        st.pill.remember(r, &monitors);
                    } else {
                        st.window.remember(r, &monitors);
                    }
                }
            }
        }
        // Copie et écriture sous un même verrou : deux enregistrements simultanés écrivent
        // dans l'ordre de leurs copies (la plus récente en dernier), jamais l'inverse.
        let Ok(_saving) = self.saving.lock() else { return };
        let snapshot = match self.file.lock() {
            Ok(f) => f.clone(),
            Err(_) => return,
        };
        if let Err(e) = self.store.save(&snapshot) {
            log::warn!("État des widgets non enregistré : {e}");
        }
    }

    // ---------- Ouverture et fermeture ----------

    /// Ouvre le widget d'un outil (ou le réaffiche s'il est déjà ouvert).
    /// À appeler hors du fil principal : la création d'une vue WebView2 l'attend.
    pub fn open(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        if !valid_id(id) {
            return Err("Identifiant d'outil invalide.".into());
        }
        if self.hidden_all.load(Ordering::SeqCst) {
            self.set_hidden_all(app, false);
        }
        if let Some(hwnd) = self.hwnd_of(id) {
            native::show_no_activate(hwnd);
            return Ok(());
        }
        // Déjà en cours de création (double clic, rappel de disposition…) : rien de plus.
        let Some(_opening) = self.begin_opening(&window_label(id)) else { return Ok(()) };

        let tool = app
            .state::<AppState>()
            .catalog()
            .and_then(|c| c.tools.into_iter().find(|t| t.id == id))
            .ok_or("Outil introuvable dans le catalogue.")?;
        // Overlay web (version compacte) par défaut ; page complète si elle a été choisie.
        let available = Available {
            app: false,
            overlay: tool.overlay_url.is_some(),
            web: tool.web_url.is_some(),
        };
        let raw_url = match web_mode(self.chosen_mode(id), available) {
            Some(OpenMode::Overlay) => tool.overlay_url.clone(),
            Some(_) => tool.web_url.clone(),
            None => None,
        }
        .ok_or("Cet outil n'a pas de version web.")?;
        let url = Url::parse(&raw_url).map_err(|_| "Adresse de la version web invalide.".to_string())?;
        if url.scheme() != "https" {
            return Err("La version web doit être en HTTPS.".into());
        }
        let host = url.host_str().ok_or("Adresse de la version web invalide.")?.to_string();

        let monitors = monitors(app);
        let (collapsed, opacity, locked, rect) = {
            let mut f = self.file.lock().map_err(|_| "État des widgets indisponible.")?;
            let locked = f.locked;
            let st = f.widget_mut(id);
            let rect = if st.collapsed && !st.pill.placements.is_empty() {
                st.pill.restore(&monitors, DEFAULT_WIDGET)
            } else {
                st.collapsed = false;
                st.window.restore(&monitors, DEFAULT_WIDGET)
            };
            (st.collapsed, st.opacity, locked, rect)
        };
        let rect = rect.unwrap_or(Rect::new(100, 100, 380, 520));

        let window = WindowBuilder::new(app, window_label(id))
            .title(format!("{} · Bworkshop", tool.name))
            .decorations(false)
            .shadow(false)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .focused(false)
            .resizable(true)
            .background_color(BACKGROUND)
            .build()
            .map_err(|e| format!("Fenêtre du widget non créée : {e}"))?;

        let first_page = url.to_string();
        let content = self.content_builder(app, id, host);
        let bar = WebviewBuilder::new(bar_label(id), WebviewUrl::App("widgetbar.html".into()))
            .additional_browser_args(state::BROWSER_ARGS)
            .focused(false)
            .background_color(BACKGROUND);
        let tiny = (PhysicalPosition::new(0, 0), PhysicalSize::new(1u32, 1u32));
        let created = window
            .add_child(content, tiny.0, tiny.1)
            .and_then(|_| window.add_child(bar, tiny.0, tiny.1));
        if let Err(e) = created {
            let _ = window.destroy();
            return Err(format!("Vues du widget non créées : {e}"));
        }

        let Some(hwnd) = native::hwnd(&window) else {
            let _ = window.destroy();
            return Err("Fenêtre du widget introuvable.".into());
        };
        // Noté ouvert seulement une fois la fenêtre bien créée.
        if let Ok(mut f) = self.file.lock() {
            f.widget_mut(id).open = true;
        }
        if let Ok(mut live) = self.live.lock() {
            live.insert(id.to_string(), Live { name: tool.name.clone(), hwnd: hwnd.0 as isize, settings_open: false });
        }
        match app.get_webview(&web_label(id)) {
            Some(web) => native::deny_permissions_then_open(&web, web_label(id), first_page),
            None => log::error!("Widget {id} : vue introuvable, page non chargée"),
        }

        let app2 = app.clone();
        let id2 = id.to_string();
        let hidden = self.hidden_all.load(Ordering::SeqCst);
        let hwnd_raw = hwnd.0 as isize;
        // Sous-classement et style : sur le fil qui possède la fenêtre.
        app.run_on_main_thread(move || {
            let hwnd = windows::Win32::Foundation::HWND(hwnd_raw as _);
            native::install_subclass(hwnd);
            if collapsed {
                native::set_fixed_size(hwnd, PILL_SIZE.0, PILL_SIZE.1);
            } else {
                native::set_min_size(hwnd, MIN_WIDGET_SIZE.0, MIN_WIDGET_SIZE.1);
            }
            native::set_rect(hwnd, rect);
            native::apply_style(hwnd, opacity, locked);
            let w = app2.state::<Widgets>();
            w.layout(&app2, &id2);
            if collapsed
                && let Some(web) = app2.get_webview(&web_label(&id2)) {
                    let _ = web.hide();
                    native::suspend(&web);
                }
            if !hidden {
                native::show_no_activate(hwnd);
            }
            w.emit_updated(&app2);
        })
        .map_err(|e| format!("Widget non affiché : {e}"))?;

        log::info!("Widget ouvert : {id}");
        self.touch();
        Ok(())
    }

    /// Vue de la page distante, avec toutes ses restrictions.
    /// Vue de la page de l'outil. Elle démarre sur une page vide : la vraie page n'est chargée
    /// qu'une fois le refus des autorisations (micro, caméra, position…) en place
    /// (voir `native::deny_permissions_then_open`).
    fn content_builder(&self, app: &AppHandle, id: &str, host: String) -> WebviewBuilder<tauri::Wry> {
        let blank = Url::parse("about:blank").expect("adresse fixe valide");
        let last_external = Arc::new(Mutex::new(None::<Instant>));
        let (app_nav, id_nav, last_nav, host_nav) = (app.clone(), id.to_string(), last_external.clone(), host.clone());
        let (app_new, id_new) = (app.clone(), id.to_string());
        let id_dl = id.to_string();
        WebviewBuilder::new(web_label(id), WebviewUrl::External(blank))
            .additional_browser_args(state::BROWSER_ARGS)
            .focused(false)
            .background_color(BACKGROUND)
            .on_navigation(move |u| {
                if domains::widget_navigation_allowed(u, &host_nav) {
                    return true;
                }
                open_outside(&app_nav, &last_nav, u, &id_nav);
                false
            })
            .on_new_window(move |u, _features| {
                log::info!("Widget {id_new} : fenêtre surgissante refusée");
                open_outside(&app_new, &last_external, &u, &id_new);
                NewWindowResponse::Deny
            })
            .on_download(move |_webview, event| {
                if let DownloadEvent::Requested { url, .. } = event {
                    log::warn!("Widget {id_dl} : téléchargement refusé ({})", domains::short(&url));
                }
                false
            })
    }

    /// Ferme un widget : sa fenêtre et ses vues sont détruites.
    pub fn close(&self, app: &AppHandle, id: &str) {
        self.mark_moved(id);
        self.flush(app);
        let removed = self.live.lock().ok().and_then(|mut l| l.remove(id));
        if let Ok(mut f) = self.file.lock() {
            let st = f.widget_mut(id);
            st.open = false;
            st.collapsed = false;
        }
        if let Some(l) = removed {
            native::forget_size_rules(windows::Win32::Foundation::HWND(l.hwnd as _));
        }
        if let Some(w) = app.get_window(&window_label(id)) {
            let _ = w.destroy();
        }
        log::info!("Widget fermé : {id}");
        self.touch();
        self.emit_updated(app);
    }

    pub fn reload(&self, app: &AppHandle, id: &str) {
        if let Some(web) = app.get_webview(&web_label(id)) {
            let _ = web.reload();
        }
    }

    // ---------- Disposition des vues ----------

    fn layout(&self, app: &AppHandle, id: &str) {
        let Some(window) = app.get_window(&window_label(id)) else { return };
        let collapsed = self.file.lock().map(|f| f.widgets.get(id).is_some_and(|w| w.collapsed)).unwrap_or(false);
        let settings_open = self.live.lock().map(|l| l.get(id).is_some_and(|w| w.settings_open)).unwrap_or(false);
        layout_views(app, &window, id, collapsed, settings_open);
    }

    // ---------- Actions de la barre ----------

    pub fn set_collapsed(&self, app: &AppHandle, id: &str, collapsed: bool) {
        let Some(hwnd) = self.hwnd_of(id) else { return };
        let monitors = monitors(app);
        let current = native::window_rect(hwnd);
        let target = {
            let Ok(mut f) = self.file.lock() else { return };
            let st = f.widget_mut(id);
            if st.collapsed == collapsed {
                return;
            }
            if let Some(r) = current {
                if collapsed {
                    st.window.remember(r, &monitors);
                } else {
                    st.pill.remember(r, &monitors);
                }
            }
            st.collapsed = collapsed;
            if collapsed {
                match (st.pill.placements.is_empty(), current) {
                    // Première fois : la pastille prend la place du coin haut-gauche du widget.
                    (true, Some(r)) => geometry::monitor_for(r, &monitors).map(|m| {
                        let pill = Rect::new(r.x, r.y, (PILL_SIZE.0 * m.scale).round() as u32, (PILL_SIZE.1 * m.scale).round() as u32);
                        geometry::clamp(pill, m)
                    }),
                    _ => st.pill.restore(&monitors, DEFAULT_WIDGET),
                }
            } else {
                st.window.restore(&monitors, DEFAULT_WIDGET)
            }
        };
        if let Ok(mut l) = self.live.lock()
            && let Some(w) = l.get_mut(id) {
                w.settings_open = false;
            }
        let web = app.get_webview(&web_label(id));
        if collapsed {
            native::set_fixed_size(hwnd, PILL_SIZE.0, PILL_SIZE.1);
            if let Some(web) = &web {
                let _ = web.hide();
                native::suspend(web);
            }
        } else {
            native::set_min_size(hwnd, MIN_WIDGET_SIZE.0, MIN_WIDGET_SIZE.1);
            if let Some(web) = &web {
                native::resume(web);
                let _ = web.show();
            }
        }
        if let Some(r) = target {
            native::set_rect(hwnd, r);
        }
        self.layout(app, id);
        self.emit_bar(app, id);
        self.touch();
    }

    pub fn toggle_collapsed(&self, app: &AppHandle, id: &str) {
        let collapsed = self.file.lock().map(|f| f.widgets.get(id).is_some_and(|w| w.collapsed)).unwrap_or(false);
        self.set_collapsed(app, id, !collapsed);
    }

    pub fn set_opacity(&self, id: &str, value: f64) {
        let value = widgets_store::clamp_opacity(value);
        let locked = {
            let Ok(mut f) = self.file.lock() else { return };
            f.widget_mut(id).opacity = value;
            f.locked
        };
        if let Some(hwnd) = self.hwnd_of(id) {
            native::apply_style(hwnd, value, locked);
        }
        self.touch();
    }

    pub fn set_settings_open(&self, app: &AppHandle, id: &str, open: bool) {
        if let Ok(mut l) = self.live.lock()
            && let Some(w) = l.get_mut(id) {
                w.settings_open = open;
            }
        self.layout(app, id);
        self.emit_bar(app, id);
    }

    // ---------- Actions globales ----------

    /// Verrouillage : tous les widgets laissent passer les clics.
    pub fn set_locked(&self, app: &AppHandle, locked: bool) {
        let opacities: BTreeMap<String, f64> = {
            let Ok(mut f) = self.file.lock() else { return };
            f.locked = locked;
            f.widgets.iter().map(|(k, v)| (k.clone(), v.opacity)).collect()
        };
        for (id, hwnd) in self.live_hwnds() {
            if locked {
                self.set_settings_open(app, &id, false);
            }
            native::apply_style(hwnd, opacities.get(&id).copied().unwrap_or(1.0), locked);
        }
        log::info!("Widgets {}", if locked { "verrouillés" } else { "déverrouillés" });
        self.touch();
        self.emit_all_bars(app);
        self.emit_updated(app);
    }

    pub fn set_hidden_all(&self, app: &AppHandle, hidden: bool) {
        self.hidden_all.store(hidden, Ordering::SeqCst);
        let toolbar = self.toolbar_hwnd_for_all().map(|h| (String::new(), h));
        for (_, hwnd) in self.live_hwnds().into_iter().chain(toolbar) {
            if hidden {
                native::hide(hwnd);
            } else {
                native::show_no_activate(hwnd);
            }
        }
        self.emit_updated(app);
        crate::tray::refresh(app); // libellé « Tout cacher » / « Tout montrer »
    }

    /// Réaffiche les derniers widgets : ceux qui sont cachés, sinon ceux
    /// qui étaient ouverts à la dernière fermeture du launcher.
    /// Renvoie faux s'il n'y a aucun widget à montrer.
    fn show_last_inner(&self, app: &AppHandle) -> bool {
        if self.hidden_all.load(Ordering::SeqCst) {
            self.set_hidden_all(app, false);
            return true;
        }
        let live: Vec<_> = self.live_hwnds().into_iter().chain(self.toolbar_hwnd_for_all().map(|h| (String::new(), h))).collect();
        if !live.is_empty() {
            for (_, hwnd) in live {
                if !native::is_visible(hwnd) {
                    native::show_no_activate(hwnd);
                }
            }
            return true;
        }
        let to_open: Vec<String> = self
            .file
            .lock()
            .map(|f| f.widgets.iter().filter(|(_, w)| w.open).map(|(k, _)| k.clone()).collect())
            .unwrap_or_default();
        if to_open.is_empty() {
            return false;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            for id in to_open {
                if let Err(e) = app.state::<Widgets>().open(&app, &id) {
                    log::warn!("Widget {id} non rouvert : {e}");
                }
            }
        });
        true
    }

    /// Événements de la fenêtre de la barre d'outils.
    pub fn on_toolbar_event(&self, window: &Window, event: &WindowEvent) {
        match event {
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                self.close_toolbar(window.app_handle());
            }
            WindowEvent::Moved(_) => self.mark_moved(toolbar::MOVED_KEY),
            _ => {}
        }
    }

    /// Événements des fenêtres de widget.
    pub fn on_window_event(&self, window: &Window, id: &str, event: &WindowEvent) {
        let app = window.app_handle();
        match event {
            WindowEvent::CloseRequested { api, .. } => {
                // Alt+F4 sur un widget : on le ferme proprement.
                api.prevent_close();
                self.close(app, id);
            }
            WindowEvent::Moved(_) => self.mark_moved(id),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.layout(app, id);
                self.mark_moved(id);
            }
            _ => {}
        }
    }
}

/// Place la barre et la page dans la fenêtre (pixels physiques).
fn layout_views(app: &AppHandle, window: &Window, id: &str, collapsed: bool, settings_open: bool) {
    let Ok(size) = window.inner_size() else { return };
    let scale = window.scale_factor().unwrap_or(1.0);
    let px = |v: f64| (v * scale).round() as u32;
    let inset = px(INSET);
    let bar_h = px(BAR_HEIGHT);
    let w = size.width.saturating_sub(2 * inset).max(1);
    let h = size.height.saturating_sub(2 * inset).max(1);
    let place = |label: String, y: u32, height: u32| {
        if let Some(v) = app.get_webview(&label) {
            let _ = v.set_bounds(tauri::Rect {
                position: PhysicalPosition::new(inset as i32, y as i32).into(),
                size: PhysicalSize::new(w, height.max(1)).into(),
            });
        }
    };
    if collapsed {
        place(bar_label(id), inset, h);
    } else {
        let bar_total = if settings_open { bar_h + px(SETTINGS_HEIGHT) } else { bar_h };
        place(web_label(id), inset + bar_h, h.saturating_sub(bar_h));
        place(bar_label(id), inset, bar_total.min(h));
    }
}

/// Ouvre un lien de la page dans le navigateur par défaut (http et https seulement).
fn open_outside(app: &AppHandle, last: &Mutex<Option<Instant>>, url: &Url, id: &str) {
    if !matches!(url.scheme(), "http" | "https") {
        log::warn!("Widget {id} : lien refusé (schéma {})", url.scheme());
        return;
    }
    if let Ok(mut l) = last.lock() {
        if l.is_some_and(|t| t.elapsed() < EXTERNAL_OPEN_INTERVAL) {
            log::warn!("Widget {id} : trop de liens externes, ouverture ignorée");
            return;
        }
        *l = Some(Instant::now());
    }
    log::info!("Widget {id} : lien ouvert dans le navigateur ({})", domains::short(url));
    if let Err(e) = app.opener().open_url(url.as_str(), None::<&str>) {
        log::warn!("Navigateur non ouvert : {e}");
    }
}

/// Écrans connus de Windows, au format du cœur.
pub fn monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let primary = app.primary_monitor().ok().flatten();
    app.available_monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|m| {
            let wa = m.work_area();
            let is_primary = primary.as_ref().is_some_and(|p| p.position() == m.position() && p.size() == m.size());
            MonitorInfo {
                key: format!("{} {}x{}", m.name().map(String::as_str).unwrap_or("?"), m.size().width, m.size().height),
                work: Rect::new(wa.position.x, wa.position.y, wa.size.width, wa.size.height),
                scale: m.scale_factor(),
                primary: is_primary,
            }
        })
        .collect()
}

/// Clic gauche sur l'icône : réafficher les derniers widgets (sinon la fenêtre principale).
pub fn show_last(app: &AppHandle) {
    if !app.state::<Widgets>().show_last_inner(app) {
        state::show_main(app);
    }
}

/// « Tout cacher » / « Tout montrer ».
pub fn toggle_hide_all(app: &AppHandle) {
    let w = app.state::<Widgets>();
    let hidden = !w.hidden_all.load(Ordering::SeqCst);
    w.set_hidden_all(app, hidden);
}

/// Ce qui existe pour un outil : logiciel installé, overlay web, page web.
pub fn available(app: &AppHandle, id: &str) -> Available {
    let tool = app.state::<AppState>().catalog().and_then(|c| c.tools.into_iter().find(|t| t.id == id));
    Available {
        app: crate::installer_bridge::is_installed(app, id),
        overlay: tool.as_ref().is_some_and(|t| t.overlay_url.is_some()),
        web: tool.as_ref().is_some_and(|t| t.web_url.is_some()),
    }
}

/// Ouvre ou ferme un outil selon son mode (tuile de la barre, raccourci). Hors du fil principal.
pub fn toggle_tool(app: &AppHandle, id: &str) -> Result<(), String> {
    let widgets = app.state::<Widgets>();
    match widgets_store::effective_mode(widgets.chosen_mode(id), available(app, id)) {
        Some(OpenMode::App) => {
            let running = app.state::<crate::running::Running>();
            if running.can_force(id) {
                crate::running::force_close(app, id)
            } else if running.is_running(id) {
                crate::running::close(app, id)
            } else {
                crate::installer_bridge::launch_tool(app, id)
            }
        }
        Some(_) => {
            if widgets.view().open.iter().any(|x| x == id) {
                widgets.close(app, id);
                Ok(())
            } else {
                widgets.open(app, id)
            }
        }
        None => Err("Cet outil n'est ni installé ni disponible en version web.".into()),
    }
}
