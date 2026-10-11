//! Fonctions propres au système d'exploitation.
//!
//! Chaque fonction a une interface commune ici et une implémentation par système.
//! Versions Linux, macOS et Windows ; un autre système demande un module frère, sans
//! toucher au reste.
//!
//! - `hardening` : durcissement du processus (vidages mémoire, traçage) ;
//! - `keyring`   : trousseau système (Secret Service : KWallet / GNOME Keyring ;
//!   Trousseau macOS ; Gestionnaire d'identifiants de Windows) ;
//! - `session`   : verrouillage de la session et mise en veille.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::{hardening, keyring, session};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{hardening, keyring, session};
#[cfg(target_os = "macos")]
pub use macos::menu;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{hardening, keyring, session};

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod unsupported;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub use unsupported::{hardening, keyring, session};

/// Référence d'un secret dans le trousseau système : un usage et des attributs.
/// L'identifiant de l'application est ajouté automatiquement.
// Les implémentations neutres (`unsupported`) ne lisent pas ces champs.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos", windows)), allow(dead_code))]
pub struct SecretRef<'a> {
    pub purpose: &'a str,
    pub fields: Vec<(&'a str, &'a str)>,
}

/// Événements système qui doivent verrouiller le coffre.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos", windows)), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    /// La session de bureau vient d'être verrouillée.
    SessionLocked,
    /// La machine va se mettre en veille ou en hibernation.
    Suspending,
}
