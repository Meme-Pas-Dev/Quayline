//! Réinitialisation complète : retour à l'état d'une installation neuve.
//!
//! Coffre ouvert et mot de passe maître exigés (jamais depuis l'écran de
//! verrouillage). Le coffre est supprimé en premier ; le reste (anciens coffres, copies
//! locales, clé du fournisseur d'identité) ensuite, au mieux. Les préférences de
//! l'interface (stockage local de la fenêtre) sont effacées par l'interface.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use tauri::{AppHandle, Manager};
use vault::Secret;

use crate::commands::confirm_master_password;
use crate::error::{CmdError, CmdResult};
use crate::i18n::tr;
use crate::state::AppState;

/// Supprime un fichier ; `false` s'il existe encore.
fn remove(path: &Path) -> bool {
    match std::fs::remove_file(path) {
        Ok(()) => true,
        Err(e) => e.kind() == ErrorKind::NotFound,
    }
}

/// Fichier du coffre, son verrou et les fichiers temporaires d'un enregistrement
/// interrompu. Renvoie ceux qui n'ont pas pu être supprimés (hors coffre).
fn remove_vault_files(vault: &Path, leftovers: &mut Vec<PathBuf>) {
    let (Some(dir), Some(name)) = (vault.parent(), vault.file_name()) else { return };
    let name = name.to_string_lossy();
    let lock = dir.join(format!("{name}.lock"));
    if !remove(&lock) {
        leftovers.push(lock);
    }
    let temp_prefix = format!(".{name}.tmp-");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&temp_prefix) && !remove(&entry.path()) {
                leftovers.push(entry.path());
            }
        }
    }
}

/// Efface toutes les données de Quayline. Renvoie les chemins restés en place (à
/// supprimer à la main), vide si tout a été effacé.
#[tauri::command]
pub async fn app_reset(app: AppHandle, password: Secret) -> CmdResult<Vec<String>> {
    confirm_master_password(&app, password).await?;
    let state = app.state::<AppState>();
    let oidc = state.with_vault(|v| Ok(v.oidc().cloned()))?;

    // Plus rien d'ouvert : modifications externes, sessions, puis le coffre lui-même
    // (clé de données effacée, verrou du fichier relâché).
    app.state::<crate::sftp_edit::EditManager>().stop_all();
    app.state::<crate::ssh::SshManager>().close_all();
    app.state::<crate::sftp::SftpManager>().close_all();
    app.state::<crate::local_term::LocalTermManager>().close_all();
    state.lock();
    crate::clipboard::clear_now(&app);

    let vault = state.vault_path.clone();
    if let Err(e) = std::fs::remove_file(&vault) {
        if e.kind() != ErrorKind::NotFound {
            return Err(CmdError::new("io", tr!("err.reset.vault", detail = e)));
        }
    }

    let mut leftovers = Vec::new();
    remove_vault_files(&vault, &mut leftovers);
    // Coffres laissés sous un ancien identifiant de l'application : sans cela, ils
    // seraient repris au prochain démarrage (migrate_data_dir).
    if let Some(parent) = vault.parent().and_then(Path::parent) {
        for old_id in crate::identity::migrate_from() {
            let old = parent.join(old_id).join(crate::VAULT_FILE);
            if !remove(&old) {
                leftovers.push(old.clone());
            }
            remove_vault_files(&old, &mut leftovers);
        }
    }
    crate::sftp_edit::purge_leftovers(&app);
    if let Some(binding) = oidc {
        crate::oidc::keyring_delete(&binding).await;
    }
    Ok(leftovers.into_iter().map(|p| p.display().to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_lock_and_temp_files_only() {
        let dir = std::env::temp_dir().join(format!("quayline-reset-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir_all(&dir).unwrap();
        let vault = dir.join("vault.qlv");
        for name in ["vault.qlv.lock", ".vault.qlv.tmp-1234", "autre.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let mut leftovers = Vec::new();
        remove_vault_files(&vault, &mut leftovers);
        let mut left: Vec<String> =
            std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        left.sort();
        assert!(leftovers.is_empty());
        assert_eq!(left, ["autre.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
