//! État partagé de l'application.

use bworkshop_core::catalog::CatalogSnapshot;
use bworkshop_core::paths::AppPaths;
use bworkshop_core::settings::{Settings, SettingsStore};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewWindowBuilder};

pub const MAIN_LABEL: &str = "main";

/// Options du moteur web, **identiques pour toutes les fenêtres** (sinon WebView2 refuse d'en créer).
/// `--disable-gpu` : rendu sans carte graphique, ≈ 60 Mo de moins pour des pages simples.
/// Doit rester égal à `additionalBrowserArgs` dans `tauri.conf.json`.
pub const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu";

pub struct AppState {
    pub paths: AppPaths,
    pub store: SettingsStore,
    pub settings: Mutex<Settings>,
    /// Dernier catalogue connu (cache puis réseau).
    pub catalog: Mutex<Option<CatalogSnapshot>>,
}

impl AppState {
    pub fn new(paths: AppPaths, store: SettingsStore, settings: Settings) -> Self {
        Self { paths, store, settings: Mutex::new(settings), catalog: Mutex::new(None) }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn catalog(&self) -> Option<CatalogSnapshot> {
        self.catalog.lock().ok().and_then(|c| c.clone())
    }

    /// Enregistre de nouveaux réglages (normalisés) et les rend actifs.
    pub fn replace_settings(&self, settings: Settings) -> Result<Settings, String> {
        let settings = settings.normalized();
        self.store
            .save(&settings)
            .map_err(|e| format!("Impossible d'enregistrer les réglages : {e}"))?;
        if let Ok(mut current) = self.settings.lock() {
            *current = settings.clone();
        }
        Ok(settings)
    }
}

/// Création de la fenêtre principale en cours (évite d'en créer deux sur un double clic).
static CREATING_MAIN: AtomicBool = AtomicBool::new(false);

/// Affiche la fenêtre principale. Appelé seulement sur une action de l'utilisateur.
///
/// Elle n'existe que lorsqu'elle est affichée : cachée, elle est détruite pour rendre la
/// mémoire de sa page (≈ 30 Mo). La recréer prend un court instant.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_LABEL) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        emit_main_visible(app);
        return;
    }
    if CREATING_MAIN.swap(true, Ordering::SeqCst) {
        return;
    }
    // Créer une fenêtre depuis le fil principal bloquerait : on passe par un autre fil.
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("fenetre-principale".into()).spawn(move || {
        let config = app.config().app.windows.iter().find(|w| w.label == MAIN_LABEL).cloned();
        let built = config
            .ok_or_else(|| "configuration absente".to_string())
            .and_then(|c| WebviewWindowBuilder::from_config(&app, &c).and_then(|b| b.build()).map_err(|e| e.to_string()));
        match built {
            Ok(w) => {
                let _ = w.show();
                let _ = w.set_focus();
            }
            Err(e) => log::error!("Fenêtre principale non créée : {e}"),
        }
        CREATING_MAIN.store(false, Ordering::SeqCst);
        emit_main_visible(&app);
    });
    if spawned.is_err() {
        CREATING_MAIN.store(false, Ordering::SeqCst);
    }
}

/// Cache la fenêtre principale (le launcher reste près de l'horloge) : elle est fermée,
/// ce qui libère sa page. `RunEvent::ExitRequested` empêche alors le launcher de quitter.
pub fn hide_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_LABEL) {
        let _ = w.close();
    }
}

/// Événement envoyé à la barre d'outils quand la fenêtre principale s'ouvre ou se cache.
pub const EVENT_MAIN_VISIBLE: &str = "main-visible";
/// Réglages enregistrés (langue, favoris…) : la barre d'outils suit.
pub const EVENT_SETTINGS: &str = "settings-updated";

/// Prévient la barre d'outils que les réglages ont changé.
pub fn emit_settings(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit_to(crate::widgets::toolbar::LABEL, EVENT_SETTINGS, app.state::<AppState>().settings());
}

/// Fenêtre principale affichée (visible et pas réduite).
pub fn main_visible(app: &AppHandle) -> bool {
    app.get_webview_window(MAIN_LABEL)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
}

/// Prévient la barre d'outils (tuile « + » / « − »).
pub fn emit_main_visible(app: &AppHandle) {
    use tauri::Emitter;
    let _ = app.emit_to(crate::widgets::toolbar::LABEL, EVENT_MAIN_VISIBLE, main_visible(app));
}

/// Aligne l'entrée « lancer au démarrage » de Windows (HKCU\…\Run) sur le réglage.
/// L'erreur est rendue (et journalisée) : l'appelant décide de la montrer.
pub fn sync_autostart(app: &AppHandle) -> Result<(), String> {
    let wanted = app.state::<AppState>().settings().autostart;
    let result = std::env::current_exe()
        .map_err(|e| format!("exécutable introuvable ({e})"))
        .and_then(|exe| crate::autostart::sync(wanted, &exe, crate::HIDDEN_ARG).map_err(|e| e.to_string()));
    match result {
        Ok(true) => log::info!("Lancement au démarrage de Windows : {}", if wanted { "activé" } else { "désactivé" }),
        Ok(false) => {}
        Err(ref e) => log::warn!("Impossible de modifier le lancement au démarrage : {e}"),
    }
    result.map(|_| ()).map_err(|e| format!("Impossible de modifier le lancement au démarrage : {e}"))
}
