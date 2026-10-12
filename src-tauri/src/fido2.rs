//! Déverrouillage par clé de sécurité FIDO2 (YubiKey, Nitrokey, SoloKey…).
//!
//! La clé du slot est la sortie de l'extension CTAP2 `hmac-secret` : un HMAC calculé
//! dans la clé physique à partir d'un secret propre à l'identifiant enregistré et d'un
//! sel stocké dans le coffre. Elle est demandée avec le code PIN de la clé (vérification
//! de l'utilisateur) et un toucher : sans la clé ET son PIN, le slot ne s'ouvre pas.
//! Rien n'est conservé dans le trousseau système.
//!
//! La clé est jointe directement en USB (hidraw), sans passer par le navigateur ; sous
//! Windows, par l'API WebAuthn du système (voir `fido2_windows.rs`), qui affiche sa
//! propre fenêtre pour le PIN et le toucher.

#[cfg(windows)]
#[path = "fido2_windows.rs"]
mod webauthn;

use std::sync::{mpsc, OnceLock};

use ctap_hid_fido2::fidokey::get_assertion::get_assertion_params::Extension as Gext;
use ctap_hid_fido2::fidokey::make_credential::make_credential_params::Extension as Mext;
use ctap_hid_fido2::fidokey::{GetAssertionArgsBuilder, MakeCredentialArgsBuilder};
use ctap_hid_fido2::public_key_credential_user_entity::PublicKeyCredentialUserEntity;
use ctap_hid_fido2::{get_fidokey_devices, FidoKeyHid, FidoKeyHidFactory, LibCfg};
use rand_core::{OsRng, RngCore};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};
use tokio::sync::oneshot;
use uuid::Uuid;
use vault::{Fido2Binding, Secret, Vault};
use zeroize::Zeroizing;

use crate::i18n::tr;
use crate::error::{CmdError, CmdResult};
use crate::identity;
use crate::state::AppState;

const MAX_LABEL: usize = 60;
/// Longueur minimale d'un PIN FIDO2 (CTAP2), si la clé n'en annonce pas d'autre.
const MIN_PIN: usize = 4;
/// Longueur maximale d'un PIN FIDO2 : 63 octets UTF-8.
const MAX_PIN_BYTES: usize = 63;

fn fido_err(message: impl Into<String>) -> CmdError {
    CmdError::new("fido2", message)
}

/// Traduit les erreurs CTAP2 (renvoyées sous forme de texte par la bibliothèque).
fn ctap_err(e: impl std::fmt::Display) -> CmdError {
    let msg = e.to_string();
    type Known = (&'static str, &'static str, fn() -> String);
    let known: [Known; 11] = [
        ("CTAP2_ERR_PIN_INVALID", "fido2_pin", || tr!("err.fido2.pinInvalid")),
        ("CTAP2_ERR_PIN_AUTH_BLOCKED", "fido2", || tr!("err.fido2.authBlocked")),
        ("CTAP2_ERR_PIN_BLOCKED", "fido2", || tr!("err.fido2.pinBlocked")),
        ("CTAP2_ERR_PIN_NOT_SET", "fido2", || tr!("err.fido2.pinNotSet")),
        ("CTAP2_ERR_PIN_POLICY_VIOLATION", "fido2_pin", || tr!("err.fido2.pinPolicy")),
        ("TIMEOUT", "fido2", || tr!("err.fido2.timeout")),
        ("CTAP2_ERR_KEEPALIVE_CANCEL", "fido2", || tr!("err.fido2.cancelled")),
        ("CTAP2_ERR_NO_CREDENTIALS", "fido2", || tr!("err.fido2.notRegistered")),
        ("CTAP2_ERR_CREDENTIAL_EXCLUDED", "fido2", || tr!("err.fido2.alreadyRegistered")),
        ("CTAP2_ERR_OPERATION_DENIED", "fido2", || tr!("err.fido2.denied")),
        ("CTAP2_ERR_UNSUPPORTED_EXTENSION", "fido2", || tr!("err.fido2.noHmac")),
    ];
    match known.iter().find(|(needle, _, _)| msg.contains(needle)) {
        Some((_, code, text)) => CmdError::new(code, text()),
        None => fido_err(tr!("err.fido2.generic", detail = msg)),
    }
}

/// Comme `ctap_err`, en précisant le nombre d'essais restants après un PIN incorrect.
// Accès direct à la clé (Linux, macOS) ; Windows passe par WebAuthn.
#[cfg_attr(windows, allow(dead_code))]
fn ctap_err_with_retries(device: &FidoKeyHid, e: impl std::fmt::Display) -> CmdError {
    let mut err = ctap_err(e);
    if err.code == "fido2_pin" {
        if let Ok(left) = device.get_pin_retries() {
            err.message = match left {
                0 => tr!("err.fido2.pinLocked"),
                1 => tr!("err.fido2.pinLast"),
                n => tr!("err.fido2.pinRetries", count = n),
            };
        }
    }
    err
}

/// Ouvre l'unique clé branchée.
fn open_device() -> CmdResult<FidoKeyHid> {
    match get_fidokey_devices().len() {
        0 => Err(fido_err(tr!("err.fido2.noKey"))),
        1 => FidoKeyHidFactory::create(&LibCfg::init()).map_err(ctap_err),
        _ => Err(fido_err(tr!("err.fido2.multiple"))),
    }
}

/// Vérifie que la clé sait faire `hmac-secret` et qu'un code PIN y est défini.
// Accès direct à la clé (Linux, macOS) ; Windows passe par WebAuthn.
#[cfg_attr(windows, allow(dead_code))]
fn check_capabilities(device: &FidoKeyHid) -> CmdResult<()> {
    let info = device.get_info().map_err(ctap_err)?;
    if !info.extensions.iter().any(|e| e == "hmac-secret") {
        return Err(fido_err(tr!("err.fido2.noHmacFido2")));
    }
    if !info.options.iter().any(|(name, on)| name == "clientPin" && *on) {
        return Err(fido_err(tr!("err.fido2.setPinFirst")));
    }
    Ok(())
}

fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

/// Demande à la clé le secret `hmac-secret` de `binding` (PIN + toucher).
// Accès direct à la clé (Linux, macOS) ; Windows passe par WebAuthn.
#[cfg_attr(windows, allow(dead_code))]
fn derive_secret(device: &FidoKeyHid, binding: &Fido2Binding, pin: &str) -> CmdResult<Zeroizing<[u8; 32]>> {
    let salt: [u8; 32] = binding.salt.as_slice().try_into().map_err(|_| fido_err(tr!("err.fido2.badSalt")))?;
    let challenge = random::<32>();
    let args = GetAssertionArgsBuilder::new(&binding.rp_id, &challenge)
        .pin(pin)
        .credential_id(&binding.credential_id)
        .extensions(&[Gext::HmacSecret(Some(salt))])
        .build();
    let assertions = device.get_assertion_with_args(&args).map_err(|e| ctap_err_with_retries(device, e))?;
    let assertion = assertions.first().ok_or_else(|| fido_err(tr!("err.fido2.noResponse")))?;
    // Le secret dépend de la vérification de l'utilisateur : on l'exige.
    if !assertion.flags.user_verified_result {
        return Err(fido_err(tr!("err.fido2.noUv")));
    }
    assertion
        .extensions
        .iter()
        .find_map(|e| match e {
            Gext::HmacSecret(Some(out)) => Some(Zeroizing::new(*out)),
            _ => None,
        })
        .ok_or_else(|| fido_err(tr!("err.fido2.noSecret")))
}

/// La clé branchée détient-elle cet identifiant ? Sans PIN ni toucher.
// Accès direct à la clé (Linux, macOS) ; Windows passe par WebAuthn.
#[cfg_attr(windows, allow(dead_code))]
fn holds_credential(device: &FidoKeyHid, binding: &Fido2Binding) -> bool {
    let challenge = random::<32>();
    let args = GetAssertionArgsBuilder::new(&binding.rp_id, &challenge)
        .without_pin_and_uv()
        .without_up()
        .credential_id(&binding.credential_id)
        .build();
    device.get_assertion_with_args(&args).is_ok()
}

/// Étape de l'enregistrement, envoyée à l'interface : quel toucher est attendu.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollProgress {
    touch: u8,
}

/// Enregistre la clé branchée : création de l'identifiant puis calcul du secret.
/// Deux touchers sont demandés.
// Accès direct à la clé (Linux, macOS) ; Windows passe par WebAuthn.
#[cfg_attr(windows, allow(dead_code))]
fn enroll(
    label: String,
    pin: &str,
    existing: &[Vec<u8>],
    progress: &Channel<EnrollProgress>,
) -> CmdResult<(Fido2Binding, Zeroizing<[u8; 32]>)> {
    let device = open_device()?;
    check_capabilities(&device)?;

    let rp_id = identity::ID.to_string();
    let challenge = random::<32>();
    let user_id = random::<16>();
    let user = PublicKeyCredentialUserEntity::new(Some(&user_id), Some(identity::SLUG), Some(identity::NAME));
    let mut args = MakeCredentialArgsBuilder::new(&rp_id, &challenge)
        .pin(pin)
        .user_entity(&user)
        .extensions(&[Mext::HmacSecret(Some(true))]);
    // Refuse d'enregistrer deux fois la même clé physique.
    for id in existing {
        args = args.exclude_authenticator(id);
    }
    let _ = progress.send(EnrollProgress { touch: 1 });
    let attestation = device.make_credential_with_args(&args.build()).map_err(|e| ctap_err_with_retries(&device, e))?;
    if !attestation.extensions.iter().any(|e| matches!(e, Mext::HmacSecret(Some(true)))) {
        return Err(fido_err(tr!("err.fido2.hmacNotEnabled")));
    }

    let binding = Fido2Binding {
        label,
        rp_id,
        credential_id: attestation.credential_descriptor.id.clone(),
        salt: random::<32>().to_vec(),
    };
    let _ = progress.send(EnrollProgress { touch: 2 });
    let secret = derive_secret(&device, &binding, pin)?;
    Ok((binding, secret))
}

#[derive(Serialize)]
pub struct Fido2KeyView {
    id: Uuid,
    label: String,
}

/// Lisible sans déverrouiller : l'écran de verrouillage propose la clé si besoin.
#[tauri::command]
pub fn fido2_list(state: State<'_, AppState>) -> CmdResult<Vec<Fido2KeyView>> {
    Ok(Vault::fido2_bindings(&state.vault_path)?
        .into_iter()
        .map(|(id, b)| Fido2KeyView { id, label: b.label })
        .collect())
}

type Job = Box<dyn FnOnce() + Send>;

/// Fil unique pour tous les échanges avec les clés, qui vit aussi longtemps que
/// l'application. hidapi n'est pas sûr entre fils : sur macOS, deux énumérations
/// simultanées font planter le processus ; son gestionnaire USB est en outre rattaché
/// à la boucle d'événements du premier fil qui l'utilise.
static WORKER: OnceLock<mpsc::Sender<Job>> = OnceLock::new();

fn worker() -> &'static mpsc::Sender<Job> {
    WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("fido2".into())
            // Pas de rattrapage des panics : la version livrée est compilée avec
            // `panic = "abort"` (voir Cargo.toml), un panic y arrête l'application.
            .spawn(move || rx.into_iter().for_each(|job| job()))
            .expect("impossible de créer le fil FIDO2");
        tx
    })
}

async fn run_blocking<T: Send + 'static>(f: impl FnOnce() -> CmdResult<T> + Send + 'static) -> CmdResult<T> {
    let interrupted = || CmdError::new("internal", tr!("err.interrupted"));
    let (tx, rx) = oneshot::channel();
    worker()
        .send(Box::new(move || drop(tx.send(f()))))
        .map_err(|_| interrupted())?;
    rx.await.map_err(|_| interrupted())?
}

/// Fenêtre principale, parente de la fenêtre de Windows (PIN, toucher).
#[cfg(windows)]
fn main_hwnd(app: &AppHandle) -> isize {
    app.get_webview_window("main").and_then(|w| w.hwnd().ok()).map_or(0, |h| h.0 as isize)
}

/// Enregistrement sous Windows : identifiant créé, puis secret calculé, chacun par la
/// fenêtre de Windows (PIN et toucher).
#[cfg(windows)]
fn enroll_windows(hwnd: isize, label: String, existing: &[Vec<u8>], progress: &Channel<EnrollProgress>) -> CmdResult<(Fido2Binding, Zeroizing<[u8; 32]>)> {
    if !webauthn::available() {
        return Err(fido_err(tr!("err.fido2.windowsUnavailable")));
    }
    let rp_id = identity::ID.to_string();
    let _ = progress.send(EnrollProgress { touch: 1 });
    let credential_id = webauthn::make_credential(hwnd, &rp_id, existing)?;
    let binding = Fido2Binding { label, rp_id, credential_id, salt: random::<32>().to_vec() };
    let _ = progress.send(EnrollProgress { touch: 2 });
    let (_, secret) = webauthn::derive_secret(hwnd, &[(Uuid::nil(), binding.clone())])?;
    Ok((binding, secret))
}

/// Ajoute une clé de sécurité (mot de passe maître exigé).
#[tauri::command]
pub async fn fido2_enroll(
    app: AppHandle,
    label: String,
    pin: Secret,
    password: Secret,
    progress: Channel<EnrollProgress>,
) -> CmdResult<Fido2KeyView> {
    let label = label.trim().to_string();
    if label.is_empty() || label.chars().count() > MAX_LABEL || label.chars().any(char::is_control) {
        return Err(CmdError::invalid(tr!("err.fido2.labelRequired", max = MAX_LABEL)));
    }
    // Sous Windows, le PIN est demandé par la fenêtre du système.
    if pin.is_empty() && !cfg!(windows) {
        return Err(CmdError::new("fido2_pin", tr!("err.fido2.enterPin")));
    }

    crate::commands::confirm_master_password(&app, password).await?;
    let existing = app
        .state::<AppState>()
        .with_vault(|v| Ok(v.fido2_keys().into_iter().map(|(_, b)| b.credential_id.clone()).collect::<Vec<_>>()))?;

    let label_for_view = label.clone();
    #[cfg(windows)]
    let (binding, secret) = {
        let _ = pin;
        let hwnd = main_hwnd(&app);
        run_blocking(move || enroll_windows(hwnd, label, &existing, &progress)).await?
    };
    #[cfg(not(windows))]
    let (binding, secret) = run_blocking(move || enroll(label, pin.expose(), &existing, &progress)).await?;
    let id = app.state::<AppState>().with_vault_mut_nosave(|v| v.add_fido2(binding, &secret).map_err(Into::into))?;
    Ok(Fido2KeyView { id, label: label_for_view })
}

/// État de la clé branchée, pour l'assistant d'ajout.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fido2Device {
    /// `none`, `multiple`, `unsupported` ou `ready`.
    state: &'static str,
    product: Option<String>,
    has_pin: bool,
    min_pin_length: usize,
    pin_retries: Option<i32>,
}

impl Fido2Device {
    fn empty(state: &'static str) -> Self {
        Self { state, product: None, has_pin: false, min_pin_length: MIN_PIN, pin_retries: None }
    }
}

/// Détecte la clé branchée sans rien lui demander d'autre que ses caractéristiques
/// (pas de PIN, pas de toucher).
#[tauri::command]
pub async fn fido2_detect(state: State<'_, AppState>) -> CmdResult<Fido2Device> {
    state.with_vault(|_| Ok(()))?;
    // Windows : pas d'accès direct à la clé ; tout passe par la fenêtre du système.
    #[cfg(windows)]
    let detected = Ok(Fido2Device::empty(if webauthn::available() { "windows" } else { "unsupported" }));
    #[cfg(not(windows))]
    let detected = run_blocking(|| {
        let mut devices = get_fidokey_devices();
        match devices.len() {
            0 => return Ok(Fido2Device::empty("none")),
            1 => {}
            _ => return Ok(Fido2Device::empty("multiple")),
        }
        let product = devices.pop().map(|d| d.product_string.trim().to_string()).filter(|p| !p.is_empty());
        let device = FidoKeyHidFactory::create(&LibCfg::init()).map_err(ctap_err)?;
        let info = device.get_info().map_err(ctap_err)?;
        let has_pin = info.options.iter().any(|(name, on)| name == "clientPin" && *on);
        let supported = info.extensions.iter().any(|e| e == "hmac-secret")
            && info.options.iter().any(|(name, _)| name == "clientPin");
        Ok(Fido2Device {
            state: if supported { "ready" } else { "unsupported" },
            product,
            has_pin,
            min_pin_length: (info.min_pin_length as usize).max(MIN_PIN),
            pin_retries: if has_pin { device.get_pin_retries().ok() } else { None },
        })
    })
    .await;
    detected
}

/// Définit le code PIN d'une clé qui n'en a pas encore.
#[tauri::command]
pub async fn fido2_set_pin(state: State<'_, AppState>, pin: Secret) -> CmdResult<()> {
    state.with_vault(|_| Ok(()))?;
    if cfg!(windows) {
        return Err(fido_err(tr!("err.fido2.windowsPin")));
    }
    let chars = pin.expose().chars().count();
    if chars < MIN_PIN || pin.expose().len() > MAX_PIN_BYTES {
        return Err(CmdError::new("fido2_pin", tr!("err.fido2.pinLength", min = MIN_PIN)));
    }
    run_blocking(move || {
        let device = open_device()?;
        let info = device.get_info().map_err(ctap_err)?;
        if info.options.iter().any(|(name, on)| name == "clientPin" && *on) {
            return Err(fido_err(tr!("err.fido2.hasPin")));
        }
        let min = (info.min_pin_length as usize).max(MIN_PIN);
        if chars < min {
            return Err(CmdError::new("fido2_pin", tr!("err.fido2.pinMin", min = min)));
        }
        device.set_new_pin(pin.expose()).map_err(ctap_err)
    })
    .await
}

#[tauri::command]
pub fn fido2_remove(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    if state.with_vault_mut_nosave(|v| v.remove_fido2(id).map_err(Into::into))? {
        Ok(())
    } else {
        Err(CmdError::not_found())
    }
}

/// Déverrouille avec la clé branchée. Les essais de PIN sont limités par la clé
/// elle-même (blocage après plusieurs erreurs).
#[tauri::command]
pub async fn fido2_unlock(app: AppHandle, pin: Secret) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let _guard = state.begin_unlock()?;
    let path = state.vault_path.clone();
    #[cfg(windows)]
    let vault = {
        let _ = pin;
        let hwnd = main_hwnd(&app);
        run_blocking(move || {
            if !webauthn::available() {
                return Err(fido_err(tr!("err.fido2.windowsUnavailable")));
            }
            let bindings = Vault::fido2_bindings(&path)?;
            if bindings.is_empty() {
                return Err(fido_err(tr!("err.fido2.noneRegistered")));
            }
            // Champs pas encore authentifiés : s'ils ont été modifiés, le slot refusera.
            let (id, secret) = webauthn::derive_secret(hwnd, &bindings)?;
            Vault::unlock_with_fido2(path, id, &secret).map_err(|e| match e {
                vault::Error::InvalidCredentials => fido_err(tr!("err.fido2.mismatch")),
                other => other.into(),
            })
        })
        .await?
    };
    #[cfg(not(windows))]
    if pin.is_empty() {
        return Err(CmdError::new("fido2_pin", tr!("err.fido2.enterPin")));
    }
    #[cfg(not(windows))]
    let vault = run_blocking(move || {
        let bindings = Vault::fido2_bindings(&path)?;
        if bindings.is_empty() {
            return Err(fido_err(tr!("err.fido2.noneRegistered")));
        }
        let device = open_device()?;
        // Les champs lus ici ne sont pas encore authentifiés : s'ils ont été modifiés,
        // le slot refusera de s'ouvrir.
        let (id, binding) = bindings
            .iter()
            .find(|(_, b)| holds_credential(&device, b))
            .ok_or_else(|| fido_err(tr!("err.fido2.notRegistered")))?;
        let secret = derive_secret(&device, binding, pin.expose())?;
        Vault::unlock_with_fido2(path, *id, &secret).map_err(|e| match e {
            vault::Error::InvalidCredentials => {
                fido_err(tr!("err.fido2.mismatch"))
            }
            other => other.into(),
        })
    })
    .await?;
    state.set_vault(vault);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctap_errors_are_translated() {
        let e = ctap_err("0x31 CTAP2_ERR_PIN_INVALID   PIN Invalid.");
        assert_eq!((e.code, e.message), ("fido2_pin", tr!("err.fido2.pinInvalid")));
        let e = ctap_err("0x34 CTAP2_ERR_PIN_AUTH_BLOCKED PIN authentication, pinAuth, blocked.");
        assert_eq!(e.message, tr!("err.fido2.authBlocked"));
        let e = ctap_err("0x32 CTAP2_ERR_PIN_BLOCKED PIN Blocked.");
        assert_eq!(e.message, tr!("err.fido2.pinBlocked"));
        let e = ctap_err("something odd");
        assert_eq!(e.code, "fido2");
        assert!(e.message.contains("something odd"));
    }

    /// Des énumérations simultanées faisaient planter hidapi sur macOS (SIGTRAP).
    #[test]
    fn concurrent_calls_share_one_thread() {
        let calls: Vec<_> = (0..8)
            .map(|_| {
                tauri::async_runtime::spawn(run_blocking(|| {
                    for _ in 0..10 {
                        get_fidokey_devices();
                    }
                    Ok(std::thread::current().id())
                }))
            })
            .collect();
        let threads: Vec<_> = tauri::async_runtime::block_on(async {
            let mut threads = Vec::new();
            for call in calls {
                threads.push(call.await.unwrap().unwrap());
            }
            threads
        });
        assert!(threads.windows(2).all(|w| w[0] == w[1]));
    }

}
