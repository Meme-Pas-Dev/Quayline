use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Secret;

/// Contenu déchiffré du coffre. Tout ce qui est ici est chiffré sur disque,
/// y compris la liste des serveurs (qui ne doit pas fuiter non plus).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct VaultData {
    pub hosts: Vec<Host>,
    pub identities: Vec<Identity>,
    pub known_hosts: Vec<KnownHost>,
    pub settings: Settings,
}

/// Type de connexion principal d'un hôte : ce qu'ouvre un clic sur sa tuile.
/// L'autre reste accessible depuis la tuile.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostKind {
    /// Terminal (valeur des hôtes créés avant l'apparition de ce choix).
    #[default]
    Ssh,
    /// Explorateur de fichiers.
    Sftp,
}

/// Une session SSH/SFTP enregistrée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub id: Uuid,
    pub label: String,
    #[serde(default)]
    pub kind: HostKind,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub hostname: String,
    pub port: u16,
    /// Prioritaire sur le nom d'utilisateur de l'identité.
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub identity_id: Option<Uuid>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub notes: String,
}

/// Entrée du trousseau : identifiants réutilisables entre plusieurs hôtes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub id: Uuid,
    pub label: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<Secret>,
    /// Clé privée au format OpenSSH/PEM.
    #[serde(default)]
    pub private_key: Option<Secret>,
    #[serde(default)]
    pub key_passphrase: Option<Secret>,
}

/// Clé d'hôte approuvée (équivalent chiffré de `~/.ssh/known_hosts`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownHost {
    pub hostname: String,
    pub port: u16,
    pub algorithm: String,
    /// Empreinte `SHA256:…` telle qu'affichée par OpenSSH.
    pub fingerprint: String,
    /// Clé publique encodée en base64 (format wire SSH).
    pub public_key: String,
    /// Horodatage Unix (secondes) de l'approbation.
    pub added_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Verrouillage après inactivité ; 0 = désactivé.
    pub auto_lock_minutes: u32,
    /// Effacement du presse-papiers après copie d'un secret ; 0 = désactivé.
    pub clipboard_clear_seconds: u32,
    pub terminal_font_size: u16,
    pub terminal_theme: String,
    /// Le verrouillage ferme les sessions SSH/SFTP (choix le plus sûr, par défaut).
    /// Sinon elles restent ouvertes, masquées derrière l'écran de verrouillage.
    pub lock_closes_sessions: bool,
    /// Verrouiller quand la session du bureau se verrouille ou que la machine se met en veille.
    pub lock_on_system_events: bool,
    /// Rechercher une nouvelle version sur GitHub après le déverrouillage ; `None` tant
    /// que l'utilisateur n'a pas répondu (question posée après le déverrouillage).
    pub check_updates: Option<bool>,
    /// Déverrouillage par fournisseur d'identité dans le navigateur : délai (minutes)
    /// après la dernière connexion pendant lequel la session du fournisseur suffit ;
    /// au-delà, ou à 0, une nouvelle authentification est exigée.
    pub oidc_reauth_minutes: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: 10,
            clipboard_clear_seconds: 30,
            terminal_font_size: 14,
            terminal_theme: "night".into(),
            lock_closes_sessions: true,
            lock_on_system_events: true,
            check_updates: None,
            oidc_reauth_minutes: 15,
        }
    }
}
