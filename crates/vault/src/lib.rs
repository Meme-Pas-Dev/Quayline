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
    #[error("No vault found.")]
    NotFound,
    #[error("A vault already exists at this location.")]
    AlreadyExists,
    #[error("Incorrect password.")]
    InvalidCredentials,
    #[error("The password must contain at least {MIN_PASSWORD_LEN} characters.")]
    WeakPassword,
    #[error("The vault has been tampered with: its integrity is no longer guaranteed.")]
    Tampered,
    #[error("Corrupted vault: {0}.")]
    Corrupt(&'static str),
    #[error("Unsupported vault version: {0}.")]
    UnsupportedVersion(u32),
    #[error("Maximum number of unlock methods reached.")]
    TooManySlots,
    #[error("The vault is already open in another window.")]
    InUse,
    #[error("The vault was modified by another program since it was opened.")]
    ModifiedElsewhere,
    #[error("Cryptographic error.")]
    Crypto,
    #[error("Input/output error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
