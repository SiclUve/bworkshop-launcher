fn main() {
    // Les commandes du launcher sont déclarées ici : seules les fenêtres qui ont
    // une capacité les autorisant peuvent les appeler. Les pages distantes des
    // widgets n'en ont aucune.
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(
            tauri_build::AppManifest::new().commands(&[
                "get_settings",
                "save_settings",
                "app_info",
                "get_catalog",
                "get_icons",
                "refresh_catalog",
                "toggle_favorite",
                "open_external",
                "get_installs",
                "install_tool",
                "cancel_install",
                "dismiss_install_error",
                "uninstall_tool",
                "launch_tool",
                "get_running",
                "close_tool",
                "get_hotkeys_status",
                "suspend_hotkeys",
                "force_close_tool",
                "tool_has_window",
                // Widgets (fenêtre principale).
                "get_widgets",
                "open_widget",
                "close_widget",
                "set_widgets_locked",
                "set_widgets_hidden",
                // « Mes plugins » : tuiles et mode d'ouverture.
                "get_toolbar_prefs",
                "set_tool_in_bar",
                "set_tool_mode",
                "toggle_tool",
                // Barre d'un widget, sur son propre widget seulement.
                "widget_bar_state",
                "widget_close",
                "widget_toggle_collapse",
                "widget_reload",
                "widget_set_opacity",
                "widget_settings",
                "widget_drag",
                // Barre d'outils.
                "set_toolbar_open",
                "toolbar_fit",
                "toolbar_drag",
                "toolbar_open_main",
                "toolbar_main_visible",
                "toolbar_menu",
                "toolbar_prefs",
                "toolbar_set_order",
                "toolbar_bar_menu",
                // Dispositions (fenêtre principale).
                "get_layouts",
                "layout_save",
                "layout_overwrite",
                "layout_rename",
                "layout_move",
                "layout_delete",
                "layout_apply",
                "layout_remove_item",
                "take_layouts_request",
                // Mise à jour du launcher (fenêtre principale).
                "get_launcher_update",
                "install_launcher_update",
            ]),
        ),
    )
    .expect("échec de tauri-build");
}
