//! Menu de l'application (macOS), dans la langue de l'interface : celui que Tauri
//! fournit par défaut a des libellés anglais écrits en dur. Même structure, mêmes
//! raccourcis. Reconstruit à chaque changement de langue (i18n::set_language).

use tauri::menu::{AboutMetadata, Menu, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, Runtime};

use crate::i18n::tr;
use crate::identity::NAME;

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    let about = AboutMetadata {
        name: Some(NAME.into()),
        version: Some(app.package_info().version.to_string()),
        ..Default::default()
    };
    let separator = || PredefinedMenuItem::separator(app);

    let app_menu = Submenu::with_items(
        app,
        NAME,
        true,
        &[
            &PredefinedMenuItem::about(app, Some(&tr!("macmenu.about", app = NAME)), Some(about))?,
            &separator()?,
            &PredefinedMenuItem::services(app, Some(&tr!("macmenu.services")))?,
            &separator()?,
            &PredefinedMenuItem::hide(app, Some(&tr!("macmenu.hide", app = NAME)))?,
            &PredefinedMenuItem::hide_others(app, Some(&tr!("macmenu.hideOthers")))?,
            &PredefinedMenuItem::show_all(app, Some(&tr!("macmenu.showAll")))?,
            &separator()?,
            &PredefinedMenuItem::quit(app, Some(&tr!("macmenu.quit", app = NAME)))?,
        ],
    )?;
    let file_menu = Submenu::with_items(
        app,
        tr!("macmenu.file"),
        true,
        &[&PredefinedMenuItem::close_window(app, Some(&tr!("macmenu.closeWindow")))?],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        tr!("macmenu.edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(&tr!("macmenu.undo")))?,
            &PredefinedMenuItem::redo(app, Some(&tr!("macmenu.redo")))?,
            &separator()?,
            &PredefinedMenuItem::cut(app, Some(&tr!("macmenu.cut")))?,
            &PredefinedMenuItem::copy(app, Some(&tr!("macmenu.copy")))?,
            &PredefinedMenuItem::paste(app, Some(&tr!("macmenu.paste")))?,
            &PredefinedMenuItem::select_all(app, Some(&tr!("macmenu.selectAll")))?,
        ],
    )?;
    let view_menu = Submenu::with_items(
        app,
        tr!("macmenu.view"),
        true,
        &[&PredefinedMenuItem::fullscreen(app, Some(&tr!("macmenu.fullscreen")))?],
    )?;
    // Identifiants de Tauri : macOS y ajoute la liste des fenêtres et la recherche d'aide.
    let window_menu = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        tr!("macmenu.window"),
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some(&tr!("macmenu.minimize")))?,
            &PredefinedMenuItem::maximize(app, Some(&tr!("macmenu.zoom")))?,
            &separator()?,
            &PredefinedMenuItem::close_window(app, Some(&tr!("macmenu.closeWindow")))?,
        ],
    )?;
    let help_menu = Submenu::with_id_and_items(app, HELP_SUBMENU_ID, tr!("macmenu.help"), true, &[])?;

    Menu::with_items(app, &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu, &help_menu])
}
