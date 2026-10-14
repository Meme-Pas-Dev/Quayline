mod backup;
mod clipboard;
mod commands;
mod error;
mod fido2;
mod http;
mod i18n;
mod identity;
mod oidc;
mod platform;
mod reset;
mod sftp;
mod sftp_edit;
mod ssh;
mod state;
mod text;
mod text_encoding;
mod updates;

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use state::AppState;

pub(crate) const VAULT_FILE: &str = "vault.qlv";

/// Le dossier de données (coffre + cache WebKit) n'est lisible que par l'utilisateur.
fn restrict_dir(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}


/// Après un renommage de l'application, reprend le coffre resté dans le dossier de
/// données de l'ancien identifiant (`migrateFrom` dans identity.json).
fn migrate_data_dir(dir: &std::path::Path) {
    let target = dir.join(VAULT_FILE);
    if target.exists() {
        return;
    }
    let Some(parent) = dir.parent() else { return };
    for old_id in identity::migrate_from() {
        let old = parent.join(old_id).join(VAULT_FILE);
        if old.is_file() && std::fs::rename(&old, &target).is_ok() {
            eprintln!("{}: vault taken over from {}", identity::SLUG, old.display());
            return;
        }
    }
}

/// Verrouille le coffre (s'il est ouvert) et prévient l'interface.
fn lock_now(handle: &AppHandle, reason: &str) {
    let state = handle.state::<AppState>();
    if !state.is_unlocked() {
        return;
    }
    let close_sessions = state.lock_closes_sessions();
    state.lock();
    clipboard::clear_now(handle);
    handle.state::<sftp_edit::EditManager>().stop_all();
    if close_sessions {
        handle.state::<ssh::SshManager>().close_all();
        handle.state::<sftp::SftpManager>().close_all();
    }
    let _ = handle.emit("vault://locked", reason);
}

/// Ramène la fenêtre au premier plan (authentification faite dans le navigateur ou sur
/// un téléphone, second lancement de l'application). Au mieux : certains bureaux
/// refusent le vol de focus.
pub(crate) fn bring_to_front(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        if window.set_focus().is_err() {
            let _ = window.request_user_attention(Some(tauri::UserAttentionType::Informational));
        }
    }
}

pub fn run() {
    platform::hardening::harden_process();
    // TLS (Pocket ID) : fournisseur crypto « ring », vérification par le magasin de
    // certificats du système (autorités ajoutées à Fedora comprises).
    let _ = rustls::crypto::ring::default_provider().install_default();
    #[cfg(target_os = "linux")]
    // Nom annoncé au compositeur (app_id Wayland, WM_CLASS X11) : il doit correspondre
    // au fichier .desktop installé, sinon KDE/GNOME ne retrouvent ni l'icône ni
    // l'entrée du menu. Le RPM installe « <nom>.desktop », le Flatpak « <id>.desktop ».
    glib::set_prgname(Some(if identity::is_flatpak() { identity::ID } else { identity::NAME }));

    let builder = tauri::Builder::default();
    // macOS : menu de l'application traduit (celui de Tauri est en anglais).
    #[cfg(target_os = "macos")]
    let builder = builder.menu(platform::menu::build);
    builder
        // En premier : un second lancement ramène la fenêtre existante et s'arrête avant
        // d'ouvrir quoi que ce soit (le coffre est en plus protégé par un verrou).
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| bring_to_front(app)))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(ssh::SshManager::default())
        .manage(sftp::SftpManager::default())
        .manage(sftp_edit::EditManager::default())
        .manage(oidc::OidcState::default())
        .manage(clipboard::SecretClipboard::default())
        .manage(updates::UpdateState::default())
        .setup(|app| {
            // ~/.local/share/<identifiant>/vault.qlv
            let dir = app.path().app_data_dir()?;
            restrict_dir(&dir)?;
            migrate_data_dir(&dir);
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_title(identity::NAME);
                // Créée invisible, la fenêtre est affichée par l'interface une fois
                // peinte. Filet de sécurité si cela n'arrive pas.
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    if !window.is_visible().unwrap_or(true) {
                        let _ = window.show();
                    }
                });
            }
            app.manage(AppState::new(dir.join(VAULT_FILE)));
            sftp_edit::purge_leftovers(app.handle());

            // Verrouillage automatique après inactivité.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let mut tick = tokio::time::interval(Duration::from_secs(5));
                loop {
                    tick.tick().await;
                    if let Some(close_sessions) = handle.state::<AppState>().auto_lock_if_idle() {
                        clipboard::clear_now(&handle);
                        handle.state::<sftp_edit::EditManager>().stop_all();
                        if close_sessions {
                            handle.state::<ssh::SshManager>().close_all();
                            handle.state::<sftp::SftpManager>().close_all();
                        }
                        let _ = handle.emit("vault://locked", "idle");
                    }
                }
            });

            // Verrouillage avec la session du bureau et à la mise en veille.
            let handle = app.handle().clone();
            platform::session::watch(move |event| {
                if !handle.state::<AppState>().lock_on_system_events() {
                    return;
                }
                let reason = match event {
                    platform::SystemEvent::SessionLocked => "session",
                    platform::SystemEvent::Suspending => "sleep",
                };
                lock_now(&handle, reason);
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            i18n::set_language,
            updates::update_check,
            updates::update_open,
            updates::update_install,
            updates::update_restart,
            commands::vault_status,
            commands::vault_create,
            commands::vault_unlock,
            commands::vault_lock,
            reset::app_reset,
            commands::vault_touch,
            commands::vault_change_password,
            backup::vault_export,
            backup::vault_import_pick,
            backup::vault_import_preview,
            backup::vault_import_apply,
            backup::vault_import_cancel,
            commands::hosts_list,
            commands::host_save,
            commands::host_delete,
            commands::identities_list,
            commands::identity_save,
            commands::identity_delete,
            commands::identity_copy_password,
            commands::identity_generate_key,
            commands::settings_get,
            commands::settings_save,
            commands::ssh_open,
            commands::ssh_write,
            commands::ssh_paste,
            commands::ssh_resize,
            commands::ssh_close,
            commands::ssh_prompt_answer,
            commands::known_hosts_list,
            commands::known_host_delete,
            sftp::sftp_open,
            sftp::sftp_close,
            sftp::sftp_list,
            sftp::sftp_realpath,
            sftp::sftp_mkdir,
            sftp::sftp_rename,
            sftp::sftp_delete,
            sftp::sftp_stat,
            sftp::sftp_chmod,
            sftp::local_stat,
            sftp::local_chmod,
            sftp_edit::sftp_read_text,
            sftp_edit::sftp_write_text,
            sftp_edit::edits_subscribe,
            sftp_edit::sftp_open_external,
            sftp_edit::edit_external_overwrite,
            sftp_edit::edit_external_stop,
            sftp::local_home,
            sftp::local_list,
            sftp::local_realpath,
            sftp::local_mkdir,
            sftp::local_rename,
            sftp::local_trash,
            sftp::transfers_subscribe,
            sftp::transfer_cancel,
            sftp::sftp_upload,
            sftp::sftp_download,
            oidc::oidc_status,
            oidc::oidc_redirect_uri,
            oidc::oidc_enroll,
            oidc::oidc_disable,
            oidc::oidc_unlock,
            oidc::oidc_cancel,
            oidc::oidc_device_start,
            oidc::oidc_device_wait,
            fido2::fido2_list,
            fido2::fido2_detect,
            fido2::fido2_set_pin,
            fido2::fido2_enroll,
            fido2::fido2_remove,
            fido2::fido2_unlock,
        ])
        .build(tauri::generate_context!())
        .expect("erreur au lancement de l'application")
        .run(|app, event| {
            // Ne laisse pas de mot de passe dans le presse-papiers en quittant.
            // Ni copie locale d'un fichier distant en cours de modification.
            if let tauri::RunEvent::Exit = event {
                clipboard::clear_now(app);
                app.state::<sftp_edit::EditManager>().stop_all();
            }
        });
}
