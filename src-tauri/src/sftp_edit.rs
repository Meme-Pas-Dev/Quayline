//! Modification des fichiers distants.
//!
//! - **Éditeur intégré** (texte) : le contenu passe directement du serveur à
//!   l'interface et inversement, rien n'est écrit sur le disque local.
//! - **Application par défaut du système** : copie dans un dossier privé de
//!   l'application, ouverte par le système, renvoyée au serveur à chaque
//!   enregistrement. Les copies sont supprimées à la fermeture, au verrouillage et au
//!   démarrage suivant.
//!
//! Dans les deux cas, l'envoi vérifie d'abord que le fichier distant n'a pas changé
//! depuis l'ouverture, et le remplace par un fichier temporaire renommé (les permissions
//! sont reprises) ; si ce n'est pas possible (fichier d'un autre propriétaire, dossier
//! en lecture seule, lien symbolique), le fichier est réécrit sur place.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use russh_sftp::client::SftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::i18n::tr;
use crate::sftp::{io_err, join_remote, local_child, sftp_err, split_remote, SftpManager};
use crate::state::AppState;
use crate::text_encoding::{self, Charset, Encoding};

/// Taille maximale d'un fichier ouvert dans l'éditeur intégré.
const MAX_TEXT: u64 = 5 * 1024 * 1024;
/// Taille maximale d'un fichier ouvert avec l'application du système.
const MAX_EXTERNAL: u64 = 512 * 1024 * 1024;
/// Intervalle de surveillance des copies locales.
const WATCH_TICK: Duration = Duration::from_secs(1);

// ───────────────────────────── Lecture et écriture ─────────────────────────────

/// Date de modification et taille d'un fichier distant : ce que l'envoi compare pour
/// détecter une modification faite ailleurs entre-temps.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteVersion {
    modified: Option<u64>,
    size: u64,
}

impl RemoteVersion {
    fn of(attrs: &FileAttributes) -> Self {
        RemoteVersion { modified: attrs.mtime.map(u64::from), size: attrs.len() }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteText {
    content: String,
    version: RemoteVersion,
    /// Encodage détecté, à renvoyer à l'enregistrement.
    encoding: Encoding,
}

fn too_large(max: u64) -> CmdError {
    CmdError::invalid(tr!("err.edit.tooLarge", mib = max / 1024 / 1024))
}

/// Contenu d'un fichier régulier, au plus `max` octets.
async fn read_remote(sftp: &SftpSession, path: &str, max: u64) -> CmdResult<(Vec<u8>, RemoteVersion)> {
    let attrs = sftp.metadata(path.to_owned()).await.map_err(sftp_err)?;
    if !attrs.file_type().is_file() {
        return Err(CmdError::invalid(tr!("err.edit.notAFile")));
    }
    if attrs.len() > max {
        return Err(too_large(max));
    }
    let file = sftp.open(path.to_owned()).await.map_err(sftp_err)?;
    let mut data = Vec::with_capacity(attrs.len() as usize);
    file.take(max + 1).read_to_end(&mut data).await.map_err(|e| CmdError::new("sftp", tr!("err.sftp.generic", detail = e)))?;
    if data.len() as u64 > max {
        return Err(too_large(max));
    }
    Ok((data, RemoteVersion::of(&attrs)))
}

/// Texte d'un fichier et son encodage ; refusé s'il est binaire.
fn as_text(data: &[u8]) -> CmdResult<(String, Encoding)> {
    text_encoding::decode(data).ok_or_else(|| CmdError::invalid(tr!("err.edit.binary")))
}

/// Remplace le contenu d'un fichier distant. `expected` : version lue à l'ouverture ;
/// si le fichier a changé depuis, erreur `conflict` (sauf `None` : remplacement forcé).
async fn write_remote(sftp: &SftpSession, path: &str, data: &[u8], expected: Option<RemoteVersion>) -> CmdResult<RemoteVersion> {
    let current = sftp.symlink_metadata(path.to_owned()).await.map_err(sftp_err)?;
    if expected.is_some_and(|v| v != RemoteVersion::of(&current)) {
        return Err(CmdError::new("conflict", tr!("err.edit.conflict")));
    }
    let replaced = if current.file_type().is_symlink() {
        false // Remplacer le lien le transformerait en fichier : écriture sur place.
    } else {
        replace_with_temp(sftp, path, data, &current).await?
    };
    if !replaced {
        let mut file = sftp
            .open_with_flags(path.to_owned(), OpenFlags::WRITE | OpenFlags::TRUNCATE)
            .await
            .map_err(sftp_err)?;
        write_all(&mut file, data).await?;
    }
    let after = sftp.metadata(path.to_owned()).await.map_err(sftp_err)?;
    Ok(RemoteVersion::of(&after))
}

async fn write_all(file: &mut russh_sftp::client::fs::File, data: &[u8]) -> CmdResult<()> {
    let fail = |e: std::io::Error| CmdError::new("sftp", tr!("err.sftp.generic", detail = e));
    file.write_all(data).await.map_err(fail)?;
    file.shutdown().await.map_err(fail)
}

/// Écrit un fichier temporaire à côté, lui donne les permissions de l'original, puis
/// prend sa place (l'original est mis de côté le temps du renommage et restauré en cas
/// d'échec). `Ok(false)` si la méthode ne convient pas : écriture sur place.
async fn replace_with_temp(sftp: &SftpSession, path: &str, data: &[u8], original: &FileAttributes) -> CmdResult<bool> {
    let (dir, name) = split_remote(path);
    let suffix = Uuid::new_v4().simple().to_string();
    let short: String = name.chars().take(120).collect();
    let temp = join_remote(&dir, &format!(".{short}.quayline-{}", &suffix[..8]));
    let backup = join_remote(&dir, &format!(".{short}.quayline-old-{}", &suffix[8..16]));

    let flags = OpenFlags::CREATE | OpenFlags::EXCLUDE | OpenFlags::WRITE;
    let Ok(mut file) = sftp.open_with_flags(temp.clone(), flags).await else {
        return Ok(false); // Dossier en lecture seule : écriture sur place.
    };
    let written = write_all(&mut file, data).await;
    let created = sftp.symlink_metadata(temp.clone()).await.ok();
    // Un fichier d'un autre propriétaire (droits de groupe) le perdrait : sur place.
    let other_owner = matches!((created.as_ref().and_then(|c| c.uid), original.uid), (Some(a), Some(b)) if a != b);
    if written.is_err() || other_owner {
        let _ = sftp.remove_file(temp).await;
        return written.map(|()| false);
    }
    if let Some(mode) = original.permissions {
        let mut attrs = FileAttributes::empty();
        attrs.permissions = Some(mode & 0o7777);
        let _ = sftp.set_metadata(temp.clone(), attrs).await;
    }
    if sftp.rename(path.to_owned(), backup.clone()).await.is_err() {
        let _ = sftp.remove_file(temp).await;
        return Ok(false);
    }
    if let Err(e) = sftp.rename(temp.clone(), path.to_owned()).await {
        let _ = sftp.rename(backup, path.to_owned()).await;
        let _ = sftp.remove_file(temp).await;
        return Err(sftp_err(e));
    }
    let _ = sftp.remove_file(backup).await;
    Ok(true)
}

/// Ouvre un fichier distant dans l'éditeur intégré.
#[tauri::command]
pub async fn sftp_read_text(
    state: State<'_, AppState>,
    mgr: State<'_, SftpManager>,
    sftp_id: Uuid,
    path: String,
) -> CmdResult<RemoteText> {
    state.require_unlocked()?;
    let (sftp, _) = mgr.get(sftp_id)?;
    let (data, version) = read_remote(&sftp, &path, MAX_TEXT).await?;
    let (content, encoding) = as_text(&data)?;
    Ok(RemoteText { content, version, encoding })
}

/// Enregistre le texte de l'éditeur intégré. `expected` absent : remplacement forcé
/// (après un conflit, sur décision de l'utilisateur).
#[tauri::command]
pub async fn sftp_write_text(
    state: State<'_, AppState>,
    mgr: State<'_, SftpManager>,
    sftp_id: Uuid,
    path: String,
    content: String,
    encoding: Encoding,
    expected: Option<RemoteVersionInput>,
) -> CmdResult<RemoteVersion> {
    state.require_unlocked()?;
    let data = text_encoding::encode(&content, encoding).map_err(|c| {
        let name = match encoding.charset {
            Charset::Utf8 => "UTF-8",
            Charset::Utf16Le | Charset::Utf16Be => "UTF-16",
            Charset::Windows1252 => "Windows-1252",
        };
        CmdError::invalid(tr!("err.edit.unencodable", char = c, encoding = name))
    })?;
    if data.len() as u64 > MAX_TEXT {
        return Err(too_large(MAX_TEXT));
    }
    let (sftp, _) = mgr.get(sftp_id)?;
    write_remote(&sftp, &path, &data, expected.map(Into::into)).await
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteVersionInput {
    modified: Option<u64>,
    size: u64,
}

impl From<RemoteVersionInput> for RemoteVersion {
    fn from(v: RemoteVersionInput) -> Self {
        RemoteVersion { modified: v.modified, size: v.size }
    }
}

// ───────────────────────── Application par défaut du système ─────────────────────────

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EditEvent {
    #[serde(rename_all = "camelCase")]
    Opened { id: Uuid, sftp_id: Uuid, name: String, remote: String },
    /// Copie locale renvoyée au serveur.
    #[serde(rename_all = "camelCase")]
    Synced { id: Uuid },
    /// Le fichier distant a changé entre-temps : synchronisation suspendue jusqu'à la
    /// décision de l'utilisateur.
    #[serde(rename_all = "camelCase")]
    Conflict { id: Uuid },
    #[serde(rename_all = "camelCase")]
    Failed { id: Uuid, message: String },
    #[serde(rename_all = "camelCase")]
    Closed { id: Uuid },
}

struct ExternalEdit {
    sftp_id: Uuid,
    remote: String,
    dir: PathBuf,
    file: PathBuf,
    stop: AtomicBool,
    /// Envoi forcé demandé après un conflit.
    force: AtomicBool,
}

#[derive(Default)]
pub struct EditManager {
    edits: Mutex<HashMap<Uuid, Arc<ExternalEdit>>>,
    events: Mutex<Option<Channel<EditEvent>>>,
}

impl EditManager {
    fn emit(&self, event: EditEvent) {
        if let Some(ch) = self.events.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let _ = ch.send(event);
        }
    }

    fn remove(&self, id: Uuid) -> Option<Arc<ExternalEdit>> {
        let edit = self.edits.lock().unwrap_or_else(|p| p.into_inner()).remove(&id)?;
        edit.stop.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_dir_all(&edit.dir);
        Some(edit)
    }

    /// Fermeture d'une session SFTP : ses fichiers ne peuvent plus être renvoyés.
    pub fn stop_session(&self, sftp_id: Uuid) {
        self.stop_where(|e| e.sftp_id == sftp_id);
    }

    /// Nouvelle ouverture d'un fichier déjà ouvert : l'ancienne copie n'est plus suivie.
    fn stop_file(&self, sftp_id: Uuid, remote: &str) {
        self.stop_where(|e| e.sftp_id == sftp_id && e.remote == remote);
    }

    fn stop_where(&self, matches: impl Fn(&ExternalEdit) -> bool) {
        let ids: Vec<Uuid> = self.edits.lock().unwrap_or_else(|p| p.into_inner()).iter().filter(|(_, e)| matches(e)).map(|(id, _)| *id).collect();
        for id in ids {
            if self.remove(id).is_some() {
                self.emit(EditEvent::Closed { id });
            }
        }
    }

    /// Verrouillage et fermeture : fin des synchronisations, copies supprimées.
    pub fn stop_all(&self) {
        self.stop_where(|_| true);
    }
}

/// Dossier des copies locales, privé (`0700`).
fn edits_root(app: &AppHandle) -> CmdResult<PathBuf> {
    let root = app.path().app_cache_dir().map_err(|e| CmdError::new("io", tr!("err.sftp.local", detail = e)))?.join("remote-edits");
    create_private_dir_all(&root).map_err(io_err)?;
    Ok(root)
}

fn create_private_dir_all(dir: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Au démarrage : copies laissées par une fermeture brutale.
pub fn purge_leftovers(app: &AppHandle) {
    if let Ok(root) = app.path().app_cache_dir().map(|d| d.join("remote-edits")) {
        let _ = std::fs::remove_dir_all(root);
    }
}

#[tauri::command]
pub fn edits_subscribe(edits: State<'_, EditManager>, events: Channel<EditEvent>) {
    *edits.events.lock().unwrap_or_else(|p| p.into_inner()) = Some(events);
}

/// Télécharge le fichier dans un dossier privé, l'ouvre avec l'application par défaut
/// et renvoie chaque enregistrement au serveur.
#[tauri::command]
pub async fn sftp_open_external(app: AppHandle, sftp_id: Uuid, path: String) -> CmdResult<Uuid> {
    app.state::<AppState>().require_unlocked()?;
    let (sftp, _) = app.state::<SftpManager>().get(sftp_id)?;
    app.state::<EditManager>().stop_file(sftp_id, &path);
    let (_, name) = split_remote(&path);
    let id = Uuid::new_v4();
    let dir = edits_root(&app)?.join(id.simple().to_string());
    create_private_dir_all(&dir).map_err(io_err)?;
    let Some(file) = local_child(&dir, &name) else {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(CmdError::invalid(tr!("err.sftp.refusedName", name = crate::text::neutralize(&name, 80))));
    };
    let fetched = async {
        let (data, version) = read_remote(&sftp, &path, MAX_EXTERNAL).await?;
        write_private_file(&file, &data).map_err(io_err)?;
        #[cfg(target_os = "macos")]
        tag_text_encoding(&file, &data);
        Ok::<_, CmdError>(version)
    }
    .await;
    let version = match fetched {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(e);
        }
    };
    if !open_with_default_app(&file).await {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(CmdError::new("io", tr!("err.edit.openFailed")));
    }

    let edit = Arc::new(ExternalEdit {
        sftp_id,
        remote: path.clone(),
        dir,
        file,
        stop: AtomicBool::new(false),
        force: AtomicBool::new(false),
    });
    let edits = app.state::<EditManager>();
    edits.edits.lock().unwrap_or_else(|p| p.into_inner()).insert(id, edit.clone());
    edits.emit(EditEvent::Opened { id, sftp_id, name: crate::text::neutralize(&name, 255), remote: path });
    tauri::async_runtime::spawn(watch(app.clone(), id, edit, version));
    Ok(id)
}

/// Ouvre `file` avec l'application par défaut. Dans le Flatpak, `xdg-open` ne donnerait
/// qu'un accès en lecture : on passe par le portail avec écriture autorisée.
async fn open_with_default_app(file: &Path) -> bool {
    #[cfg(target_os = "linux")]
    if crate::identity::is_flatpak() {
        return crate::platform::portal::open_file_writable(file).await.is_ok();
    }
    let file = file.to_owned();
    matches!(tauri::async_runtime::spawn_blocking(move || open::that_detached(file)).await, Ok(Ok(())))
}

/// Indique à macOS l'encodage d'un fichier texte (attribut `com.apple.TextEncoding`, lu
/// et conservé par TextEdit et les applications Cocoa) : sans lui, un fichier Windows-1252
/// est lu comme de l'UTF-8 ou du MacRoman, et ses accents sont perdus à l'enregistrement.
#[cfg(target_os = "macos")]
fn tag_text_encoding(file: &Path, data: &[u8]) {
    use crate::text_encoding::{decode, Charset};
    use std::os::unix::ffi::OsStrExt;
    let Some((_, encoding)) = decode(data) else { return };
    // Nom IANA et valeur CFStringEncoding.
    let value: &[u8] = match encoding.charset {
        Charset::Utf8 => b"utf-8;134217984",
        Charset::Utf16Le => b"utf-16le;335544576",
        Charset::Utf16Be => b"utf-16be;268435712",
        Charset::Windows1252 => b"windows-1252;1280",
    };
    let Ok(path) = std::ffi::CString::new(file.as_os_str().as_bytes()) else { return };
    // SAFETY : chaînes terminées par un octet nul, valeur et longueur cohérentes.
    unsafe {
        libc::setxattr(path.as_ptr(), c"com.apple.TextEncoding".as_ptr(), value.as_ptr().cast(), value.len(), 0, 0);
    }
}

fn write_private_file(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(path)?;
    f.write_all(data)?;
    f.sync_all()
}

/// Empreinte de la copie locale : date de modification et taille.
fn local_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Surveille la copie locale ; un changement stable (identique d'un tour à l'autre,
/// l'application a fini d'écrire) est renvoyé au serveur.
async fn watch(app: AppHandle, id: Uuid, edit: Arc<ExternalEdit>, mut version: RemoteVersion) {
    let mut synced = local_stamp(&edit.file);
    let mut seen = synced;
    let mut conflict = false;
    let mut tick = tokio::time::interval(WATCH_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        if edit.stop.load(Ordering::Relaxed) {
            return;
        }
        let now = local_stamp(&edit.file);
        let force = edit.force.swap(false, Ordering::Relaxed);
        let changed = now.is_some() && now != synced && now == seen;
        seen = now;
        if !(changed && !conflict) && !force {
            continue;
        }
        let edits = app.state::<EditManager>();
        let result = async {
            let (sftp, _) = app.state::<SftpManager>().get(edit.sftp_id)?;
            let data = std::fs::read(&edit.file).map_err(io_err)?;
            if data.len() as u64 > MAX_EXTERNAL {
                return Err(too_large(MAX_EXTERNAL));
            }
            write_remote(&sftp, &edit.remote, &data, (!force).then_some(version)).await
        }
        .await;
        match result {
            Ok(v) => {
                version = v;
                synced = now;
                conflict = false;
                edits.emit(EditEvent::Synced { id });
            }
            Err(e) if e.code == "conflict" => {
                conflict = true;
                edits.emit(EditEvent::Conflict { id });
            }
            Err(e) if e.code == "no_session" || e.code == "locked" => {
                edits.emit(EditEvent::Failed { id, message: e.message });
                if edits.remove(id).is_some() {
                    edits.emit(EditEvent::Closed { id });
                }
                return;
            }
            Err(e) => edits.emit(EditEvent::Failed { id, message: e.message }),
        }
    }
}

/// Après un conflit : remplacer le fichier distant par la copie locale.
#[tauri::command]
pub fn edit_external_overwrite(edits: State<'_, EditManager>, id: Uuid) {
    if let Some(edit) = edits.edits.lock().unwrap_or_else(|p| p.into_inner()).get(&id) {
        edit.force.store(true, Ordering::Relaxed);
    }
}

/// Fin de la modification : synchronisation arrêtée, copie locale supprimée.
#[tauri::command]
pub fn edit_external_stop(edits: State<'_, EditManager>, id: Uuid) {
    if edits.remove(id).is_some() {
        edits.emit(EditEvent::Closed { id });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binary_files_are_refused() {
        assert_eq!(as_text(b"ligne\n").unwrap().0, "ligne\n");
        assert!(as_text(b"a\0b").is_err());
    }
}
