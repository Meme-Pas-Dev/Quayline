//! Copie de secrets (mots de passe du trousseau) dans le presse-papiers.
//!
//! - Le secret est copié par le backend : il ne transite jamais par l'interface.
//! - Il est marqué pour être exclu de l'historique (Klipper de KDE, gestionnaires
//!   d'historique qui respectent `x-kde-passwordManagerHint`).
//! - Il est effacé après le délai réglé, au verrouillage et à la fermeture, mais
//!   seulement s'il est toujours dans le presse-papiers : ce que l'utilisateur a
//!   copié entre-temps n'est jamais effacé. La comparaison se fait sur une empreinte
//!   SHA-256 ; le secret lui-même n'est pas conservé.

use std::sync::Mutex;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use zeroize::Zeroizing;

#[derive(Default)]
pub struct SecretClipboard {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Gardé ouvert : sous X11, le contenu n'existe que tant que son propriétaire répond.
    clipboard: Option<arboard::Clipboard>,
    /// Empreinte du secret copié en dernier, s'il n'a pas encore été effacé.
    copied: Option<[u8; 32]>,
    /// Incrémenté à chaque copie : un effacement programmé pour une copie plus ancienne
    /// ne touche pas à la suivante.
    generation: u64,
}

fn digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

impl SecretClipboard {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Copie `secret` ; renvoie le numéro de cette copie.
    pub fn copy(&self, secret: &str) -> Result<u64, String> {
        let mut inner = self.lock();
        if inner.clipboard.is_none() {
            inner.clipboard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        let clipboard = inner.clipboard.as_mut().expect("initialisé ci-dessus");
        let set = clipboard.set();
        #[cfg(target_os = "linux")]
        let set = {
            use arboard::SetExtLinux;
            set.exclude_from_history()
        };
        // Ni l'historique Win+V, ni la synchronisation entre appareils.
        #[cfg(windows)]
        let set = {
            use arboard::SetExtWindows;
            set.exclude_from_history().exclude_from_cloud()
        };
        set.text(secret.to_owned()).map_err(|e| e.to_string())?;
        inner.copied = Some(digest(secret));
        inner.generation += 1;
        Ok(inner.generation)
    }

    /// Efface le presse-papiers s'il contient encore le secret copié (de la copie
    /// `generation`, ou de n'importe laquelle si `None`). Si son contenu est illisible,
    /// on efface quand même : mieux vaut perdre une copie que laisser un secret.
    pub fn clear_if_unchanged(&self, generation: Option<u64>) {
        let mut inner = self.lock();
        if generation.is_some_and(|g| g != inner.generation) {
            return;
        }
        let Some(expected) = inner.copied.take() else { return };
        let Some(clipboard) = inner.clipboard.as_mut() else { return };
        match clipboard.get_text().map(Zeroizing::new) {
            Ok(text) if digest(&text) != expected => {}
            _ => {
                let _ = clipboard.clear();
            }
        }
    }

    /// Texte du presse-papiers, lu par le backend pour le coller dans un terminal :
    /// l'interface n'a pas le droit de lire le presse-papiers (un mot de passe copié
    /// depuis le trousseau ne doit pas pouvoir y remonter).
    pub fn read_text(&self) -> Result<Zeroizing<String>, String> {
        let mut inner = self.lock();
        if inner.clipboard.is_none() {
            inner.clipboard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
        }
        let clipboard = inner.clipboard.as_mut().expect("initialisé ci-dessus");
        clipboard.get_text().map(Zeroizing::new).map_err(|e| e.to_string())
    }
}

/// Prépare un texte collé pour un terminal, comme xterm.js : fins de ligne en `\r`,
/// séquences de collage retirées du texte (elles permettraient de « sortir » du mode
/// collage et d'exécuter des commandes), encadrement si l'application l'a demandé.
pub fn prepare_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let cleaned = text.replace("\x1b[200~", "").replace("\x1b[201~", "");
    let cleaned = cleaned.replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        format!("\x1b[200~{cleaned}\x1b[201~").into_bytes()
    } else {
        cleaned.into_bytes()
    }
}

/// Copie un secret et programme son effacement après `clear_after` (0 : jamais).
pub fn copy_secret(app: &AppHandle, secret: &str, clear_after: u32) -> Result<(), String> {
    let generation = app.state::<SecretClipboard>().copy(secret)?;
    if clear_after > 0 {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_secs(u64::from(clear_after))).await;
            let app2 = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                app2.state::<SecretClipboard>().clear_if_unchanged(Some(generation));
            })
            .await;
        });
    }
    Ok(())
}

/// Au verrouillage et à la fermeture : retire un secret encore présent.
pub fn clear_now(app: &AppHandle) {
    app.state::<SecretClipboard>().clear_if_unchanged(None);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_is_stable_and_distinct() {
        assert_eq!(digest("abc"), digest("abc"));
        assert_ne!(digest("abc"), digest("abd"));
    }

    #[test]
    fn paste_is_sanitized() {
        assert_eq!(prepare_paste("a\nb\r\nc", false), b"a\rb\rc");
        assert_eq!(prepare_paste("ls\x1b[201~; rm -rf ~\n", true), b"\x1b[200~ls; rm -rf ~\r\x1b[201~");
    }

    #[test]
    fn stale_generation_is_ignored() {
        let c = SecretClipboard::default();
        {
            let mut inner = c.lock();
            inner.copied = Some(digest("secret"));
            inner.generation = 2;
        }
        // Un effacement programmé pour la copie n° 1 ne touche pas à la n° 2.
        c.clear_if_unchanged(Some(1));
        assert!(c.lock().copied.is_some());
    }
}
