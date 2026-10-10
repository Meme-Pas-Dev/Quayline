//! Explorateur SFTP : navigation distante et locale, transferts.
//!
//! Chaque explorateur ouvre sa propre connexion SSH (même vérification de clé d'hôte et
//! même authentification que le terminal), puis le sous-système `sftp`.
//!
//! Point de sécurité : les noms de fichiers renvoyés par le serveur ne sont pas de
//! confiance. Un serveur malveillant pourrait renvoyer `../../.bashrc` pour écrire hors
//! du dossier de destination lors d'un téléchargement récursif : ces noms sont rejetés
//! (voir [`safe_name`]).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, UNIX_EPOCH};

use russh::client;
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::FileAttributes;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::ssh::{self, Asker, HostKeyVerifier, SessionEvent, Target};

const CHUNK: usize = 256 * 1024;
const PROGRESS_INTERVAL: Duration = Duration::from_millis(150);
const PARALLEL_TRANSFERS: usize = 3;

// ───────────────────────────── État ─────────────────────────────

struct Conn {
    sftp: Arc<SftpSession>,
    // Garde la connexion SSH ouverte tant que l'explorateur existe.
    _ssh: Arc<client::Handle<HostKeyVerifier>>,
    limiter: Arc<Semaphore>,
}

#[derive(Default)]
pub struct SftpManager {
    conns: Mutex<HashMap<Uuid, Conn>>,
    /// Incrémenté à chaque verrouillage : une connexion qui aboutit après un
    /// verrouillage est abandonnée au lieu de rester ouverte.
    lock_generation: AtomicU64,
    transfers: Mutex<HashMap<Uuid, Arc<AtomicBool>>>,
    events: Mutex<Option<Channel<TransferEvent>>>,
}

impl SftpManager {
    fn get(&self, id: Uuid) -> CmdResult<(Arc<SftpSession>, Arc<Semaphore>)> {
        let conns = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        let c = conns.get(&id).ok_or_else(|| CmdError::new("no_session", "Connexion SFTP fermée."))?;
        Ok((c.sftp.clone(), c.limiter.clone()))
    }

    pub fn close(&self, id: Uuid) {
        self.conns.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    }

    /// Verrouillage : ferme les connexions et annule les transferts.
    pub fn close_all(&self) {
        let mut conns = self.conns.lock().unwrap_or_else(|p| p.into_inner());
        self.lock_generation.fetch_add(1, Ordering::AcqRel);
        conns.clear();
        drop(conns);
        for flag in self.transfers.lock().unwrap_or_else(|p| p.into_inner()).values() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    fn emit(&self, event: TransferEvent) {
        if let Some(ch) = self.events.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
            let _ = ch.send(event);
        }
    }
}

// ───────────────────────────── Modèle ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    name: String,
    path: String,
    kind: &'static str, // "dir" | "file" | "link"
    /// Pour un lien : vrai s'il pointe vers un dossier.
    is_dir: bool,
    size: u64,
    modified: Option<u64>,
    permissions: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TransferEvent {
    #[serde(rename_all = "camelCase")]
    Started { id: Uuid, sftp_id: Uuid, direction: &'static str, name: String, destination: String },
    #[serde(rename_all = "camelCase")]
    Progress { id: Uuid, done: u64, total: u64, files: u32 },
    #[serde(rename_all = "camelCase")]
    Finished { id: Uuid, error: Option<String>, cancelled: bool, skipped: u32 },
}

/// Nom de fichier acceptable comme composant de chemin (jamais de traversée).
pub fn safe_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

fn validate_new_name(name: &str) -> CmdResult<()> {
    if safe_name(name) && name.chars().count() <= 255 {
        Ok(())
    } else {
        Err(CmdError::invalid("Nom invalide (pas de « / », ni « . » ou « .. »)."))
    }
}

pub fn join_remote(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

fn remote_basename(path: &str) -> &str {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path)
}

fn mode_string(mode: u32) -> String {
    let flags = [(0o400, 'r'), (0o200, 'w'), (0o100, 'x'), (0o040, 'r'), (0o020, 'w'), (0o010, 'x'), (0o004, 'r'), (0o002, 'w'), (0o001, 'x')];
    flags.iter().map(|&(bit, c)| if mode & bit != 0 { c } else { '-' }).collect()
}

fn sftp_err(e: russh_sftp::client::error::Error) -> CmdError {
    CmdError::new("sftp", format!("Erreur SFTP : {e}"))
}

fn io_err(e: std::io::Error) -> CmdError {
    CmdError::new("io", format!("Erreur locale : {e}"))
}

fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

// ───────────────────────────── Connexion ─────────────────────────────

#[tauri::command]
pub fn sftp_open(app: AppHandle, host_id: Uuid, events: Channel<SessionEvent>) -> CmdResult<Uuid> {
    let target = Target::load(&app, host_id)?;
    let id = Uuid::new_v4();
    let generation = app.state::<SftpManager>().lock_generation.load(Ordering::Acquire);
    tauri::async_runtime::spawn(async move {
        let asker = Asker::new(&app, &events);
        match connect(&asker, target).await {
            Ok((sftp, ssh_handle, home)) => {
                let manager = app.state::<SftpManager>();
                let mut conns = manager.conns.lock().unwrap_or_else(|p| p.into_inner());
                if manager.lock_generation.load(Ordering::Acquire) != generation {
                    drop(conns);
                    let _ = events.send(SessionEvent::Closed {
                        error: Some("Connexion abandonnée : le coffre a été verrouillé.".into()),
                    });
                    return;
                }
                conns.insert(
                    id,
                    Conn { sftp: Arc::new(sftp), _ssh: Arc::new(ssh_handle), limiter: Arc::new(Semaphore::new(PARALLEL_TRANSFERS)) },
                );
                drop(conns);
                let _ = events.send(SessionEvent::SftpReady { home });
            }
            Err(error) => {
                let _ = events.send(SessionEvent::Closed { error: Some(error) });
            }
        }
    });
    Ok(id)
}

async fn connect(
    asker: &Asker,
    target: Target,
) -> Result<(SftpSession, client::Handle<HostKeyVerifier>, String), String> {
    let session = ssh::establish(asker, target).await?;
    let channel = session.channel_open_session().await.map_err(|e| format!("Ouverture du canal impossible : {e}"))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| format!("Le serveur refuse SFTP : {e}"))?;
    let sftp = SftpSession::new(channel.into_stream()).await.map_err(|e| format!("Initialisation SFTP impossible : {e}"))?;
    let home = sftp.canonicalize(".").await.unwrap_or_else(|_| "/".into());
    Ok((sftp, session, home))
}

#[tauri::command]
pub fn sftp_close(mgr: State<'_, SftpManager>, sftp_id: Uuid) {
    mgr.close(sftp_id);
}

// ───────────────────────────── Navigation distante ─────────────────────────────

fn remote_entry(dir: &str, name: String, attrs: &FileAttributes, target_is_dir: bool) -> FileEntry {
    let ft = attrs.file_type();
    let kind = if ft.is_dir() { "dir" } else if ft.is_symlink() { "link" } else { "file" };
    FileEntry {
        path: join_remote(dir, &name),
        name,
        kind,
        is_dir: ft.is_dir() || (ft.is_symlink() && target_is_dir),
        size: attrs.len(),
        modified: attrs.mtime.map(u64::from),
        permissions: attrs.permissions.map(mode_string),
    }
}

#[tauri::command]
pub async fn sftp_list(mgr: State<'_, SftpManager>, sftp_id: Uuid, path: String) -> CmdResult<Vec<FileEntry>> {
    let (sftp, _) = mgr.get(sftp_id)?;
    let dir = sftp.canonicalize(path).await.map_err(sftp_err)?;
    let mut entries = Vec::new();
    for entry in sftp.read_dir(dir.clone()).await.map_err(sftp_err)? {
        let name = entry.file_name();
        // Noms dangereux ou absurdes renvoyés par le serveur : ignorés.
        if !safe_name(&name) {
            continue;
        }
        let attrs = entry.metadata();
        let link_to_dir = if attrs.file_type().is_symlink() {
            sftp.metadata(join_remote(&dir, &name)).await.map(|m| m.file_type().is_dir()).unwrap_or(false)
        } else {
            false
        };
        entries.push(remote_entry(&dir, name, &attrs, link_to_dir));
    }
    sort_entries(&mut entries);
    Ok(entries)
}

/// Résout un chemin distant (`~`, `..`, liens) en chemin absolu.
#[tauri::command]
pub async fn sftp_realpath(mgr: State<'_, SftpManager>, sftp_id: Uuid, path: String) -> CmdResult<String> {
    let (sftp, _) = mgr.get(sftp_id)?;
    sftp.canonicalize(path).await.map_err(sftp_err)
}

#[tauri::command]
pub async fn sftp_mkdir(mgr: State<'_, SftpManager>, sftp_id: Uuid, dir: String, name: String) -> CmdResult<()> {
    validate_new_name(&name)?;
    let (sftp, _) = mgr.get(sftp_id)?;
    sftp.create_dir(join_remote(&dir, &name)).await.map_err(sftp_err)
}

#[tauri::command]
pub async fn sftp_rename(mgr: State<'_, SftpManager>, sftp_id: Uuid, path: String, new_name: String) -> CmdResult<()> {
    validate_new_name(&new_name)?;
    let (sftp, _) = mgr.get(sftp_id)?;
    let parent = path.trim_end_matches('/').rsplit_once('/').map(|(p, _)| if p.is_empty() { "/" } else { p }).unwrap_or(".");
    sftp.rename(path.clone(), join_remote(parent, &new_name)).await.map_err(sftp_err)
}

#[tauri::command]
pub async fn sftp_delete(mgr: State<'_, SftpManager>, sftp_id: Uuid, paths: Vec<String>) -> CmdResult<()> {
    let (sftp, _) = mgr.get(sftp_id)?;
    for path in paths {
        remove_remote(&sftp, &path).await?;
    }
    Ok(())
}

/// Suppression récursive. Les liens symboliques sont supprimés, jamais suivis.
fn remove_remote<'a>(
    sftp: &'a SftpSession,
    path: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = CmdResult<()>> + Send + 'a>> {
    Box::pin(async move {
        let meta = sftp.symlink_metadata(path).await.map_err(sftp_err)?;
        if meta.file_type().is_dir() {
            for entry in sftp.read_dir(path).await.map_err(sftp_err)? {
                let name = entry.file_name();
                if safe_name(&name) {
                    remove_remote(sftp, &join_remote(path, &name)).await?;
                }
            }
            sftp.remove_dir(path).await.map_err(sftp_err)
        } else {
            sftp.remove_file(path).await.map_err(sftp_err)
        }
    })
}

// ───────────────────────────── Navigation locale ─────────────────────────────

#[tauri::command]
pub fn local_home() -> String {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("/")).to_string_lossy().into_owned()
}

/// Droits Unix d'un fichier local (absents sous Windows, qui utilise des ACL).
#[cfg(unix)]
fn local_permissions(meta: &std::fs::Metadata) -> Option<String> {
    use std::os::unix::fs::PermissionsExt;
    Some(mode_string(meta.permissions().mode()))
}

#[cfg(not(unix))]
fn local_permissions(_meta: &std::fs::Metadata) -> Option<String> {
    None
}

fn local_entry(path: &Path, meta: &std::fs::Metadata, target_is_dir: bool) -> FileEntry {
    let ft = meta.file_type();
    let kind = if ft.is_dir() { "dir" } else if ft.is_symlink() { "link" } else { "file" };
    FileEntry {
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path: path.to_string_lossy().into_owned(),
        kind,
        is_dir: ft.is_dir() || (ft.is_symlink() && target_is_dir),
        size: meta.len(),
        modified: meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()),
        permissions: local_permissions(meta),
    }
}

#[tauri::command]
pub async fn local_list(path: String) -> CmdResult<Vec<FileEntry>> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::fs::canonicalize(&path).map_err(io_err)?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&dir).map_err(io_err)?.flatten() {
            let p = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&p) else { continue };
            let target_is_dir = meta.file_type().is_symlink() && p.is_dir();
            entries.push(local_entry(&p, &meta, target_is_dir));
        }
        sort_entries(&mut entries);
        Ok(entries)
    })
    .await
    .map_err(|_| CmdError::new("internal", "Tâche interrompue."))?
}

#[tauri::command]
pub fn local_realpath(path: String) -> CmdResult<String> {
    std::fs::canonicalize(path).map(|p| p.to_string_lossy().into_owned()).map_err(io_err)
}

#[tauri::command]
pub fn local_mkdir(dir: String, name: String) -> CmdResult<()> {
    validate_new_name(&name)?;
    std::fs::create_dir(Path::new(&dir).join(name)).map_err(io_err)
}

#[tauri::command]
pub fn local_rename(path: String, new_name: String) -> CmdResult<()> {
    validate_new_name(&new_name)?;
    let p = PathBuf::from(&path);
    let parent = p.parent().ok_or_else(|| CmdError::invalid("Chemin invalide."))?;
    let target = parent.join(new_name);
    if target.exists() {
        return Err(CmdError::invalid("Un élément porte déjà ce nom."));
    }
    std::fs::rename(&p, target).map_err(io_err)
}

/// Les fichiers locaux vont à la corbeille (récupérables), jamais supprimés définitivement.
#[tauri::command]
pub fn local_trash(paths: Vec<String>) -> CmdResult<()> {
    trash::delete_all(&paths).map_err(|e| CmdError::new("io", format!("Mise à la corbeille impossible : {e}")))
}

// ───────────────────────────── Transferts ─────────────────────────────

#[tauri::command]
pub fn transfers_subscribe(mgr: State<'_, SftpManager>, events: Channel<TransferEvent>) {
    *mgr.events.lock().unwrap_or_else(|p| p.into_inner()) = Some(events);
}

#[tauri::command]
pub fn transfer_cancel(mgr: State<'_, SftpManager>, transfer_id: Uuid) {
    if let Some(flag) = mgr.transfers.lock().unwrap_or_else(|p| p.into_inner()).get(&transfer_id) {
        flag.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Direction {
    Upload,
    Download,
}

/// Progression partagée d'un transfert (éventuellement récursif).
struct Job {
    id: Uuid,
    app: AppHandle,
    sftp: Arc<SftpSession>,
    cancel: Arc<AtomicBool>,
    overwrite: bool,
    total: u64,
    done: u64,
    files: u32,
    skipped: u32,
    last_emit: Instant,
}

impl Job {
    fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    fn advance(&mut self, n: u64) {
        self.done += n;
        if self.last_emit.elapsed() >= PROGRESS_INTERVAL {
            self.last_emit = Instant::now();
            self.emit_progress();
        }
    }

    fn emit_progress(&self) {
        self.app.state::<SftpManager>().emit(TransferEvent::Progress {
            id: self.id,
            done: self.done,
            total: self.total,
            files: self.files,
        });
    }
}

const CANCELLED: &str = "\u{0}cancelled";

#[tauri::command]
pub fn sftp_upload(
    app: AppHandle,
    sftp_id: Uuid,
    local_paths: Vec<String>,
    remote_dir: String,
    overwrite: bool,
) -> CmdResult<Vec<Uuid>> {
    start_transfers(app, sftp_id, Direction::Upload, local_paths, remote_dir, overwrite)
}

#[tauri::command]
pub fn sftp_download(
    app: AppHandle,
    sftp_id: Uuid,
    remote_paths: Vec<String>,
    local_dir: String,
    overwrite: bool,
) -> CmdResult<Vec<Uuid>> {
    start_transfers(app, sftp_id, Direction::Download, remote_paths, local_dir, overwrite)
}

fn start_transfers(
    app: AppHandle,
    sftp_id: Uuid,
    direction: Direction,
    sources: Vec<String>,
    destination: String,
    overwrite: bool,
) -> CmdResult<Vec<Uuid>> {
    let mgr = app.state::<SftpManager>();
    let (sftp, limiter) = mgr.get(sftp_id)?;
    let mut ids = Vec::new();
    for source in sources {
        let id = Uuid::new_v4();
        let cancel = Arc::new(AtomicBool::new(false));
        mgr.transfers.lock().unwrap_or_else(|p| p.into_inner()).insert(id, cancel.clone());
        let name = match direction {
            Direction::Upload => Path::new(&source).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            Direction::Download => remote_basename(&source).to_string(),
        };
        mgr.emit(TransferEvent::Started {
            id,
            sftp_id,
            direction: if direction == Direction::Upload { "upload" } else { "download" },
            name: name.clone(),
            destination: destination.clone(),
        });

        let (app, sftp, limiter, destination) = (app.clone(), sftp.clone(), limiter.clone(), destination.clone());
        tauri::async_runtime::spawn(async move {
            let _permit = limiter.acquire_owned().await;
            let mut job = Job { id, app: app.clone(), sftp, cancel, overwrite, total: 0, done: 0, files: 0, skipped: 0, last_emit: Instant::now() };
            let result = match direction {
                Direction::Upload => upload(&mut job, &source, &destination, &name).await,
                Direction::Download => download(&mut job, &source, &destination, &name).await,
            };
            job.emit_progress();
            let (error, cancelled) = match result {
                Ok(()) => (None, false),
                Err(e) if e == CANCELLED || job.cancelled() => (None, true),
                Err(e) => (Some(e), false),
            };
            let mgr = app.state::<SftpManager>();
            mgr.transfers.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
            mgr.emit(TransferEvent::Finished { id, error, cancelled, skipped: job.skipped });
        });
        ids.push(id);
    }
    Ok(ids)
}

// ── Envoi ──

fn local_size(path: &Path) -> u64 {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => std::fs::read_dir(path)
            .map(|rd| rd.flatten().map(|e| local_size(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) if m.is_file() => m.len(),
        _ => 0,
    }
}

async fn upload(job: &mut Job, source: &str, remote_dir: &str, name: &str) -> Result<(), String> {
    let src = PathBuf::from(source);
    job.total = tokio::task::spawn_blocking({
        let src = src.clone();
        move || local_size(&src)
    })
    .await
    .unwrap_or(0);
    upload_item(job, &src, &join_remote(remote_dir, name)).await
}

fn upload_item<'a>(
    job: &'a mut Job,
    src: &'a Path,
    dest: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
    Box::pin(async move {
        if job.cancelled() {
            return Err(CANCELLED.into());
        }
        let meta = tokio::fs::symlink_metadata(src).await.map_err(|e| format!("{} : {e}", src.display()))?;
        if meta.is_dir() {
            if !job.sftp.try_exists(dest).await.unwrap_or(false) {
                job.sftp.create_dir(dest).await.map_err(|e| format!("Création de {dest} : {e}"))?;
            }
            let mut rd = tokio::fs::read_dir(src).await.map_err(|e| e.to_string())?;
            while let Some(entry) = rd.next_entry().await.map_err(|e| e.to_string())? {
                let name = entry.file_name().to_string_lossy().into_owned();
                upload_item(job, &entry.path(), &join_remote(dest, &name)).await?;
            }
            Ok(())
        } else if meta.is_file() {
            upload_file(job, src, dest, meta.len()).await
        } else {
            // Liens symboliques et fichiers spéciaux : ignorés (jamais suivis).
            job.skipped += 1;
            Ok(())
        }
    })
}

async fn upload_file(job: &mut Job, src: &Path, dest: &str, size: u64) -> Result<(), String> {
    if !job.overwrite && job.sftp.try_exists(dest).await.unwrap_or(false) {
        job.skipped += 1;
        job.advance(size);
        return Ok(());
    }
    let mut input = tokio::fs::File::open(src).await.map_err(|e| format!("{} : {e}", src.display()))?;
    let mut output = job.sftp.create(dest).await.map_err(|e| format!("Écriture de {dest} : {e}"))?;
    let mut buf = vec![0u8; CHUNK];
    loop {
        if job.cancelled() {
            drop(output);
            let _ = job.sftp.remove_file(dest).await;
            return Err(CANCELLED.into());
        }
        let n = input.read(&mut buf).await.map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n]).await.map_err(|e| format!("Écriture de {dest} : {e}"))?;
        job.advance(n as u64);
    }
    output.shutdown().await.map_err(|e| format!("Écriture de {dest} : {e}"))?;
    job.files += 1;
    Ok(())
}

// ── Téléchargement ──

/// Profondeur maximale explorée pour estimer la taille d'un dossier distant : un
/// serveur malveillant pourrait présenter une arborescence sans fin.
const MAX_SIZE_DEPTH: u32 = 64;

fn remote_size<'a>(
    sftp: &'a SftpSession,
    path: &'a str,
    cancel: &'a AtomicBool,
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = u64> + Send + 'a>> {
    Box::pin(async move {
        if depth > MAX_SIZE_DEPTH || cancel.load(Ordering::Relaxed) {
            return 0;
        }
        let Ok(meta) = sftp.symlink_metadata(path).await else { return 0 };
        if meta.file_type().is_dir() {
            let mut total = 0;
            if let Ok(rd) = sftp.read_dir(path).await {
                for entry in rd {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let name = entry.file_name();
                    if safe_name(&name) {
                        total += remote_size(sftp, &join_remote(path, &name), cancel, depth + 1).await;
                    }
                }
            }
            total
        } else if meta.file_type().is_file() {
            meta.len()
        } else {
            0
        }
    })
}

/// Délai maximal sans recevoir de données pendant un téléchargement.
const READ_STALL: Duration = Duration::from_secs(60);

/// Quantité maximale acceptée pour un fichier annoncé à `size` octets : le double,
/// plus 64 Mio (un journal peut grossir pendant le téléchargement). Au-delà, le
/// serveur envoie plus qu'annoncé et le téléchargement est interrompu, au lieu de
/// remplir le disque.
fn download_limit(size: u64) -> u64 {
    size.saturating_mul(2).saturating_add(64 * 1024 * 1024)
}

async fn download(job: &mut Job, source: &str, local_dir: &str, name: &str) -> Result<(), String> {
    if !safe_name(name) {
        return Err(format!("Nom de fichier refusé : « {name} »."));
    }
    job.total = remote_size(&job.sftp.clone(), source, &job.cancel.clone(), 0).await;
    download_item(job, source, &Path::new(local_dir).join(name), 0).await
}

fn download_item<'a>(
    job: &'a mut Job,
    src: &'a str,
    dest: &'a Path,
    depth: u32,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + 'a>> {
    Box::pin(async move {
        if job.cancelled() {
            return Err(CANCELLED.into());
        }
        if depth > MAX_SIZE_DEPTH {
            return Err(format!("{src} : arborescence trop profonde (plus de {MAX_SIZE_DEPTH} niveaux)."));
        }
        let meta = job.sftp.symlink_metadata(src).await.map_err(|e| format!("{src} : {e}"))?;
        let ft = meta.file_type();
        if ft.is_dir() {
            match tokio::fs::symlink_metadata(dest).await {
                Ok(m) if m.is_dir() => {}
                Ok(_) => return Err(format!("{} existe et n'est pas un dossier.", dest.display())),
                Err(_) => tokio::fs::create_dir(dest).await.map_err(|e| format!("{} : {e}", dest.display()))?,
            }
            let entries: Vec<String> = job
                .sftp
                .read_dir(src)
                .await
                .map_err(|e| format!("{src} : {e}"))?
                .map(|e| e.file_name())
                .collect();
            for name in entries {
                if !safe_name(&name) {
                    job.skipped += 1;
                    continue;
                }
                download_item(job, &join_remote(src, &name), &dest.join(&name), depth + 1).await?;
            }
            Ok(())
        } else if ft.is_file() {
            download_file(job, src, dest, meta.len()).await
        } else {
            job.skipped += 1;
            Ok(())
        }
    })
}

async fn download_file(job: &mut Job, src: &str, dest: &Path, size: u64) -> Result<(), String> {
    // Ne jamais écrire à travers un lien symbolique local existant.
    if let Ok(m) = tokio::fs::symlink_metadata(dest).await {
        if m.file_type().is_symlink() || !job.overwrite {
            job.skipped += 1;
            job.advance(size);
            return Ok(());
        }
    }
    let part = dest.with_file_name(format!(
        ".{}.part-{}",
        dest.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
        &job.id.simple().to_string()[..8]
    ));
    let mut input = job.sftp.open(src).await.map_err(|e| format!("Lecture de {src} : {e}"))?;
    let mut output = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&part)
        .await
        .map_err(|e| format!("{} : {e}", dest.display()))?;
    let mut buf = vec![0u8; CHUNK];
    let limit = download_limit(size);
    let result: Result<(), String> = async {
        let mut received: u64 = 0;
        loop {
            if job.cancelled() {
                return Err(CANCELLED.into());
            }
            let n = match tokio::time::timeout(READ_STALL, input.read(&mut buf)).await {
                Err(_) => return Err(format!("Lecture de {src} : le serveur ne répond plus.")),
                Ok(r) => r.map_err(|e| format!("Lecture de {src} : {e}"))?,
            };
            if n == 0 {
                break;
            }
            received += n as u64;
            if received > limit {
                return Err(format!(
                    "Lecture de {src} : le serveur envoie bien plus de données qu'annoncé ({size} octets), téléchargement interrompu."
                ));
            }
            output.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
            job.advance(n as u64);
        }
        output.sync_all().await.map_err(|e| e.to_string())?;
        Ok(())
    }
    .await;
    drop(output);
    match result {
        Ok(()) => {
            tokio::fs::rename(&part, dest).await.map_err(|e| format!("{} : {e}", dest.display()))?;
            job.files += 1;
            Ok(())
        }
        Err(e) => {
            let _ = tokio::fs::remove_file(&part).await;
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_limit_leaves_room_but_stays_bounded() {
        assert_eq!(download_limit(0), 64 * 1024 * 1024);
        assert_eq!(download_limit(1_000), 2_000 + 64 * 1024 * 1024);
        assert_eq!(download_limit(u64::MAX), u64::MAX);
    }

    #[test]
    fn rejects_traversal_names() {
        for bad in ["", ".", "..", "../etc", "a/b", "a\\b", "x\0y"] {
            assert!(!safe_name(bad), "{bad:?} devrait être refusé");
        }
        for ok in ["fichier.txt", ".bashrc", "..cache", "nom avec espaces", "été"] {
            assert!(safe_name(ok), "{ok:?} devrait être accepté");
        }
    }

    #[test]
    fn remote_paths() {
        assert_eq!(join_remote("/", "etc"), "/etc");
        assert_eq!(join_remote("/home/demo", "a.txt"), "/home/demo/a.txt");
        assert_eq!(remote_basename("/home/demo/dossier/"), "dossier");
        assert_eq!(remote_basename("/a.txt"), "a.txt");
    }

    #[test]
    fn permissions_rendering() {
        assert_eq!(mode_string(0o100644), "rw-r--r--");
        assert_eq!(mode_string(0o40755), "rwxr-xr-x");
    }
}
