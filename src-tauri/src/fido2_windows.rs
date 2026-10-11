//! Clés FIDO2 sous Windows, par l'API WebAuthn du système (`webauthn.dll`).
//!
//! Depuis Windows 10 1903, un programme sans droits administrateur ne peut plus
//! parler directement à une clé en HID : Windows s'interpose et affiche sa propre
//! fenêtre (choix de la clé, code PIN, toucher). L'extension `hmac-secret` reste
//! disponible : les sels sont transmis tels quels à la clé
//! (`WEBAUTHN_AUTHENTICATOR_HMAC_SECRET_VALUES_FLAG`), comme sous Linux et macOS. Une
//! clé enregistrée sur un autre système ouvre donc aussi le coffre sous Windows.

use std::ffi::c_void;

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use uuid::Uuid;
use vault::Fido2Binding;
use windows_sys::core::{BOOL, HRESULT};
use windows_sys::Win32::Foundation::{E_INVALIDARG, NTE_EXISTS, NTE_NOT_FOUND, NTE_USER_CANCELLED};
use windows_sys::Win32::Networking::WindowsWebServices::*;
use zeroize::Zeroizing;

use crate::error::{CmdError, CmdResult};
use crate::i18n::tr;
use crate::identity;

/// Temps laissé à l'utilisateur pour la fenêtre de Windows.
const TIMEOUT_MS: u32 = 120_000;
/// Les sels `hmac-secret` passés directement à la clé sont arrivés avec la version 3
/// de l'API (Windows 10 2004, Windows 11).
const MIN_API_VERSION: u32 = WEBAUTHN_API_VERSION_3;

fn fido_err(message: impl Into<String>) -> CmdError {
    CmdError::new("fido2", message)
}

/// Version de l'API WebAuthn du système, si les clés y sont utilisables.
pub fn available() -> bool {
    unsafe { WebAuthNGetApiVersionNumber() >= MIN_API_VERSION }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Erreur WebAuthn traduite ; les cas courants ont leur message.
fn webauthn_err(hr: HRESULT) -> CmdError {
    match hr {
        NTE_USER_CANCELLED => fido_err(tr!("err.fido2.cancelled")),
        NTE_NOT_FOUND => fido_err(tr!("err.fido2.notRegistered")),
        NTE_EXISTS => fido_err(tr!("err.fido2.alreadyRegistered")),
        _ => {
            let name = unsafe {
                let p = WebAuthNGetErrorName(hr);
                if p.is_null() {
                    String::new()
                } else {
                    String::from_utf16_lossy(std::slice::from_raw_parts(p, (0..).take_while(|&i| *p.add(i) != 0).count()))
                }
            };
            fido_err(tr!("err.fido2.generic", detail = format!("{name} (0x{:08X})", hr as u32)))
        }
    }
}

/// Appelle `call` avec la version de structure la plus récente acceptée par le
/// système : un `webauthn.dll` plus ancien refuse une version qu'il ne connaît pas
/// (E_INVALIDARG) avant d'afficher quoi que ce soit.
fn with_versions(newest: u32, oldest: u32, mut call: impl FnMut(u32) -> HRESULT) -> HRESULT {
    let mut hr = E_INVALIDARG;
    for version in (oldest..=newest).rev() {
        hr = call(version);
        if hr != E_INVALIDARG {
            break;
        }
    }
    hr
}

fn client_data(kind: &str, challenge: &[u8]) -> Vec<u8> {
    format!(
        r#"{{"type":"{kind}","challenge":"{}","origin":"https://{}","crossOrigin":false}}"#,
        B64URL.encode(challenge),
        identity::ID
    )
    .into_bytes()
}

fn random<const N: usize>() -> [u8; N] {
    use rand_core::{OsRng, RngCore};
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

/// Vérification de l'utilisateur (PIN) attestée par la clé : bit UV des données de
/// l'authentificateur (32 octets d'empreinte du RP, puis les indicateurs).
fn user_verified(authenticator_data: &[u8]) -> bool {
    authenticator_data.get(32).is_some_and(|flags| flags & 0x04 != 0)
}

fn extension_enabled(extensions: &WEBAUTHN_EXTENSIONS, id: &str) -> bool {
    let wanted: Vec<u16> = id.encode_utf16().collect();
    (0..extensions.cExtensions as usize).any(|i| unsafe {
        let ext = &*extensions.pExtensions.add(i);
        let name = ext.pwszExtensionIdentifier;
        let len = (0..).take_while(|&j| *name.add(j) != 0).count();
        std::slice::from_raw_parts(name, len) == wanted.as_slice()
            && ext.cbExtension as usize >= std::mem::size_of::<BOOL>()
            && *(ext.pvExtension as *const BOOL) != 0
    })
}

/// Crée un identifiant sur la clé choisie dans la fenêtre de Windows, avec
/// `hmac-secret` et vérification de l'utilisateur. Refuse une clé déjà enregistrée.
pub fn make_credential(hwnd: isize, rp_id: &str, exclude: &[Vec<u8>]) -> CmdResult<Vec<u8>> {
    let rp_id_w = wide(rp_id);
    let rp_name = wide(identity::NAME);
    let rp = WEBAUTHN_RP_ENTITY_INFORMATION {
        dwVersion: WEBAUTHN_RP_ENTITY_INFORMATION_CURRENT_VERSION,
        pwszId: rp_id_w.as_ptr(),
        pwszName: rp_name.as_ptr(),
        pwszIcon: std::ptr::null(),
    };
    let mut user_id = random::<16>();
    let user_name = wide(identity::SLUG);
    let user_display = wide(identity::NAME);
    let user = WEBAUTHN_USER_ENTITY_INFORMATION {
        dwVersion: WEBAUTHN_USER_ENTITY_INFORMATION_CURRENT_VERSION,
        cbId: user_id.len() as u32,
        pbId: user_id.as_mut_ptr(),
        pwszName: user_name.as_ptr(),
        pwszIcon: std::ptr::null(),
        pwszDisplayName: user_display.as_ptr(),
    };
    let mut algorithms: Vec<WEBAUTHN_COSE_CREDENTIAL_PARAMETER> = [WEBAUTHN_COSE_ALGORITHM_ECDSA_P256_WITH_SHA256, -8]
        .into_iter()
        .map(|alg| WEBAUTHN_COSE_CREDENTIAL_PARAMETER {
            dwVersion: WEBAUTHN_COSE_CREDENTIAL_PARAMETER_CURRENT_VERSION,
            pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
            lAlg: alg,
        })
        .collect();
    let params = WEBAUTHN_COSE_CREDENTIAL_PARAMETERS {
        cCredentialParameters: algorithms.len() as u32,
        pCredentialParameters: algorithms.as_mut_ptr(),
    };
    let mut data = client_data("webauthn.create", &random::<32>());
    let client = WEBAUTHN_CLIENT_DATA {
        dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION,
        cbClientDataJSON: data.len() as u32,
        pbClientDataJSON: data.as_mut_ptr(),
        pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256,
    };

    let mut hmac: BOOL = 1;
    let mut extension = WEBAUTHN_EXTENSION {
        pwszExtensionIdentifier: WEBAUTHN_EXTENSIONS_IDENTIFIER_HMAC_SECRET,
        cbExtension: std::mem::size_of::<BOOL>() as u32,
        pvExtension: (&raw mut hmac).cast::<c_void>(),
    };
    let mut excluded: Vec<WEBAUTHN_CREDENTIAL_EX> = exclude
        .iter()
        .map(|id| WEBAUTHN_CREDENTIAL_EX {
            dwVersion: WEBAUTHN_CREDENTIAL_EX_CURRENT_VERSION,
            cbId: id.len() as u32,
            pbId: id.as_ptr().cast_mut(),
            pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
            dwTransports: 0,
        })
        .collect();
    let mut excluded_ptrs: Vec<*mut WEBAUTHN_CREDENTIAL_EX> = excluded.iter_mut().map(std::ptr::from_mut).collect();
    let mut exclude_list =
        WEBAUTHN_CREDENTIAL_LIST { cCredentials: excluded_ptrs.len() as u32, ppCredentials: excluded_ptrs.as_mut_ptr() };

    let mut attestation: *mut WEBAUTHN_CREDENTIAL_ATTESTATION = std::ptr::null_mut();
    let hr = with_versions(
        WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS_CURRENT_VERSION,
        WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS_VERSION_3,
        |version| {
            let options = WEBAUTHN_AUTHENTICATOR_MAKE_CREDENTIAL_OPTIONS {
                dwVersion: version,
                dwTimeoutMilliseconds: TIMEOUT_MS,
                Extensions: WEBAUTHN_EXTENSIONS { cExtensions: 1, pExtensions: &raw mut extension },
                dwAuthenticatorAttachment: WEBAUTHN_AUTHENTICATOR_ATTACHMENT_CROSS_PLATFORM,
                bRequireResidentKey: 0,
                dwUserVerificationRequirement: WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED,
                dwAttestationConveyancePreference: WEBAUTHN_ATTESTATION_CONVEYANCE_PREFERENCE_NONE,
                pExcludeCredentialList: if exclude.is_empty() { std::ptr::null_mut() } else { &raw mut exclude_list },
                ..Default::default()
            };
            unsafe { WebAuthNAuthenticatorMakeCredential(hwnd as _, &rp, &user, &params, &client, &options, &mut attestation) }
        },
    );
    if hr < 0 || attestation.is_null() {
        return Err(webauthn_err(hr));
    }
    let result = unsafe {
        let a = &*attestation;
        let id = std::slice::from_raw_parts(a.pbCredentialId, a.cbCredentialId as usize).to_vec();
        let hmac_on = extension_enabled(&a.Extensions, "hmac-secret");
        WebAuthNFreeCredentialAttestation(attestation);
        (id, hmac_on)
    };
    match result {
        (id, true) if !id.is_empty() => Ok(id),
        _ => Err(fido_err(tr!("err.fido2.hmacNotEnabled"))),
    }
}

/// Demande à la clé choisie dans la fenêtre de Windows le secret `hmac-secret` de
/// l'un des `bindings` (chacun avec son sel). Renvoie le slot et son secret.
pub fn derive_secret(hwnd: isize, bindings: &[(Uuid, Fido2Binding)]) -> CmdResult<(Uuid, Zeroizing<[u8; 32]>)> {
    let rp_id = bindings.first().map(|(_, b)| b.rp_id.clone()).ok_or_else(|| fido_err(tr!("err.fido2.noneRegistered")))?;
    // Un seul identifiant de partie de confiance par appel : celui de l'application.
    let bindings: Vec<&(Uuid, Fido2Binding)> = bindings.iter().filter(|(_, b)| b.rp_id == rp_id).collect();
    let mut salts: Vec<Zeroizing<[u8; 32]>> = bindings
        .iter()
        .map(|(_, b)| b.salt.as_slice().try_into().map(Zeroizing::new).map_err(|_| fido_err(tr!("err.fido2.badSalt"))))
        .collect::<CmdResult<_>>()?;
    let mut salt_structs: Vec<WEBAUTHN_HMAC_SECRET_SALT> = salts
        .iter_mut()
        .map(|s| WEBAUTHN_HMAC_SECRET_SALT { cbFirst: 32, pbFirst: s.as_mut_ptr(), cbSecond: 0, pbSecond: std::ptr::null_mut() })
        .collect();
    let mut per_credential: Vec<WEBAUTHN_CRED_WITH_HMAC_SECRET_SALT> = bindings
        .iter()
        .zip(salt_structs.iter_mut())
        .map(|((_, b), salt)| WEBAUTHN_CRED_WITH_HMAC_SECRET_SALT {
            cbCredID: b.credential_id.len() as u32,
            pbCredID: b.credential_id.as_ptr().cast_mut(),
            pHmacSecretSalt: std::ptr::from_mut(salt),
        })
        .collect();
    let mut salt_values = WEBAUTHN_HMAC_SECRET_SALT_VALUES {
        pGlobalHmacSalt: std::ptr::null_mut(),
        cCredWithHmacSecretSaltList: per_credential.len() as u32,
        pCredWithHmacSecretSaltList: per_credential.as_mut_ptr(),
    };
    let mut allowed: Vec<WEBAUTHN_CREDENTIAL> = bindings
        .iter()
        .map(|(_, b)| WEBAUTHN_CREDENTIAL {
            dwVersion: WEBAUTHN_CREDENTIAL_CURRENT_VERSION,
            cbId: b.credential_id.len() as u32,
            pbId: b.credential_id.as_ptr().cast_mut(),
            pwszCredentialType: WEBAUTHN_CREDENTIAL_TYPE_PUBLIC_KEY,
        })
        .collect();
    let rp_id_w = wide(&rp_id);
    let mut data = client_data("webauthn.get", &random::<32>());
    let client = WEBAUTHN_CLIENT_DATA {
        dwVersion: WEBAUTHN_CLIENT_DATA_CURRENT_VERSION,
        cbClientDataJSON: data.len() as u32,
        pbClientDataJSON: data.as_mut_ptr(),
        pwszHashAlgId: WEBAUTHN_HASH_ALGORITHM_SHA_256,
    };

    let mut assertion: *mut WEBAUTHN_ASSERTION = std::ptr::null_mut();
    let hr = with_versions(
        WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS_CURRENT_VERSION,
        WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS_VERSION_4,
        |version| {
            let options = WEBAUTHN_AUTHENTICATOR_GET_ASSERTION_OPTIONS {
                dwVersion: version,
                dwTimeoutMilliseconds: TIMEOUT_MS,
                CredentialList: WEBAUTHN_CREDENTIALS { cCredentials: allowed.len() as u32, pCredentials: allowed.as_mut_ptr() },
                dwAuthenticatorAttachment: WEBAUTHN_AUTHENTICATOR_ATTACHMENT_CROSS_PLATFORM,
                dwUserVerificationRequirement: WEBAUTHN_USER_VERIFICATION_REQUIREMENT_REQUIRED,
                // Sels transmis tels quels à la clé (pas la variante « PRF », qui les hache).
                dwFlags: WEBAUTHN_AUTHENTICATOR_HMAC_SECRET_VALUES_FLAG,
                pHmacSecretSaltValues: &raw mut salt_values,
                ..Default::default()
            };
            unsafe { WebAuthNAuthenticatorGetAssertion(hwnd as _, rp_id_w.as_ptr(), &client, &options, &mut assertion) }
        },
    );
    if hr < 0 || assertion.is_null() {
        return Err(webauthn_err(hr));
    }
    let outcome = unsafe {
        let a = &*assertion;
        let auth_data = std::slice::from_raw_parts(a.pbAuthenticatorData, a.cbAuthenticatorData as usize);
        let credential = std::slice::from_raw_parts(a.Credential.pbId, a.Credential.cbId as usize);
        let found = bindings.iter().find(|(_, b)| b.credential_id == credential).map(|(id, _)| *id);
        let secret = (!a.pHmacSecret.is_null() && (*a.pHmacSecret).cbFirst == 32).then(|| {
            let mut out = Zeroizing::new([0u8; 32]);
            out.copy_from_slice(std::slice::from_raw_parts((*a.pHmacSecret).pbFirst, 32));
            out
        });
        let verified = user_verified(auth_data);
        WebAuthNFreeAssertion(assertion);
        (found, secret, verified)
    };
    match outcome {
        (None, _, _) => Err(fido_err(tr!("err.fido2.notRegistered"))),
        (_, _, false) => Err(fido_err(tr!("err.fido2.noUv"))),
        (_, None, _) => Err(fido_err(tr!("err.fido2.noSecret"))),
        (Some(id), Some(secret), true) => Ok((id, secret)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_verification_flag() {
        let mut data = vec![0u8; 37];
        assert!(!user_verified(&data));
        data[32] = 0x05; // UP + UV
        assert!(user_verified(&data));
        assert!(!user_verified(&[0u8; 10]));
    }

    #[test]
    fn versions_fall_back_on_invalid_argument() {
        let mut tried = Vec::new();
        let hr = with_versions(7, 3, |v| {
            tried.push(v);
            if v > 5 { E_INVALIDARG } else { 0 }
        });
        assert_eq!((hr, tried), (0, vec![7, 6, 5]));
    }
}
