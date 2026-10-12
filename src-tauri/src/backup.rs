//! Export et import chiffrés (hôtes, trousseau, clés d'hôte, réglages).
//!
//! Les fichiers sont choisis par une boîte de dialogue ouverte ici, côté backend :
//! l'interface ne fournit jamais de chemin, elle ne peut donc ni écrire ni lire un
//! fichier de son choix. Les moyens de déverrouillage (Pocket ID, clés FIDO2) ne sont
//! jamais exportés.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use vault::export::{self, Duplicates, ExportData, ExportSelection, ImportPreview, ImportReport, MAX_EXPORT_LEN};
use vault::{KdfParams, Secret};

use crate::i18n::tr;
use crate::commands::{confirm_master_password, validate_import, MAX_LABEL};
use crate::error::{CmdError, CmdResult};
use crate::state::{AppState, PendingImport};

const EXTENSION: &str = "qlexport";

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| CmdError::new("internal", tr!("err.interrupted")))?
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Date AAAA-MM-JJ (UTC) d'un horodatage Unix, pour le nom de fichier proposé.
fn date(secs: u64) -> String {
    // Algorithme « days to civil » (H. Hinnant).
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Exporte la sélection dans un fichier choisi par l'utilisateur, chiffré par
/// `passphrase`. Le mot de passe maître est exigé : le fichier contient les secrets
/// du trousseau. Renvoie le nom du fichier, ou rien si l'utilisateur a annulé.
#[tauri::command]
pub async fn vault_export(
    app: AppHandle,
    password: Secret,
    passphrase: Secret,
    selection: ExportSelection,
) -> CmdResult<Option<String>> {
    vault::check_password_policy(passphrase.expose().as_bytes()).map_err(|_| {
        CmdError::invalid(tr!("err.export.passphrase", count = vault::MIN_PASSWORD_LEN))
    })?;
    confirm_master_password(&app, password).await?;

    let created_at = now();
    let version = app.package_info().version.to_string();
    let data = app
        .state::<AppState>()
        .with_vault(|v| Ok(ExportData::from_vault(&v.data, selection, created_at, &version)))?;
    if data.is_empty() {
        return Err(CmdError::invalid(tr!("err.export.nothing")));
    }

    let dialog_app = app.clone();
    let Some(path) = blocking(move || {
        let mut dialog = dialog_app
            .dialog()
            .file()
            .set_title(tr!("backup.dialogExport"))
            .set_file_name(format!("quayline-{}.{EXTENSION}", date(created_at)))
            .add_filter(tr!("backup.filter"), &[EXTENSION]);
        if let Some(window) = dialog_app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        Ok(dialog.blocking_save_file().and_then(|p| p.into_path().ok()))
    })
    .await?
    else {
        return Ok(None);
    };
    let path: PathBuf =
        if path.extension().is_some() { path } else { path.with_extension(EXTENSION) };

    blocking(move || {
        let bytes = export::seal(&data, passphrase.expose().as_bytes(), KdfParams::DEFAULT)?;
        vault::vault::write_atomic(&path, &bytes)
            .map_err(|e| CmdError::new("io", tr!("err.write", detail = e)))?;
        Ok(Some(file_name(&path)))
    })
    .await
}

/// Première étape de l'import : choix du fichier. Renvoie son nom, ou rien si
/// l'utilisateur a annulé.
#[tauri::command]
pub async fn vault_import_pick(app: AppHandle) -> CmdResult<Option<String>> {
    if !app.state::<AppState>().is_unlocked() {
        return Err(CmdError::locked());
    }
    let dialog_app = app.clone();
    let Some(path) = blocking(move || {
        let mut dialog = dialog_app
            .dialog()
            .file()
            .set_title(tr!("backup.dialogImport"))
            .add_filter(tr!("backup.filter"), &[EXTENSION]);
        if let Some(window) = dialog_app.get_webview_window("main") {
            dialog = dialog.set_parent(&window);
        }
        Ok(dialog.blocking_pick_file().and_then(|p| p.into_path().ok()))
    })
    .await?
    else {
        return Ok(None);
    };

    let raw = blocking(move || {
        let len = std::fs::metadata(&path).map_err(|e| CmdError::new("io", tr!("err.read", detail = e)))?.len();
        if len > MAX_EXPORT_LEN as u64 {
            return Err(CmdError::invalid(tr!("err.import.tooLarge")));
        }
        let raw = std::fs::read(&path).map_err(|e| CmdError::new("io", tr!("err.read", detail = e)))?;
        Ok((raw, file_name(&path)))
    })
    .await?;
    app.state::<AppState>().set_pending_import(Some(PendingImport::File(raw.0)))?;
    Ok(Some(raw.1))
}

/// Deuxième étape : déchiffrement et aperçu, sans rien modifier. En cas d'échec, le
/// fichier reste choisi pour un nouvel essai.
#[tauri::command]
pub async fn vault_import_preview(app: AppHandle, passphrase: Secret) -> CmdResult<ImportPreview> {
    let state = app.state::<AppState>();
    let raw = match state.take_pending_import()? {
        PendingImport::File(raw) => raw,
        PendingImport::Opened(_) => return Err(CmdError::new("no_import", tr!("err.import.chooseFile"))),
    };
    let opened = {
        let raw = raw.clone();
        blocking(move || export::open(&raw, passphrase.expose().as_bytes()).map_err(Into::into)).await
    };
    let mut data = match opened {
        Ok(data) => data,
        Err(e) => {
            state.set_pending_import(Some(PendingImport::File(raw)))?;
            return Err(match e.code {
                "bad_password" => {
                    CmdError::new("bad_password", tr!("err.import.badPassphrase"))
                }
                _ => e,
            });
        }
    };
    validate_import(&mut data)?;
    let preview = state.with_vault(|v| Ok(export::preview(&v.data, &data)))?;
    state.set_pending_import(Some(PendingImport::Opened(data)))?;
    Ok(preview)
}

/// Dernière étape : fusion dans le coffre, puis enregistrement.
#[tauri::command]
pub fn vault_import_apply(
    app: AppHandle,
    duplicates: Duplicates,
    import_settings: bool,
) -> CmdResult<ImportReport> {
    let state = app.state::<AppState>();
    let data = match state.take_pending_import()? {
        PendingImport::Opened(data) => data,
        PendingImport::File(_) => return Err(CmdError::new("no_import", tr!("err.import.notDecrypted"))),
    };
    let suffix = tr!("backup.importedSuffix");
    state.mutate_vault(|v| Ok(export::merge(&mut v.data, data, duplicates, import_settings, MAX_LABEL, &suffix)))
}

#[tauri::command]
pub fn vault_import_cancel(app: AppHandle) {
    let _ = app.state::<AppState>().set_pending_import(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_791_590_400), "2026-10-10");
        assert_eq!(date(951_782_400), "2000-02-29");
    }
}
