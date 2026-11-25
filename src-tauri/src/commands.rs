//! Commandes appelables depuis les pages locales du launcher.

use crate::catalog_bridge;
use crate::installer_bridge;
use crate::state::{self, AppState};
use crate::tray;
use bworkshop_core::catalog::CatalogSnapshot;
use bworkshop_core::settings::Settings;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    let settings = state.replace_settings(settings)?;
    state::sync_autostart(&app);
    tray::refresh(&app); // la langue du menu a pu changer
    tauri::Manager::state::<crate::widgets::Widgets>(&app).emit_all_bars(&app);
    crate::hotkeys::reload(&app); // les raccourcis ont pu changer
    crate::dofus::reload(); // l'option « Suivre Dofus » a pu changer
    Ok(settings)
}

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo { version: env!("CARGO_PKG_VERSION") }
}

/// Dernier catalogue connu (peut être celui du cache, en attendant la vérification).
#[tauri::command]
pub fn get_catalog(state: State<'_, AppState>) -> Option<CatalogSnapshot> {
    state.catalog()
}

/// Icônes des outils déjà en cache (identifiant → adresse `data:`).
#[tauri::command]
pub fn get_icons(
    state: State<'_, AppState>,
    bridge: State<'_, std::sync::Arc<catalog_bridge::CatalogBridge>>,
) -> std::collections::BTreeMap<String, String> {
    state.catalog().map(|snap| bridge.icons_for(&snap)).unwrap_or_default()
}

/// Bouton « Actualiser » : vérification immédiate, hors du fil de l'interface.
#[tauri::command]
pub async fn refresh_catalog(app: AppHandle) -> Result<Option<CatalogSnapshot>, String> {
    let handle = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || catalog_bridge::refresh_now(&handle))
        .await
        .map_err(|e| format!("Actualisation interrompue : {e}"))?;
    // Déjà en cours : on renvoie l'état actuel, l'événement suivra.
    Ok(result.or_else(|| tauri::Manager::state::<AppState>(&app).catalog()))
}

/// Ajoute ou retire un favori.
#[tauri::command]
pub fn toggle_favorite(state: State<'_, AppState>, id: String) -> Result<Settings, String> {
    let mut settings = state.settings();
    if let Some(pos) = settings.favorites.iter().position(|f| f == &id) {
        settings.favorites.remove(pos);
    } else {
        settings.favorites.push(id);
    }
    state.replace_settings(settings)
}

// ---------- Installation ----------

#[tauri::command]
pub fn get_installs(app: AppHandle) -> installer_bridge::InstallsView {
    installer_bridge::view(&app)
}

/// Installe ou met à jour. Refusé si le catalogue ne publie pas d'empreinte.
#[tauri::command]
pub fn install_tool(app: AppHandle, id: String) -> Result<(), String> {
    installer_bridge::dismiss(&app, &id);
    installer_bridge::install(&app, &id)
}

#[tauri::command]
pub fn cancel_install(app: AppHandle, id: String) {
    installer_bridge::cancel(&app, &id);
}

#[tauri::command]
pub fn dismiss_install_error(app: AppHandle, id: String) {
    installer_bridge::dismiss(&app, &id);
}

/// Désinstalle (la confirmation est demandée par l'interface avant l'appel).
#[tauri::command]
pub fn uninstall_tool(app: AppHandle, id: String) -> Result<(), String> {
    installer_bridge::dismiss(&app, &id);
    installer_bridge::uninstall(&app, &id)
}

#[tauri::command]
pub fn launch_tool(app: AppHandle, id: String) -> Result<(), String> {
    installer_bridge::launch_tool(&app, &id)
}

/// Logiciels lancés par le launcher et encore ouverts.
#[tauri::command]
pub fn get_running(running: State<'_, crate::running::Running>) -> crate::running::RunningView {
    running.view()
}

/// Un logiciel lancé par le launcher a-t-il encore une fenêtre visible ?
#[tauri::command]
pub fn tool_has_window(running: State<'_, crate::running::Running>, id: String) -> bool {
    running.has_window(&id)
}

/// Ferme poliment un logiciel lancé par le launcher (jamais Dofus).
#[tauri::command]
pub fn close_tool(app: AppHandle, id: String) -> Result<(), String> {
    crate::running::close(&app, &id)
}

/// Arrêt forcé, seulement après une fermeture polie restée sans effet.
#[tauri::command]
pub fn force_close_tool(app: AppHandle, id: String) -> Result<(), String> {
    crate::running::force_close(&app, &id)
}

/// État des raccourcis (actifs, refusés et pourquoi).
#[tauri::command]
pub fn get_hotkeys_status(hotkeys: State<'_, crate::hotkeys::Hotkeys>) -> Vec<crate::hotkeys::HotkeyStatus> {
    hotkeys.status()
}

/// Suspend les raccourcis pendant la saisie d'un nouveau raccourci.
#[tauri::command]
pub fn suspend_hotkeys(app: AppHandle, suspended: bool) {
    crate::hotkeys::set_suspended(&app, suspended);
}

/// Ouvre un lien dans le navigateur par défaut. Seuls les liens présents dans le
/// catalogue sont acceptés : une page ne peut pas faire ouvrir une adresse arbitraire.
#[tauri::command]
pub fn open_external(app: AppHandle, state: State<'_, AppState>, url: String) -> Result<(), String> {
    let known = state
        .catalog()
        .map(|c| c.tools.iter().any(|t| t.links().any(|l| l == url)))
        .unwrap_or(false);
    if !known || !url.starts_with("https://") {
        log::warn!("Ouverture refusée d'un lien hors catalogue");
        return Err("Lien refusé".into());
    }
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| format!("Impossible d'ouvrir le lien : {e}"))
}
