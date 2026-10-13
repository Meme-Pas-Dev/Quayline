//! Trousseau macOS (mots de passe génériques de « Trousseau d'accès »).
//!
//! Service : identifiant de l'application ; compte : usage et attributs de
//! `SecretRef`, concaténés de façon stable (voir `account`). Les appels au
//! trousseau sont bloquants (macOS peut demander l'autorisation de l'utilisateur) :
//! ils tournent hors du runtime asynchrone.

use security_framework::passwords::{
    delete_generic_password, generic_password, set_generic_password_options, PasswordOptions,
};
use zeroize::Zeroizing;

use crate::i18n::tr;
use crate::identity;
use crate::platform::SecretRef;

/// `errSecItemNotFound` : aucun élément ne correspond.
const ITEM_NOT_FOUND: i32 = -25300;

fn err(e: impl std::fmt::Display) -> String {
    tr!("err.keyring.systemUnavailable", detail = e)
}

/// Échappe les séparateurs pour que deux `SecretRef` différents ne donnent jamais
/// le même compte.
fn escape(s: &str) -> String {
    s.replace('%', "%25").replace(';', "%3B").replace('=', "%3D")
}

/// « usage;clé=valeur;clé=valeur », dans l'ordre des attributs.
fn account(r: &SecretRef<'_>) -> String {
    let mut account = escape(r.purpose);
    for (key, value) in &r.fields {
        account.push(';');
        account.push_str(&escape(key));
        account.push('=');
        account.push_str(&escape(value));
    }
    account
}

fn store_blocking(label: &str, service: &str, account: &str, secret: &[u8]) -> Result<(), String> {
    let mut options = PasswordOptions::new_generic_password(service, account);
    options.set_label(label);
    set_generic_password_options(secret, options).map_err(err)
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)
}

pub async fn store(label: &str, r: &SecretRef<'_>, secret: &[u8]) -> Result<(), String> {
    let (label, account) = (label.to_owned(), account(r));
    let secret = Zeroizing::new(secret.to_vec());
    blocking(move || store_blocking(&label, identity::ID, &account, &secret)).await?
}

/// Lit un secret. S'il n'existe que sous un ancien identifiant de l'application
/// (après un renommage), il est repris sous l'identifiant actuel.
pub async fn load(label: &str, r: &SecretRef<'_>) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    let (label, account) = (label.to_owned(), account(r));
    blocking(move || {
        for (index, service) in std::iter::once(identity::ID).chain(identity::migrate_from()).enumerate() {
            let secret = match generic_password(PasswordOptions::new_generic_password(service, &account)) {
                Ok(secret) => Zeroizing::new(secret),
                Err(e) if e.code() == ITEM_NOT_FOUND => continue,
                Err(e) => return Err(err(e)),
            };
            if index > 0 {
                store_blocking(&label, identity::ID, &account, &secret)?;
                let _ = delete_generic_password(service, &account);
            }
            return Ok(Some(secret));
        }
        Ok(None)
    })
    .await?
}

pub async fn delete(r: &SecretRef<'_>) {
    let account = account(r);
    let _ = blocking(move || {
        for service in std::iter::once(identity::ID).chain(identity::migrate_from()) {
            let _ = delete_generic_password(service, &account);
        }
    })
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_is_stable_and_unambiguous() {
        let r = SecretRef {
            purpose: "pocket-id-unlock",
            fields: vec![("issuer", "https://id.example.org"), ("subject", "abc")],
        };
        assert_eq!(account(&r), "pocket-id-unlock;issuer=https://id.example.org;subject=abc");

        // Un séparateur dans une valeur ne doit pas imiter un autre attribut.
        let a = SecretRef { purpose: "p", fields: vec![("k", "a;x=b")] };
        let b = SecretRef { purpose: "p", fields: vec![("k", "a"), ("x", "b")] };
        assert_ne!(account(&a), account(&b));
    }
}
