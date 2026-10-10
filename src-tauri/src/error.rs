use serde::Serialize;
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
        Self::new("locked", "Le coffre est verrouillé.")
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new("invalid", message)
    }

    pub fn not_found() -> Self {
        Self::new("not_found", "Élément introuvable.")
    }
}

impl From<VaultError> for CmdError {
    fn from(e: VaultError) -> Self {
        let code = match e {
            VaultError::NotFound => "vault_missing",
            VaultError::AlreadyExists => "vault_exists",
            VaultError::InvalidCredentials => "bad_password",
            VaultError::WeakPassword => "weak_password",
            VaultError::Tampered => "tampered",
            VaultError::Corrupt(_) | VaultError::UnsupportedVersion(_) => "corrupt",
            VaultError::TooManySlots => "invalid",
            VaultError::Crypto | VaultError::Io(_) => "internal",
        };
        Self::new(code, e.to_string())
    }
}

pub type CmdResult<T> = Result<T, CmdError>;
