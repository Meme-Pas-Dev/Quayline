//! Terminal local : un shell de la machine dans un onglet, à côté des sessions SSH.
//!
//! Pseudo-terminal par `portable-pty` (PTY sous Linux et macOS, ConPTY sous Windows).
//! Le shell est celui de l'utilisateur, lancé comme shell de connexion (Linux, macOS),
//! ou PowerShell sous Windows. Indisponible dans le Flatpak : le shell y serait celui du
//! bac à sable, et en sortir demanderait une permission qui annule l'isolation.
//! Aucun secret du coffre n'y passe ; au verrouillage, il suit le réglage des sessions.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::error::{CmdError, CmdResult};
use crate::i18n::tr;
use crate::ssh::SessionEvent;
use crate::state::AppState;

struct LocalSession {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

#[derive(Default)]
pub struct LocalTermManager {
    sessions: Mutex<HashMap<Uuid, Arc<LocalSession>>>,
}

impl LocalTermManager {
    fn get(&self, id: Uuid) -> CmdResult<Arc<LocalSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .cloned()
            .ok_or_else(|| CmdError::new("no_session", tr!("err.ssh.sessionClosed")))
    }

    fn remove(&self, id: Uuid) -> Option<Arc<LocalSession>> {
        self.sessions.lock().unwrap_or_else(|p| p.into_inner()).remove(&id)
    }

    fn write(&self, id: Uuid, data: &[u8]) -> CmdResult<()> {
        let session = self.get(id)?;
        let mut writer = session.writer.lock().unwrap_or_else(|p| p.into_inner());
        writer.write_all(data).and_then(|()| writer.flush()).map_err(|_| CmdError::new("no_session", tr!("err.ssh.sessionClosed")))
    }

    /// Termine le shell (et ce qu'il a lancé au premier plan).
    pub fn close(&self, id: Uuid) {
        if let Some(session) = self.remove(id) {
            let _ = session.killer.lock().unwrap_or_else(|p| p.into_inner()).kill();
        }
    }

    /// Verrouillage (sessions fermées) et fermeture de l'application.
    pub fn close_all(&self) {
        let sessions = std::mem::take(&mut *self.sessions.lock().unwrap_or_else(|p| p.into_inner()));
        for session in sessions.values() {
            let _ = session.killer.lock().unwrap_or_else(|p| p.into_inner()).kill();
        }
    }
}

/// Terminal local possible : pas dans le Flatpak.
pub fn available() -> bool {
    !crate::identity::is_flatpak()
}

fn pty_size(cols: u32, rows: u32) -> PtySize {
    let clamp = |v: u32| v.clamp(1, 1000) as u16;
    PtySize { rows: clamp(rows), cols: clamp(cols), pixel_width: 0, pixel_height: 0 }
}

/// Shell de l'utilisateur. Sous Windows : PowerShell 7 s'il est installé, sinon Windows
/// PowerShell (toujours présent), plutôt que cmd.
fn shell_command() -> CommandBuilder {
    #[cfg(windows)]
    {
        let pwsh = std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).any(|dir| dir.join("pwsh.exe").is_file()))
            .unwrap_or(false);
        let mut cmd = CommandBuilder::new(if pwsh { "pwsh.exe" } else { "powershell.exe" });
        cmd.arg("-NoLogo");
        cmd
    }
    #[cfg(not(windows))]
    {
        // Shell de connexion (base des utilisateurs), comme un terminal de bureau.
        CommandBuilder::new_default_prog()
    }
}

#[tauri::command]
pub fn local_term_available() -> bool {
    available()
}

#[tauri::command]
pub fn local_term_open(
    app: AppHandle,
    cols: u32,
    rows: u32,
    events: Channel<SessionEvent>,
    data: Channel<InvokeResponseBody>,
) -> CmdResult<Uuid> {
    app.state::<AppState>().require_unlocked()?;
    if !available() {
        return Err(CmdError::new("unavailable", tr!("err.localTerm.flatpak")));
    }
    let failed = |e: &dyn std::fmt::Display| CmdError::new("io", tr!("err.localTerm.start", detail = e));

    let pair = native_pty_system().openpty(pty_size(cols, rows)).map_err(|e| failed(&e))?;
    let mut cmd = shell_command();
    if let Some(home) = dirs::home_dir() {
        cmd.cwd(home);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("TERM_PROGRAM", crate::identity::NAME);
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| failed(&e))?;
    // Le côté esclave appartient désormais au shell : le garder ouvert empêcherait de
    // détecter sa fin.
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| failed(&e))?;
    let writer = pair.master.take_writer().map_err(|e| failed(&e))?;

    let id = Uuid::new_v4();
    let session = Arc::new(LocalSession {
        writer: Mutex::new(writer),
        master: Mutex::new(pair.master),
        killer: Mutex::new(child.clone_killer()),
    });
    app.state::<LocalTermManager>().sessions.lock().unwrap_or_else(|p| p.into_inner()).insert(id, session);

    // Sortie du shell vers l'interface.
    std::thread::spawn(move || {
        let mut buf = [0u8; 16 * 1024];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if data.send(InvokeResponseBody::Raw(buf[..n].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let _ = events.send(SessionEvent::Connected);
    // Fin du shell : la session est retirée (sous Windows, c'est ce qui termine la
    // lecture), puis l'interface est prévenue.
    let waiter_app = app.clone();
    std::thread::spawn(move || {
        let _ = child.wait();
        waiter_app.state::<LocalTermManager>().remove(id);
        let _ = events.send(SessionEvent::Closed { error: None });
    });

    Ok(id)
}

#[tauri::command]
pub fn local_term_write(state: State<'_, AppState>, terms: State<'_, LocalTermManager>, id: Uuid, data: String) -> CmdResult<()> {
    state.require_unlocked()?;
    terms.write(id, data.as_bytes())
}

/// Colle le presse-papiers (lu par le backend, jamais par l'interface).
#[tauri::command]
pub fn local_term_paste(
    state: State<'_, AppState>,
    clipboard: State<'_, crate::clipboard::SecretClipboard>,
    terms: State<'_, LocalTermManager>,
    id: Uuid,
    bracketed: bool,
) -> CmdResult<()> {
    state.require_unlocked()?;
    let text = clipboard
        .read_text()
        .map_err(|e| CmdError::new("clipboard", tr!("err.clipboard.unreadable", detail = e)))?;
    if text.is_empty() {
        return Ok(());
    }
    terms.write(id, &crate::clipboard::prepare_paste(&text, bracketed))
}

#[tauri::command]
pub fn local_term_resize(terms: State<'_, LocalTermManager>, id: Uuid, cols: u32, rows: u32) -> CmdResult<()> {
    let session = terms.get(id)?;
    let master = session.master.lock().unwrap_or_else(|p| p.into_inner());
    master.resize(pty_size(cols, rows)).map_err(|e| CmdError::new("io", e.to_string()))
}

#[tauri::command]
pub fn local_term_close(terms: State<'_, LocalTermManager>, id: Uuid) {
    terms.close(id);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un vrai pseudo-terminal : la sortie d'une commande revient par le lecteur, et la
    /// fin du processus est détectée.
    #[cfg(unix)]
    #[test]
    fn pty_runs_a_command() {
        let pair = native_pty_system().openpty(pty_size(80, 24)).unwrap();
        let mut cmd = CommandBuilder::new("sh");
        cmd.args(["-c", "printf 'quayline-%s' ok"]);
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 1024];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
            if out.windows(11).any(|w| w == b"quayline-ok") {
                break;
            }
        }
        assert!(String::from_utf8_lossy(&out).contains("quayline-ok"));
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn size_is_clamped() {
        let s = pty_size(0, 5000);
        assert_eq!((s.cols, s.rows), (1, 1000));
    }
}
