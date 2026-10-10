//! Coffre-fort chiffré.
//!
//! Voir [`vault`] pour le format et la hiérarchie de clés.

pub mod export;
mod model;
mod secret;
pub mod vault;

pub use model::{Host, HostKind, Identity, KnownHost, Settings, VaultData};
pub use secret::Secret;
pub use vault::{check_password_policy, Fido2Binding, KdfParams, OidcBinding, Vault, MIN_PASSWORD_LEN};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Aucun coffre trouvé.")]
    NotFound,
    #[error("Un coffre existe déjà à cet emplacement.")]
    AlreadyExists,
    #[error("Mot de passe incorrect.")]
    InvalidCredentials,
    #[error("Le mot de passe doit contenir au moins {MIN_PASSWORD_LEN} caractères.")]
    WeakPassword,
    #[error("Le coffre a été altéré : son intégrité n'est plus garantie.")]
    Tampered,
    #[error("Coffre corrompu : {0}.")]
    Corrupt(&'static str),
    #[error("Version de coffre non prise en charge : {0}.")]
    UnsupportedVersion(u32),
    #[error("Nombre maximal de moyens de déverrouillage atteint.")]
    TooManySlots,
    #[error("Erreur cryptographique.")]
    Crypto,
    #[error("Erreur d'entrée/sortie : {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
