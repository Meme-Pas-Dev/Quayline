//! Menu de l'application (macOS), en français : celui que Tauri fournit par défaut a
//! des libellés anglais écrits en dur. Même structure, mêmes raccourcis.

use tauri::menu::{AboutMetadata, Menu, PredefinedMenuItem, Submenu, HELP_SUBMENU_ID, WINDOW_SUBMENU_ID};
use tauri::{AppHandle, Runtime};

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
            &PredefinedMenuItem::about(app, Some(&format!("À propos de {NAME}")), Some(about))?,
            &separator()?,
            &PredefinedMenuItem::services(app, Some("Services"))?,
            &separator()?,
            &PredefinedMenuItem::hide(app, Some(&format!("Masquer {NAME}")))?,
            &PredefinedMenuItem::hide_others(app, Some("Masquer les autres"))?,
            &PredefinedMenuItem::show_all(app, Some("Tout afficher"))?,
            &separator()?,
            &PredefinedMenuItem::quit(app, Some(&format!("Quitter {NAME}")))?,
        ],
    )?;
    let file_menu = Submenu::with_items(
        app,
        "Fichier",
        true,
        &[&PredefinedMenuItem::close_window(app, Some("Fermer la fenêtre"))?],
    )?;
    let edit_menu = Submenu::with_items(
        app,
        "Édition",
        true,
        &[
            &PredefinedMenuItem::undo(app, Some("Annuler"))?,
            &PredefinedMenuItem::redo(app, Some("Rétablir"))?,
            &separator()?,
            &PredefinedMenuItem::cut(app, Some("Couper"))?,
            &PredefinedMenuItem::copy(app, Some("Copier"))?,
            &PredefinedMenuItem::paste(app, Some("Coller"))?,
            &PredefinedMenuItem::select_all(app, Some("Tout sélectionner"))?,
        ],
    )?;
    let view_menu = Submenu::with_items(
        app,
        "Présentation",
        true,
        &[&PredefinedMenuItem::fullscreen(app, Some("Passer en mode plein écran"))?],
    )?;
    // Identifiants de Tauri : macOS y ajoute la liste des fenêtres et la recherche d'aide.
    let window_menu = Submenu::with_id_and_items(
        app,
        WINDOW_SUBMENU_ID,
        "Fenêtre",
        true,
        &[
            &PredefinedMenuItem::minimize(app, Some("Placer dans le Dock"))?,
            &PredefinedMenuItem::maximize(app, Some("Réduire/agrandir"))?,
            &separator()?,
            &PredefinedMenuItem::close_window(app, Some("Fermer la fenêtre"))?,
        ],
    )?;
    let help_menu = Submenu::with_id_and_items(app, HELP_SUBMENU_ID, "Aide", true, &[])?;

    Menu::with_items(app, &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu, &help_menu])
}
