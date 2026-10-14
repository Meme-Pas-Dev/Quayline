//! Ouverture d'une adresse dans le navigateur, au premier plan sous Wayland.
//!
//! Wayland interdit à une fenêtre de prendre le focus sans jeton d'activation
//! (`xdg_activation_v1`). `xdg-open` n'en transmet pas : le navigateur s'ouvre derrière
//! Quayline. Lancé avec le contexte de lancement de GDK (fil principal, juste après le
//! clic), GIO demande ce jeton pour la fenêtre et le passe au navigateur (variable
//! `XDG_ACTIVATION_TOKEN`, ou portail `OpenURI` dans le Flatpak).

use tauri::{AppHandle, Runtime};
use tokio::sync::oneshot;

/// `true` si GIO a lancé l'application par défaut pour `url`.
pub async fn open_uri<R: Runtime>(app: &AppHandle<R>, url: String) -> bool {
    let (tx, rx) = oneshot::channel();
    let queued = app
        .run_on_main_thread(move || {
            let context = gdk::Display::default().and_then(|d| d.app_launch_context());
            let _ = tx.send(gio::AppInfo::launch_default_for_uri(&url, context.as_ref()).is_ok());
        })
        .is_ok();
    queued && rx.await.unwrap_or(false)
}
