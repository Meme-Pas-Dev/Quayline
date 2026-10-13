use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use vault::export::ExportData;
use vault::Vault;

use crate::i18n::tr;
use crate::error::{CmdError, CmdResult};

pub struct AppState {
    pub vault_path: PathBuf,
    inner: Mutex<Inner>,
    unlocking: AtomicBool,
}

/// Garantit qu'une seule tentative de création/déverrouillage tourne à la fois.
pub struct UnlockGuard<'a>(&'a AtomicBool);

impl Drop for UnlockGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// Import en cours : le fichier choisi, puis son contenu déchiffré. Rattaché à la
/// session déverrouillée et effacé au verrouillage (il contient des secrets).
pub enum PendingImport {
    File(Vec<u8>),
    Opened(ExportData),
}

struct Inner {
    vault: Option<Vault>,
    pending_import: Option<PendingImport>,
    last_activity: Instant,
    failed_unlocks: u32,
    /// Délai de grâce du fournisseur d'identité (réglage du coffre, gardé ici pour
    /// être lisible coffre verrouillé) et dernière connexion par le navigateur.
    /// En mémoire seulement : après un redémarrage, une connexion est exigée.
    oidc_grace: Duration,
    last_oidc_login: Option<Instant>,
}

fn grace_of(vault: &Vault) -> Duration {
    Duration::from_secs(u64::from(vault.data.settings.oidc_reauth_minutes) * 60)
}

impl AppState {
    pub fn new(vault_path: PathBuf) -> Self {
        Self {
            vault_path,
            inner: Mutex::new(Inner {
                vault: None,
                pending_import: None,
                last_activity: Instant::now(),
                failed_unlocks: 0,
                oidc_grace: Duration::ZERO,
                last_oidc_login: None,
            }),
            unlocking: AtomicBool::new(false),
        }
    }

    fn lock_inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        // Un panic pendant qu'on tient le verrou ne doit pas rendre l'app inutilisable.
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn is_unlocked(&self) -> bool {
        self.lock_inner().vault.is_some()
    }

    /// Garde des commandes d'action (saisie dans un terminal, transferts, fichiers
    /// locaux) : refusées coffre verrouillé, même si l'interface les envoyait.
    pub fn require_unlocked(&self) -> CmdResult<()> {
        if self.is_unlocked() { Ok(()) } else { Err(CmdError::locked()) }
    }

    pub fn set_vault(&self, vault: Vault) {
        let mut inner = self.lock_inner();
        inner.oidc_grace = grace_of(&vault);
        inner.vault = Some(vault);
        inner.pending_import = None;
        inner.last_activity = Instant::now();
        inner.failed_unlocks = 0;
    }

    /// Détruit le coffre déverrouillé : la DEK et tous les secrets sont effacés.
    pub fn lock(&self) -> bool {
        let mut inner = self.lock_inner();
        inner.pending_import = None;
        inner.vault.take().is_some()
    }

    /// Remplace l'import en cours (coffre déverrouillé uniquement).
    pub fn set_pending_import(&self, pending: Option<PendingImport>) -> CmdResult<()> {
        let mut inner = self.lock_inner();
        if inner.vault.is_none() {
            return Err(CmdError::locked());
        }
        inner.pending_import = pending;
        Ok(())
    }

    /// Retire l'import en cours pour le traiter.
    pub fn take_pending_import(&self) -> CmdResult<PendingImport> {
        let mut inner = self.lock_inner();
        if inner.vault.is_none() {
            return Err(CmdError::locked());
        }
        inner.pending_import.take().ok_or_else(|| CmdError::new("no_import", tr!("err.noImport")))
    }

    /// Âge maximal accepté pour la connexion chez le fournisseur : `None` s'il faut
    /// une nouvelle authentification (délai à 0, écoulé, ou aucune connexion depuis le
    /// démarrage), sinon le délai de grâce.
    pub fn oidc_grace_window(&self) -> Option<Duration> {
        let inner = self.lock_inner();
        let last = inner.last_oidc_login?;
        (!inner.oidc_grace.is_zero() && last.elapsed() < inner.oidc_grace).then_some(inner.oidc_grace)
    }

    /// Connexion par le navigateur réussie : point de départ du délai de grâce.
    pub fn record_oidc_login(&self) {
        self.lock_inner().last_oidc_login = Some(Instant::now());
    }

    pub fn touch(&self) {
        self.lock_inner().last_activity = Instant::now();
    }

    pub fn begin_unlock(&self) -> CmdResult<UnlockGuard<'_>> {
        if self.unlocking.swap(true, Ordering::AcqRel) {
            return Err(CmdError::new("busy", tr!("err.busy")));
        }
        Ok(UnlockGuard(&self.unlocking))
    }

    pub fn record_failed_unlock(&self) {
        let mut inner = self.lock_inner();
        inner.failed_unlocks = inner.failed_unlocks.saturating_add(1);
    }

    pub fn failed_unlocks(&self) -> u32 {
        self.lock_inner().failed_unlocks
    }

    /// Exécute `f` sur le coffre déverrouillé (lecture seule).
    pub fn with_vault<T>(&self, f: impl FnOnce(&Vault) -> CmdResult<T>) -> CmdResult<T> {
        let mut inner = self.lock_inner();
        inner.last_activity = Instant::now();
        f(inner.vault.as_ref().ok_or_else(CmdError::locked)?)
    }

    /// Exécute `f` sur le coffre ; `f` est responsable de l'enregistrement.
    pub fn with_vault_mut_nosave<T>(&self, f: impl FnOnce(&mut Vault) -> CmdResult<T>) -> CmdResult<T> {
        let mut inner = self.lock_inner();
        inner.last_activity = Instant::now();
        f(inner.vault.as_mut().ok_or_else(CmdError::locked)?)
    }

    /// Exécute `f` puis enregistre. En cas d'échec d'écriture, le contenu en
    /// mémoire est restauré pour rester cohérent avec le disque.
    pub fn mutate_vault<T>(&self, f: impl FnOnce(&mut Vault) -> CmdResult<T>) -> CmdResult<T> {
        let mut inner = self.lock_inner();
        inner.last_activity = Instant::now();
        let vault = inner.vault.as_mut().ok_or_else(CmdError::locked)?;
        let backup = vault.data.clone();
        let out = f(vault)?;
        if let Err(e) = vault.save() {
            vault.data = backup;
            return Err(e.into());
        }
        inner.oidc_grace = grace_of(inner.vault.as_ref().expect("coffre ouvert ci-dessus"));
        Ok(out)
    }

    /// Verrouille si l'inactivité dépasse le délai configuré.
    /// Renvoie `Some(fermer_les_sessions)` si le coffre vient d'être verrouillé.
    pub fn auto_lock_if_idle(&self) -> Option<bool> {
        let mut inner = self.lock_inner();
        let vault = inner.vault.as_ref()?;
        let settings = &vault.data.settings;
        let (minutes, close) = (settings.auto_lock_minutes, settings.lock_closes_sessions);
        if minutes == 0 {
            return None;
        }
        if inner.last_activity.elapsed() >= Duration::from_secs(u64::from(minutes) * 60) {
            inner.vault = None;
            inner.pending_import = None;
            return Some(close);
        }
        None
    }

    /// Réglage « verrouiller avec la session / la mise en veille » (faux si déjà verrouillé).
    pub fn lock_on_system_events(&self) -> bool {
        self.lock_inner().vault.as_ref().is_some_and(|v| v.data.settings.lock_on_system_events)
    }

    /// Réglage « le verrouillage ferme les sessions » (vrai si le coffre est déjà verrouillé).
    pub fn lock_closes_sessions(&self) -> bool {
        self.lock_inner().vault.as_ref().is_none_or(|v| v.data.settings.lock_closes_sessions)
    }
}
