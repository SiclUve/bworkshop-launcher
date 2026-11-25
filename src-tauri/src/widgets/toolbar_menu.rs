//! Menu du clic droit sur une tuile de la barre d'outils (menu natif de Windows).
//!
//! Les actions passent par les mêmes fonctions que la fenêtre principale :
//! lancer, fermer, forcer (après une fermeture polie), mettre à jour, ouvrir le dossier,
//! désinstaller (après confirmation).

use super::{toolbar, Widgets};
use crate::state::{self, AppState};
use crate::{installer_bridge, running};
use bworkshop_core::settings::Language;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Window, Wry};

/// Préfixe des identifiants d'entrées (le menu de la zone de notification a les siens).
const PREFIX: &str = "tb|";
/// Message court affiché dans la barre (erreur ou information).
pub const EVENT_NOTICE: &str = "toolbar-notice";

fn tr(lang: Language, fr: &'static str, en: &'static str) -> &'static str {
    match lang {
        Language::Fr => fr,
        Language::En => en,
    }
}

fn tool_name(app: &AppHandle, id: &str) -> Option<String> {
    app.state::<AppState>().catalog()?.tools.into_iter().find(|t| t.id == id).map(|t| t.name)
}

/// Affiche le menu de l'outil `id` sous la souris.
pub fn popup(app: &AppHandle, window: &Window, id: &str) -> Result<(), String> {
    let name = tool_name(app, id).ok_or("Outil inconnu du catalogue")?;
    let lang = app.state::<AppState>().settings().language;
    let item = |action: &str, text: String| {
        MenuItem::with_id(app, format!("{PREFIX}{action}|{id}"), text, true, None::<&str>).map_err(|e| e.to_string())
    };
    let r = app.state::<running::Running>();
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    // Titre grisé : le nom de l'outil.
    items.push(Box::new(MenuItem::new(app, &name, false, None::<&str>).map_err(|e| e.to_string())?));
    // Outil web seulement (non installé) : ouvrir/fermer sa fenêtre web, rien d'autre.
    if !installer_bridge::is_installed(app, id) {
        let open = app.state::<Widgets>().view().open.iter().any(|x| x == id);
        let text = if open { tr(lang, "Fermer la version web", "Close web version") } else { tr(lang, "Ouvrir sur le jeu", "Open over the game") };
        items.push(Box::new(item("toggle", text.into())?));
        items.push(Box::new(item("show", tr(lang, "Voir dans le launcher", "Show in launcher").into())?));
        items.push(Box::new(item("hide", tr(lang, "Retirer de la barre", "Remove from bar").into())?));
        let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
        let menu = Menu::with_items(app, &refs).map_err(|e| e.to_string())?;
        return window.popup_menu(&menu).map_err(|e| e.to_string());
    }
    let main = if r.can_force(id) {
        item("force", tr(lang, "Arrêter complètement", "Stop completely").into())?
    } else if r.is_running(id) {
        item("close", tr(lang, "Fermer", "Close").into())?
    } else {
        item("launch", tr(lang, "Lancer", "Launch").into())?
    };
    items.push(Box::new(main));
    if installer_bridge::has_update(app, id) {
        items.push(Box::new(item("update", tr(lang, "Mettre à jour", "Update").into())?));
    }
    items.push(Box::new(item("folder", tr(lang, "Ouvrir le dossier", "Open folder").into())?));
    items.push(Box::new(item("show", tr(lang, "Voir dans le launcher", "Show in launcher").into())?));
    items.push(Box::new(item("hide", tr(lang, "Retirer de la barre", "Remove from bar").into())?));
    items.push(Box::new(PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?));
    items.push(Box::new(item("uninstall", tr(lang, "Désinstaller…", "Uninstall…").into())?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    let menu = Menu::with_items(app, &refs).map_err(|e| e.to_string())?;
    window.popup_menu(&menu).map_err(|e| e.to_string())
}

/// Menu de la barre (clic droit sur « + ») : remettre des outils retirés, orientation, fermer.
pub fn popup_bar(app: &AppHandle, window: &Window) -> Result<(), String> {
    let lang = app.state::<AppState>().settings().language;
    let prefs = app.state::<Widgets>().toolbar_prefs();
    let err = |e: tauri::Error| e.to_string();
    let item = |action: &str, arg: &str, text: String| {
        MenuItem::with_id(app, format!("{PREFIX}{action}|{arg}"), text, true, None::<&str>).map_err(err)
    };
    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    if !prefs.hidden.is_empty() {
        let mut restore: Vec<MenuItem<Wry>> = Vec::new();
        for id in &prefs.hidden {
            let name = tool_name(app, id).unwrap_or_else(|| id.clone());
            restore.push(item("unhide", id, name)?);
        }
        let refs: Vec<&dyn IsMenuItem<Wry>> = restore.iter().map(|m| m as &dyn IsMenuItem<Wry>).collect();
        let sub = Submenu::with_items(app, tr(lang, "Remettre dans la barre", "Put back in bar"), true, &refs).map_err(err)?;
        items.push(Box::new(sub));
    }
    // Dispositions : un clic pour en rappeler une, ou enregistrer l'état actuel.
    let layouts_view = app.state::<crate::layouts::Layouts>().view();
    let mut entries: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for (i, l) in layouts_view.layouts.iter().enumerate() {
        let label = if i < 4 { format!("{}. {}", i + 1, l.name) } else { l.name.clone() };
        let checked = layouts_view.active == Some(i);
        entries.push(Box::new(
            CheckMenuItem::with_id(app, format!("{PREFIX}layout|{i}"), label, true, checked, None::<&str>).map_err(err)?,
        ));
    }
    if !entries.is_empty() {
        entries.push(Box::new(PredefinedMenuItem::separator(app).map_err(err)?));
    }
    entries.push(Box::new(item("layout-save", "-", tr(lang, "Enregistrer l'état actuel…", "Save current state…").into())?));
    let refs: Vec<&dyn IsMenuItem<Wry>> = entries.iter().map(|b| b.as_ref()).collect();
    items.push(Box::new(Submenu::with_items(app, tr(lang, "Dispositions", "Layouts"), true, &refs).map_err(err)?));
    let vertical = CheckMenuItem::with_id(
        app,
        format!("{PREFIX}vertical|-"),
        tr(lang, "Barre verticale", "Vertical bar"),
        true,
        prefs.vertical,
        None::<&str>,
    )
    .map_err(err)?;
    items.push(Box::new(vertical));
    items.push(Box::new(PredefinedMenuItem::separator(app).map_err(err)?));
    items.push(Box::new(item("close-bar", "-", tr(lang, "Fermer la barre", "Close the bar").into())?));
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    let menu = Menu::with_items(app, &refs).map_err(err)?;
    window.popup_menu(&menu).map_err(err)
}

/// Affiche un message dans la barre.
pub fn notice(app: &AppHandle, message: impl Into<String>) {
    let _ = app.emit_to(toolbar::LABEL, EVENT_NOTICE, message.into());
}

/// Choix d'une entrée du menu (appelé pour tous les menus : on ne garde que les nôtres).
pub fn on_event(app: &AppHandle, event: &MenuEvent) {
    let Some(rest) = event.id().as_ref().strip_prefix(PREFIX) else { return };
    let Some((action, id)) = rest.split_once('|') else { return };
    let lang = app.state::<AppState>().settings().language;
    let result = match action {
        "launch" => installer_bridge::launch_tool(app, id),
        "toggle" => {
            // Créer une fenêtre web ne doit pas bloquer le fil principal.
            let (app, id) = (app.clone(), id.to_string());
            std::thread::spawn(move || {
                if let Err(e) = crate::widgets::toggle_tool(&app, &id) {
                    notice(&app, e);
                }
            });
            Ok(())
        }
        "close" => running::close(app, id),
        "force" => running::force_close(app, id),
        "folder" => installer_bridge::open_folder(app, id),
        "show" => {
            state::show_main(app);
            Ok(())
        }
        "update" => match installer_bridge::install(app, id, false) {
            Ok(()) => {
                notice(app, tr(lang, "Mise à jour en cours…", "Updating…"));
                Ok(())
            }
            // Sans empreinte publiée : la confirmation se fait dans la fenêtre principale.
            Err(e) if e == installer_bridge::UNVERIFIED => {
                state::show_main(app);
                Err(tr(lang, "Cette mise à jour demande ta confirmation dans le launcher.", "This update needs your confirmation in the launcher.").into())
            }
            Err(e) => Err(e),
        },
        "uninstall" => {
            confirm_uninstall(app, id);
            Ok(())
        }
        "hide" => {
            app.state::<Widgets>().set_tool_in_bar(app, id, false);
            notice(app, tr(lang, "Outil retiré. Remets-le depuis « Mes plugins » ou clic droit sur « + ».", "Tool removed. Put it back from « My plugins » or right-click « + »."));
            Ok(())
        }
        "unhide" => {
            app.state::<Widgets>().set_tool_in_bar(app, id, true);
            Ok(())
        }
        "vertical" => {
            let widgets = app.state::<Widgets>();
            let vertical = !widgets.toolbar_prefs().vertical;
            widgets.set_toolbar_vertical(app, vertical);
            Ok(())
        }
        "layout" => id.parse::<usize>().map_err(|e| e.to_string()).and_then(|i| crate::layouts::apply(app, i)),
        "layout-save" => {
            // Le nom se choisit dans le launcher : on l'ouvre sur la fenêtre des dispositions.
            crate::layouts::request_open(app);
            Ok(())
        }
        "close-bar" => {
            app.state::<Widgets>().close_toolbar(app);
            Ok(())
        }
        _ => Ok(()),
    };
    if let Err(e) = result {
        notice(app, e);
    }
}

/// Demande confirmation (boîte de dialogue du launcher), puis désinstalle.
fn confirm_uninstall(app: &AppHandle, id: &str) {
    let name = tool_name(app, id).unwrap_or_else(|| id.to_string());
    let lang = app.state::<AppState>().settings().language;
    let (app, id) = (app.clone(), id.to_string());
    // La boîte bloque son fil : jamais le fil principal.
    std::thread::spawn(move || {
        let text = match lang {
            Language::Fr => format!("Désinstaller {name} ?"),
            Language::En => format!("Uninstall {name}?"),
        };
        if !native_confirm(tr(lang, "Bworkshop Launcher", "Bworkshop Launcher"), &text) {
            return;
        }
        match installer_bridge::uninstall(&app, &id) {
            Ok(()) => notice(&app, tr(lang, "Désinstallation en cours…", "Uninstalling…")),
            Err(e) => notice(&app, e),
        }
    });
}

fn native_confirm(title: &str, text: &str) -> bool {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONQUESTION, MB_SETFOREGROUND, MB_TOPMOST, MB_YESNO,
    };
    let style = MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2 | MB_TOPMOST | MB_SETFOREGROUND;
    unsafe { MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(title), style) == IDYES }
}
