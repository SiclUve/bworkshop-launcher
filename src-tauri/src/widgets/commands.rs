//! Commandes des widgets.
//!
//! - Fenêtre principale : ouvrir, fermer, verrouiller, tout cacher.
//! - Barre d'un widget (`wbar-<id>`) : n'agit que sur **son** widget, déduit de
//!   l'étiquette de la vue appelante (jamais d'un argument).

use super::{id_of_bar, BarState, Widgets, WidgetsView};
use tauri::{AppHandle, Manager, State, Webview};

// ---------- Fenêtre principale ----------

#[tauri::command]
pub fn get_widgets(widgets: State<'_, Widgets>) -> WidgetsView {
    widgets.view()
}

/// Asynchrone : la création d'une vue WebView2 ne doit pas bloquer le fil principal.
#[tauri::command]
pub async fn open_widget(app: AppHandle, id: String) -> Result<(), String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || handle.state::<Widgets>().open(&handle, &id))
        .await
        .map_err(|e| format!("Ouverture interrompue : {e}"))?
}

#[tauri::command]
pub fn close_widget(app: AppHandle, widgets: State<'_, Widgets>, id: String) {
    widgets.close(&app, &id);
}

#[tauri::command]
pub fn set_widgets_locked(app: AppHandle, widgets: State<'_, Widgets>, locked: bool) {
    widgets.set_locked(&app, locked);
}

#[tauri::command]
pub fn set_widgets_hidden(app: AppHandle, widgets: State<'_, Widgets>, hidden: bool) {
    widgets.set_hidden_all(&app, hidden);
}

/// Réglages « Mes plugins » : tuiles affichées et mode d'ouverture de chaque outil.
#[tauri::command]
pub fn get_toolbar_prefs(widgets: State<'_, Widgets>) -> super::toolbar::ToolbarPrefs {
    widgets.toolbar_prefs()
}

#[tauri::command]
pub fn set_tool_in_bar(app: AppHandle, widgets: State<'_, Widgets>, id: String, value: bool) -> Result<(), String> {
    if !super::valid_id(&id) {
        return Err("Identifiant d'outil invalide.".into());
    }
    widgets.set_tool_in_bar(&app, &id, value);
    Ok(())
}

/// `mode` absent : choix automatique (logiciel, sinon overlay, sinon page web).
#[tauri::command]
pub async fn set_tool_mode(app: AppHandle, id: String, mode: Option<bworkshop_core::widgets_store::OpenMode>) -> Result<(), String> {
    if !super::valid_id(&id) {
        return Err("Identifiant d'outil invalide.".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || handle.state::<Widgets>().set_tool_mode(&handle, &id, mode))
        .await
        .map_err(|e| e.to_string())
}

/// Ouvre ou ferme un outil selon son mode (tuile de la barre, bouton de la fenêtre principale).
#[tauri::command]
pub async fn toggle_tool(app: AppHandle, id: String) -> Result<(), String> {
    if !super::valid_id(&id) {
        return Err("Identifiant d'outil invalide.".into());
    }
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || super::toggle_tool(&handle, &id))
        .await
        .map_err(|e| format!("Action interrompue : {e}"))?
}

// ---------- Barre d'un widget ----------

fn own_id(webview: &Webview) -> Result<String, String> {
    id_of_bar(webview.label()).map(str::to_string).ok_or_else(|| "Commande réservée aux barres de widget.".into())
}

#[tauri::command]
pub fn widget_bar_state(app: AppHandle, widgets: State<'_, Widgets>, webview: Webview) -> Result<Option<BarState>, String> {
    Ok(widgets.bar_state(&app, &own_id(&webview)?))
}

#[tauri::command]
pub fn widget_close(app: AppHandle, widgets: State<'_, Widgets>, webview: Webview) -> Result<(), String> {
    widgets.close(&app, &own_id(&webview)?);
    Ok(())
}

#[tauri::command]
pub fn widget_toggle_collapse(app: AppHandle, widgets: State<'_, Widgets>, webview: Webview) -> Result<(), String> {
    widgets.toggle_collapsed(&app, &own_id(&webview)?);
    Ok(())
}

#[tauri::command]
pub fn widget_reload(app: AppHandle, widgets: State<'_, Widgets>, webview: Webview) -> Result<(), String> {
    widgets.reload(&app, &own_id(&webview)?);
    Ok(())
}

#[tauri::command]
pub fn widget_set_opacity(widgets: State<'_, Widgets>, webview: Webview, value: f64) -> Result<(), String> {
    widgets.set_opacity(&own_id(&webview)?, value);
    Ok(())
}

#[tauri::command]
pub fn widget_settings(app: AppHandle, widgets: State<'_, Widgets>, webview: Webview, open: bool) -> Result<(), String> {
    widgets.set_settings_open(&app, &own_id(&webview)?, open);
    Ok(())
}

/// Déplacement à la souris depuis la barre (bouton gauche enfoncé).
#[tauri::command]
pub fn widget_drag(webview: Webview) -> Result<(), String> {
    own_id(&webview)?;
    webview.window().start_dragging().map_err(|e| e.to_string())
}

// ---------- Barre d'outils ----------

/// Fenêtre principale : ouvrir ou fermer la barre d'outils.
#[tauri::command]
pub async fn set_toolbar_open(app: AppHandle, open: bool) -> Result<(), String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let w = handle.state::<Widgets>();
        if open {
            w.open_toolbar(&handle)
        } else {
            w.close_toolbar(&handle);
            Ok(())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn from_toolbar(webview: &Webview) -> Result<(), String> {
    if webview.label() == super::toolbar::LABEL { Ok(()) } else { Err("Commande réservée à la barre d'outils.".into()) }
}

/// Réglages de la barre (tuiles retirées, ordre, orientation).
#[tauri::command]
pub fn toolbar_prefs(widgets: State<'_, Widgets>, webview: Webview) -> Result<super::toolbar::ToolbarPrefs, String> {
    from_toolbar(&webview)?;
    Ok(widgets.toolbar_prefs())
}

/// Ordre des tuiles après un glisser.
#[tauri::command]
pub fn toolbar_set_order(app: AppHandle, widgets: State<'_, Widgets>, webview: Webview, order: Vec<String>) -> Result<(), String> {
    from_toolbar(&webview)?;
    widgets.set_toolbar_order(&app, order);
    Ok(())
}

/// Clic droit sur la tuile « + » : menu de la barre.
#[tauri::command]
pub fn toolbar_bar_menu(app: AppHandle, webview: Webview) -> Result<(), String> {
    from_toolbar(&webview)?;
    super::toolbar_menu::popup_bar(&app, &webview.window())
}

/// Clic droit sur une tuile : menu de l'outil.
#[tauri::command]
pub fn toolbar_menu(app: AppHandle, webview: Webview, id: String) -> Result<(), String> {
    from_toolbar(&webview)?;
    super::toolbar_menu::popup(&app, &webview.window(), &id)
}

/// La barre a `count` tuiles (« + » comprise), et peut-être un message : sa taille suit.
#[tauri::command]
pub fn toolbar_fit(app: AppHandle, widgets: State<'_, Widgets>, webview: Webview, count: usize, message: bool) -> Result<(), String> {
    from_toolbar(&webview)?;
    widgets.fit_toolbar(&app, count.min(200), message);
    Ok(())
}

/// Coin de la barre tiré : nouvelle taille des tuiles (`done` : fin du glisser, enregistrée).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn toolbar_resize(
    app: AppHandle,
    widgets: State<'_, Widgets>,
    webview: Webview,
    tile: u32,
    count: usize,
    message: bool,
    corner: super::toolbar::Corner,
    done: bool,
) -> Result<(), String> {
    from_toolbar(&webview)?;
    widgets.resize_toolbar(&app, tile, count.min(200), message, corner, done);
    Ok(())
}

#[tauri::command]
pub fn toolbar_drag(webview: Webview) -> Result<(), String> {
    from_toolbar(&webview)?;
    webview.window().start_dragging().map_err(|e| e.to_string())
}

/// Tuile « + » / « − » : ouvre ou cache la fenêtre principale. Renvoie son nouvel état.
#[tauri::command]
pub fn toolbar_open_main(app: AppHandle, webview: Webview) -> Result<bool, String> {
    from_toolbar(&webview)?;
    if crate::state::main_visible(&app) {
        crate::state::hide_main(&app);
    } else {
        crate::state::show_main(&app);
    }
    Ok(crate::state::main_visible(&app))
}

/// État de la fenêtre principale, pour la tuile « + » / « − ».
#[tauri::command]
pub fn toolbar_main_visible(app: AppHandle, webview: Webview) -> Result<bool, String> {
    from_toolbar(&webview)?;
    Ok(crate::state::main_visible(&app))
}
