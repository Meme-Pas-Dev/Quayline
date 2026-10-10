//! Identité de l'application, issue de `identity.json` (voir build.rs).
//! Aucun nom n'est écrit en dur ailleurs dans le code.

/// Nom affiché (titres, fichier .desktop, nom annoncé au compositeur).
pub const NAME: &str = env!("APP_NAME");
/// Nom technique (binaire, paquet).
pub const SLUG: &str = env!("APP_SLUG");
/// Identifiant (dossier de données, trousseau système).
pub const ID: &str = env!("APP_ID");
/// Dépôt GitHub public (« propriétaire/nom ») : releases et vérification des mises à jour.
pub const REPOSITORY: &str = env!("APP_REPOSITORY");

/// Numéro de build (nombre de commits sur la branche) et commit, ou « dev ».
pub const BUILD: &str = env!("APP_BUILD");
pub const COMMIT: &str = env!("APP_COMMIT");

/// Lancé depuis un Flatpak ?
pub fn is_flatpak() -> bool {
    std::env::var_os("FLATPAK_ID").is_some()
}

/// Anciens identifiants dont il faut reprendre les données (après un renommage).
pub fn migrate_from() -> impl Iterator<Item = &'static str> {
    env!("APP_MIGRATE_FROM").split(';').filter(|s| !s.is_empty())
}
