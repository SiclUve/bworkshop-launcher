// Pas de console en version livrée.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod catalog_bridge;
mod commands;
mod dofus;
mod hotkeys;
mod i18n;
mod installer_bridge;
mod layouts;
mod running;
mod state;
mod tray;
mod updater;
mod widgets;

use bworkshop_core::{logging, paths::AppPaths, settings::SettingsStore};
use tauri::{Manager, RunEvent, WindowEvent};

/// Argument passé par le démarrage automatique de Windows : on reste dans la zone de notification.
pub const HIDDEN_ARG: &str = "--hidden";

fn main() {
    let paths = match AppPaths::from_env() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Impossible de trouver les dossiers du launcher : {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = logging::init(&paths.logs_dir(), log::LevelFilter::Info) {
        eprintln!("Journal indisponible : {e}");
    }
    log::info!("Démarrage du launcher v{}", env!("CARGO_PKG_VERSION"));

    let store = SettingsStore::new(paths.settings_file());
    let settings = store.load();
    let start_hidden = std::env::args().any(|a| a == HIDDEN_ARG);

    let bridge = std::sync::Arc::new(catalog_bridge::CatalogBridge::new(paths.cache_dir()));
    let widgets = widgets::Widgets::new(paths.widgets_file());
    let layouts = layouts::Layouts::new(bworkshop_core::layouts::LayoutsStore::new(paths.layouts_file()));
    let state = state::AppState::new(paths, store, settings);
    // Affichage immédiat depuis le cache, avant toute requête.
    if let Ok(mut c) = state.catalog.lock() {
        *c = bridge.load_cached();
    }

    let result = tauri::Builder::default()
        // Doit être le premier plugin : un deuxième lancement réaffiche simplement la fenêtre.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            log::info!("Deuxième lancement détecté : réaffichage de la fenêtre principale");
            state::show_main(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .manage(bridge)
        .manage(std::sync::Arc::new(installer_bridge::InstallerBridge::default()))
        .manage(widgets)
        .manage(running::Running::default())
        .manage(hotkeys::Hotkeys::default())
        .manage(layouts)
        .manage(updater::UpdaterState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_installs,
            commands::install_tool,
            commands::cancel_install,
            commands::dismiss_install_error,
            commands::uninstall_tool,
            commands::launch_tool,
            commands::get_running,
            commands::close_tool,
            commands::get_hotkeys_status,
            commands::suspend_hotkeys,
            commands::force_close_tool,
            commands::tool_has_window,
            commands::get_settings,
            commands::save_settings,
            commands::app_info,
            commands::get_catalog,
            commands::get_icons,
            commands::refresh_catalog,
            commands::toggle_favorite,
            commands::open_external,
            widgets::commands::get_widgets,
            widgets::commands::open_widget,
            widgets::commands::close_widget,
            widgets::commands::set_widgets_locked,
            widgets::commands::set_widgets_hidden,
            widgets::commands::get_toolbar_prefs,
            widgets::commands::set_tool_in_bar,
            widgets::commands::set_tool_mode,
            widgets::commands::toggle_tool,
            widgets::commands::widget_bar_state,
            widgets::commands::widget_close,
            widgets::commands::widget_toggle_collapse,
            widgets::commands::widget_reload,
            widgets::commands::widget_set_opacity,
            widgets::commands::widget_settings,
            widgets::commands::widget_drag,
            widgets::commands::set_toolbar_open,
            widgets::commands::toolbar_fit,
            widgets::commands::toolbar_drag,
            widgets::commands::toolbar_close,
            widgets::commands::toolbar_open_main,
            widgets::commands::toolbar_main_visible,
            widgets::commands::toolbar_menu,
            widgets::commands::toolbar_prefs,
            widgets::commands::toolbar_set_order,
            widgets::commands::toolbar_bar_menu,
            layouts::get_layouts,
            layouts::layout_save,
            layouts::layout_overwrite,
            layouts::layout_rename,
            layouts::layout_move,
            layouts::layout_delete,
            layouts::layout_apply,
            layouts::layout_remove_item,
            layouts::take_layouts_request,
            updater::get_launcher_update,
            updater::install_launcher_update
        ])
        .setup(move |app| {
            tray::create(app.handle())?;
            app.on_menu_event(|app, event| widgets::toolbar_menu::on_event(app, &event));
            state::sync_autostart(app.handle());
            catalog_bridge::start(app.handle());
            widgets::Widgets::start(app.handle());
            hotkeys::start(app.handle());
            dofus::start(app.handle());
            widgets::Widgets::restore_toolbar(app.handle());
            layouts::restore_at_startup(app.handle());
            if let Some(plugin) = updater::plugin() {
                app.handle().plugin(plugin)?;
            }
            updater::start(app.handle());
            if !start_hidden {
                state::show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == widgets::toolbar::LABEL {
                window.state::<widgets::Widgets>().on_toolbar_event(window, event);
                return;
            }
            if let Some(id) = widgets::id_of_window(window.label()) {
                window.state::<widgets::Widgets>().on_window_event(window, id, event);
                return;
            }
            // Fermer la fenêtre principale la détruit (sa mémoire est rendue) ; le launcher reste
            // près de l'horloge : seul « Quitter » le ferme (voir `ExitRequested` plus bas).
            if window.label() == state::MAIN_LABEL {
                match event {
                    WindowEvent::Destroyed => state::emit_main_visible(window.app_handle()),
                    // Réduite ou restaurée : la tuile « + » / « − » suit.
                    WindowEvent::Resized(_) | WindowEvent::Focused(_) => state::emit_main_visible(window.app_handle()),
                    _ => {}
                }
            }
        })
        .build(tauri::generate_context!());

    match result {
        Ok(app) => app.run(|app, event| {
            // Dernière fenêtre fermée (`code` absent) : le launcher continue près de l'horloge.
            // « Quitter » appelle `exit(0)` et passe, lui.
            if let RunEvent::ExitRequested { code: None, api, .. } = &event {
                api.prevent_exit();
                return;
            }
            // Positions des widgets enregistrées avant de quitter.
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                app.state::<widgets::Widgets>().flush(app);
            }
        }),
        Err(e) => {
            log::error!("Le launcher s'est arrêté sur une erreur : {e}");
            std::process::exit(1);
        }
    }
}
