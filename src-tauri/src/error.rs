use serde::Serialize;

use crate::i18n::tr;
use vault::Error as VaultError;

/// Erreur renvoyée à l'interface : un code stable pour la logique,
/// un message lisible pour l'utilisateur. Jamais de secret dedans.
#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: &'static str,
    pub message: String,
}

impl CmdError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }

    pub fn locked() -> Self {
        Self::new("locked", tr!("err.locked"))
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }

    pub fn not_found() -> Self {
        Self::new("not_found", tr!("err.notFound"))
    }
}

impl From<VaultError> for CmdError {
    fn from(e: VaultError) -> Self {
        // Messages dans la langue de l'interface ; le détail technique (corruption,
        // entrée/sortie) reste celui du coffre, en anglais.
        match e {
            VaultError::NotFound => Self::new("vault_missing", tr!("err.vault.missing")),
            VaultError::AlreadyExists => Self::new("vault_exists", tr!("err.vault.exists")),
            VaultError::InvalidCredentials => Self::new("bad_password", tr!("err.vault.badPassword")),
            VaultError::WeakPassword => Self::new("weak_password", tr!("err.vault.weak", count = vault::MIN_PASSWORD_LEN)),
            VaultError::Tampered => Self::new("tampered", tr!("err.vault.tampered")),
            VaultError::Corrupt(detail) => Self::new("corrupt", tr!("err.vault.corrupt", detail = detail)),
            VaultError::UnsupportedVersion(v) => Self::new("corrupt", tr!("err.vault.version", version = v)),
            VaultError::TooManySlots => Self::new("invalid", tr!("err.vault.tooManySlots")),
            VaultError::Crypto => Self::new("internal", tr!("err.vault.crypto")),
            VaultError::Io(io) => Self::new("internal", tr!("err.vault.io", detail = io)),
        }
    }
}

pub type CmdResult<T> = Result<T, CmdError>;
