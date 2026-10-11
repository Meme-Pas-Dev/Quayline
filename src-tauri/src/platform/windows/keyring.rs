//! Trousseau système : Gestionnaire d'identifiants de Windows (identifiants génériques,
//! chiffrés par DPAPI pour la session de l'utilisateur).
//!
//! Nom de l'identifiant : « <identifiant de l'app>:<usage>:<clé>=<valeur>;… », stable
//! pour une même référence (`SecretRef`).

use windows_sys::Win32::Foundation::{GetLastError, ERROR_NOT_FOUND};
use windows_sys::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
};
use zeroize::Zeroizing;

use super::wide;
use crate::i18n::tr;
use crate::identity;
use crate::platform::SecretRef;

fn target(app_id: &str, r: &SecretRef<'_>) -> String {
    let fields: Vec<String> = r.fields.iter().map(|(k, v)| format!("{k}={v}")).collect();
    format!("{app_id}:{}:{}", r.purpose, fields.join(";"))
}

fn last_error() -> String {
    tr!("err.keyring.unavailable", detail = std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32))
}

fn write(app_id: &str, label: &str, r: &SecretRef<'_>, secret: &[u8]) -> Result<(), String> {
    let mut target = wide(&target(app_id, r));
    let mut comment = wide(label);
    let mut user = wide(app_id);
    let mut blob = Zeroizing::new(secret.to_vec());
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        Comment: comment.as_mut_ptr(),
        CredentialBlobSize: u32::try_from(blob.len()).map_err(|_| tr!("err.keyring.unavailable", detail = "size"))?,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: user.as_mut_ptr(),
        ..Default::default()
    };
    if unsafe { CredWriteW(&credential, 0) } == 0 {
        return Err(last_error());
    }
    Ok(())
}

fn read(app_id: &str, r: &SecretRef<'_>) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    let target = wide(&target(app_id, r));
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    if unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        return if unsafe { GetLastError() } == ERROR_NOT_FOUND { Ok(None) } else { Err(last_error()) };
    }
    // Copie du secret, puis effacement et libération de la mémoire allouée par Windows.
    let secret = unsafe {
        let c = &*credential;
        let blob = std::slice::from_raw_parts_mut(c.CredentialBlob, c.CredentialBlobSize as usize);
        let copy = Zeroizing::new(blob.to_vec());
        zeroize::Zeroize::zeroize(blob);
        CredFree(credential.cast());
        copy
    };
    Ok(Some(secret))
}

fn remove(app_id: &str, r: &SecretRef<'_>) {
    let target = wide(&target(app_id, r));
    unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
}

pub async fn store(label: &str, r: &SecretRef<'_>, secret: &[u8]) -> Result<(), String> {
    write(identity::ID, label, r, secret)
}

/// Lit un secret. S'il n'existe que sous un ancien identifiant de l'application
/// (après un renommage), il est repris sous l'identifiant actuel.
pub async fn load(label: &str, r: &SecretRef<'_>) -> Result<Option<Zeroizing<Vec<u8>>>, String> {
    for (index, app_id) in std::iter::once(identity::ID).chain(identity::migrate_from()).enumerate() {
        let Some(secret) = read(app_id, r)? else { continue };
        if index > 0 {
            write(identity::ID, label, r, &secret)?;
            remove(app_id, r);
        }
        return Ok(Some(secret));
    }
    Ok(None)
}

pub async fn delete(r: &SecretRef<'_>) {
    for app_id in std::iter::once(identity::ID).chain(identity::migrate_from()) {
        remove(app_id, r);
    }
}
