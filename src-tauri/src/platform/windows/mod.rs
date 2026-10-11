//! Windows : Gestionnaire d'identifiants, verrouillage de session et mise en veille,
//! durcissement du processus.

pub mod hardening;
pub mod keyring;
pub mod session;

/// Chaîne UTF-16 terminée par un zéro, pour les API Windows.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
