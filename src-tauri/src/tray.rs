//! Icône de la zone de notification.
//! - Clic gauche : réafficher les derniers widgets.
//! - Clic droit : Ouvrir, Dispositions, Tout cacher (ou Tout montrer), Quitter.

use crate::i18n::t;
use crate::state::{self, AppState};
use crate::widgets;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

const TRAY_ID: &str = "main";

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let lang = app.state::<AppState>().settings().language;
    let open = MenuItem::with_id(app, "open", t(lang, "open"), true, None::<&str>)?;
    let names = app.state::<crate::layouts::Layouts>().names();
    let layouts = Submenu::with_id(app, "layouts", t(lang, "layouts"), true)?;
    if names.is_empty() {
        layouts.append(&MenuItem::with_id(app, "no_layout", t(lang, "no_layout"), false, None::<&str>)?)?;
    }
    for (i, name) in names.iter().enumerate() {
        // Les quatre premières : rappel du raccourci (Alt+1…) dans le libellé.
        let label = if i < 4 { format!("{}. {name}", i + 1) } else { name.clone() };
        layouts.append(&MenuItem::with_id(app, format!("layout|{i}"), label, true, None::<&str>)?)?;
    }
    // « Tout cacher » devient « Tout montrer » quand tout est caché.
    let hidden = app.state::<widgets::Widgets>().view().hidden_all;
    let hide_all = MenuItem::with_id(app, "hide_all", t(lang, if hidden { "show_all" } else { "hide_all" }), true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", t(lang, "quit"), true, None::<&str>)?;
    Menu::with_items(app, &[&open, &layouts, &hide_all, &sep, &quit])
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Bworkshop Launcher")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "open" => state::show_main(app),
            "hide_all" => widgets::toggle_hide_all(app),
            id if id.starts_with("layout|") => {
                if let Ok(i) = id[7..].parse::<usize>()
                    && let Err(e) = crate::layouts::apply(app, i) {
                        log::warn!("Disposition non rappelée : {e}");
                    }
            }
            "quit" => {
                log::info!("Quitter demandé depuis la zone de notification");
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                widgets::show_last(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Reconstruit le menu (langue changée, dispositions modifiées).
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    match build_menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => log::warn!("Menu de notification non reconstruit : {e}"),
    }
}
