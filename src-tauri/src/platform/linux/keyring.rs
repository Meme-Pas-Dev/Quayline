//! Trousseau système via l'API Secret Service (KWallet sous Plasma, GNOME Keyring).

use std::collections::HashMap;

use secret_service::{EncryptionType, SecretService};
use zeroize::Zeroizing;

use crate::i18n::tr;
use crate::identity;
use crate::platform::SecretRef;

fn err(e: secret_service::Error) -> String {
    tr!("err.keyring.unavailable", detail = e)
}

fn attrs<'a>(app_id: &'a str, r: &'a SecretRef<'a>) -> HashMap<&'a str, &'a str> {
    let mut map = HashMap::from([("application", app_id), ("purpose", r.purpose)]);
    map.extend(r.fields.iter().copied());
    map
}

pub async fn store(label: &str, r: &SecretRef<'_>, secret: &[u8]) -> Result<(), String> {
    let ss = SecretService::connect(EncryptionType::Dh).await.map_err(err)?;
    let collection = ss.get_default_collection().await.map_err(err)?;
    collection.ensure_unlocked().await.map_err(err)?;
    collection
        .create_item(label, attrs(identity::ID, r), secret, true, "application/octet-stream")
        .await
        .map_err(err)?;
    Ok(())
}

/// Lit un secret. S'il n'existe que sous un ancien identifiant de l'application
/// (après un renommage), il est repris sous l'identifiant actuel.
pub async fn load(label: &str, r: &SecretRef<'_>) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    let ss = SecretService::connect(EncryptionType::Dh).await.map_err(err)?;
    for (index, app_id) in std::iter::once(identity::ID).chain(identity::migrate_from()).enumerate() {
        let found = ss.search_items(attrs(app_id, r)).await.map_err(err)?;
        let Some(item) = found.unlocked.first().or(found.locked.first()) else { continue };
        item.ensure_unlocked().await.map_err(err)?;
        let secret = Zeroizing::new(item.get_secret().await.map_err(err)?);
        if index > 0 {
            store(label, r, &secret).await?;
            let _ = item.delete().await;
        }
        return Ok(Some(secret));
    }
    Ok(None)
}

pub async fn delete(r: &SecretRef<'_>) {
    let Ok(ss) = SecretService::connect(EncryptionType::Dh).await else { return };
    for app_id in std::iter::once(identity::ID).chain(identity::migrate_from()) {
        if let Ok(found) = ss.search_items(attrs(app_id, r)).await {
            for item in found.unlocked.iter().chain(found.locked.iter()) {
                let _ = item.delete().await;
            }
        }
    }
}
