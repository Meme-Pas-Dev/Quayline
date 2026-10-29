//! Commandes exposées à l'interface.
//!
//! Règle d'or : aucun secret ne repart vers l'interface. Les mots de passe et clés
//! privées entrent (saisie utilisateur), mais seules des vues sans secret sortent.

use std::time::Duration;

use serde::{Deserialize, Serialize};
// Même analyseur de clés que russh (une seule version de ssh-key) : une clé acceptée
// ici est une clé que la connexion saura utiliser.
use russh::keys::ssh_key::{Kdf, LineEnding};
use russh::keys::{Algorithm, HashAlg, PrivateKey};
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager, State};
use vault::export::ExportData;
use vault::{Error as VaultError, Host, HostKind, Identity, KdfParams, Secret, Settings, Vault};
use uuid::Uuid;

use crate::i18n::tr;
use crate::error::{CmdError, CmdResult};
use crate::ssh::{self, PromptAnswer, SessionEvent, SshManager};
use crate::state::AppState;

pub(crate) const MAX_LABEL: usize = 100;

// ───────────────────────────── Application ─────────────────────────────

#[derive(Serialize)]
pub struct AppInfo {
    name: &'static str,
    version: String,
    build: &'static str,
    commit: &'static str,
    flatpak: bool,
    updates_by_store: bool,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        name: crate::identity::NAME,
        version: app.package_info().version.to_string(),
        build: crate::identity::BUILD,
        commit: crate::identity::COMMIT,
        flatpak: crate::identity::is_flatpak(),
        updates_by_store: crate::identity::UPDATES_BY_STORE,
    }
}

// ───────────────────────────── Coffre ─────────────────────────────

#[derive(Serialize)]
pub struct VaultStatus {
    exists: bool,
    unlocked: bool,
}

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> VaultStatus {
    VaultStatus { exists: Vault::exists(&state.vault_path), unlocked: state.is_unlocked() }
}

/// Lance un calcul Argon2 hors du thread de l'interface.
async fn blocking<T: Send + 'static>(
    app: &AppHandle,
    f: impl FnOnce(&AppState) -> CmdResult<T> + Send + 'static,
) -> CmdResult<T> {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || f(&app.state::<AppState>()))
        .await
        .map_err(|_| CmdError::new("internal", tr!("err.interrupted")))?
}

#[tauri::command]
pub async fn vault_create(app: AppHandle, password: Secret) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let _guard = state.begin_unlock()?;
    blocking(&app, move |state| {
        let vault = Vault::create(&state.vault_path, password.expose().as_bytes(), KdfParams::DEFAULT)?;
        state.set_vault(vault);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn vault_unlock(app: AppHandle, password: Secret) -> CmdResult<()> {
    // Un seul essai à la fois : empêche de contourner la temporisation en parallèle.
    let state = app.state::<AppState>();
    let _guard = state.begin_unlock()?;
    let result = blocking(&app, move |state| {
        let vault = Vault::unlock(&state.vault_path, password.expose().as_bytes()).map_err(|e| {
            if matches!(e, VaultError::InvalidCredentials) {
                state.record_failed_unlock();
            }
            CmdError::from(e)
        })?;
        state.set_vault(vault);
        Ok(())
    })
    .await;

    if let Err(e) = &result {
        if e.code == "bad_password" {
            // Temporisation croissante après chaque échec (1 s, 2 s, … 30 s max).
            let failures = state.failed_unlocks();
            tokio::time::sleep(Duration::from_secs(u64::from(failures.min(30)))).await;
        }
    }
    result
}

#[tauri::command]
pub fn vault_lock(
    app: AppHandle,
    state: State<'_, AppState>,
    ssh: State<'_, SshManager>,
    sftp: State<'_, crate::sftp::SftpManager>,
) {
    if state.lock_closes_sessions() {
        ssh.close_all();
        sftp.close_all();
        app.state::<crate::local_term::LocalTermManager>().close_all();
    }
    state.lock();
    crate::clipboard::clear_now(&app);
    app.state::<crate::sftp_edit::EditManager>().stop_all();
}

#[tauri::command]
pub fn vault_touch(state: State<'_, AppState>) {
    state.touch();
}

/// Confirme le mot de passe maître avant une action sensible (changement de mot de
/// passe, ajout d'un moyen de déverrouillage). Même protection que le
/// déverrouillage : un essai à la fois, temporisation croissante après un échec.
/// Sans elle, un script injecté dans l'interface pourrait tester des mots de passe
/// à la chaîne sur un coffre ouvert.
pub(crate) async fn confirm_master_password(app: &AppHandle, password: Secret) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let _guard = state.begin_unlock()?;
    // Copie des slots sous le verrou, dérivation Argon2 hors du verrou : l'interface
    // (activité, liste des hôtes…) ne reste pas bloquée pendant le calcul.
    let verifier = state.with_vault(|v| Ok(v.password_verifier()))?;
    let ok = blocking(app, move |_| verifier.verify(password.expose().as_bytes()).map_err(Into::into)).await?;
    if ok {
        return Ok(());
    }
    state.record_failed_unlock();
    tokio::time::sleep(Duration::from_secs(u64::from(state.failed_unlocks().min(30)))).await;
    Err(CmdError::new("bad_password", tr!("err.badMasterPassword")))
}

#[tauri::command]
pub async fn vault_change_password(app: AppHandle, current: Secret, new: Secret) -> CmdResult<()> {
    confirm_master_password(&app, current).await?;
    // Le changement renouvelle la clé de données : Pocket ID et les clés FIDO2 sont
    // retirés du coffre. La clé Pocket ID est aussi effacée du trousseau système.
    let oidc = app.state::<AppState>().with_vault(|v| Ok(v.oidc().cloned()))?;
    // Dérivation du nouveau mot de passe hors du verrou, puis remplacement (rapide).
    let prepared = blocking(&app, move |_| {
        vault::NewPassword::prepare(new.expose().as_bytes(), KdfParams::DEFAULT).map_err(Into::into)
    })
    .await?;
    blocking(&app, move |state| state.with_vault_mut_nosave(|vault| vault.apply_new_password(prepared).map_err(Into::into)))
        .await?;
    if let Some(binding) = oidc {
        crate::oidc::keyring_delete(&binding).await;
    }
    Ok(())
}

// ───────────────────────────── Hôtes ─────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInput {
    id: Option<Uuid>,
    label: String,
    #[serde(default)]
    kind: HostKind,
    group: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    hostname: String,
    port: u16,
    username: Option<String>,
    identity_id: Option<Uuid>,
    color: Option<String>,
    #[serde(default)]
    notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostView {
    id: Uuid,
    label: String,
    kind: HostKind,
    group: Option<String>,
    tags: Vec<String>,
    hostname: String,
    port: u16,
    username: Option<String>,
    identity_id: Option<Uuid>,
    color: Option<String>,
    notes: String,
}

impl From<&Host> for HostView {
    fn from(h: &Host) -> Self {
        Self {
            id: h.id,
            label: h.label.clone(),
            kind: h.kind,
            group: h.group.clone(),
            tags: h.tags.clone(),
            hostname: h.hostname.clone(),
            port: h.port,
            username: h.username.clone(),
            identity_id: h.identity_id,
            color: h.color.clone(),
            notes: h.notes.clone(),
        }
    }
}

#[tauri::command]
pub fn hosts_list(state: State<'_, AppState>) -> CmdResult<Vec<HostView>> {
    state.with_vault(|v| Ok(v.data.hosts.iter().map(HostView::from).collect()))
}

// Les commandes qui écrivent sur disque (chiffrement, fsync) sont marquées `async` :
// Tauri exécute les commandes synchrones sur le fil principal, qui gèle alors
// l'interface le temps de l'écriture.
#[tauri::command(async)]
pub fn host_save(state: State<'_, AppState>, host: HostInput) -> CmdResult<HostView> {
    let HostFields { label, hostname, port, username, group, color, tags, notes } = validate_host(HostFields {
        label: host.label,
        hostname: host.hostname,
        port: host.port,
        username: host.username,
        group: host.group,
        color: host.color,
        tags: host.tags,
        notes: host.notes,
    })?;

    state.mutate_vault(|v| {
        if let Some(identity_id) = host.identity_id {
            if !v.data.identities.iter().any(|i| i.id == identity_id) {
                return Err(CmdError::invalid(tr!("err.host.identity")));
            }
        }
        let record = Host {
            id: host.id.unwrap_or_else(Uuid::new_v4),
            label,
            kind: host.kind,
            group,
            tags,
            hostname,
            port,
            username,
            identity_id: host.identity_id,
            color,
            notes,
        };
        let view = HostView::from(&record);
        match host.id {
            Some(id) => {
                let slot = v.data.hosts.iter_mut().find(|h| h.id == id).ok_or_else(CmdError::not_found)?;
                *slot = record;
            }
            None => v.data.hosts.push(record),
        }
        Ok(view)
    })
}

#[tauri::command(async)]
pub fn host_delete(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.mutate_vault(|v| {
        let before = v.data.hosts.len();
        v.data.hosts.retain(|h| h.id != id);
        if v.data.hosts.len() == before {
            return Err(CmdError::not_found());
        }
        Ok(())
    })
}

// ───────────────────────────── Trousseau ─────────────────────────────

/// Champ secret envoyé par l'interface : comme l'interface ne reçoit jamais
/// les secrets existants, elle indique seulement s'il faut les garder, les
/// effacer ou les remplacer.
#[derive(Deserialize)]
#[serde(tag = "action", content = "value", rename_all = "camelCase")]
pub enum SecretField {
    Keep,
    Clear,
    Set(Secret),
}

impl SecretField {
    fn apply(self, current: Option<Secret>) -> Option<Secret> {
        match self {
            SecretField::Keep => current,
            SecretField::Clear => None,
            SecretField::Set(s) if s.is_empty() => None,
            SecretField::Set(s) => Some(s),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityInput {
    id: Option<Uuid>,
    label: String,
    username: Option<String>,
    password: SecretField,
    private_key: SecretField,
    key_passphrase: SecretField,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityView {
    id: Uuid,
    label: String,
    username: Option<String>,
    has_password: bool,
    has_private_key: bool,
    has_passphrase: bool,
    key: Option<KeyInfo>,
}

/// Informations publiques d'une clé (jamais la partie privée).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
    algorithm: String,
    fingerprint: String,
    public_key: String,
    encrypted: bool,
}

fn key_info(pem: &Secret) -> Option<KeyInfo> {
    let key = PrivateKey::from_openssh(pem.expose()).ok()?;
    let public = key.public_key();
    Some(KeyInfo {
        algorithm: public.algorithm().to_string(),
        fingerprint: public.fingerprint(HashAlg::Sha256).to_string(),
        public_key: public.to_openssh().unwrap_or_default(),
        encrypted: key.is_encrypted(),
    })
}

impl From<&Identity> for IdentityView {
    fn from(i: &Identity) -> Self {
        Self {
            id: i.id,
            label: i.label.clone(),
            username: i.username.clone(),
            has_password: i.password.is_some(),
            has_private_key: i.private_key.is_some(),
            has_passphrase: i.key_passphrase.is_some(),
            key: i.private_key.as_ref().and_then(key_info),
        }
    }
}

#[tauri::command]
pub fn identities_list(state: State<'_, AppState>) -> CmdResult<Vec<IdentityView>> {
    state.with_vault(|v| Ok(v.data.identities.iter().map(IdentityView::from).collect()))
}

#[tauri::command(async)]
pub fn identity_save(state: State<'_, AppState>, identity: IdentityInput) -> CmdResult<IdentityView> {
    let label = required(&identity.label, &tr!("field.name"))?;
    let username = optional(identity.username, &tr!("field.username"))?;

    state.mutate_vault(|v| {
        let existing = match identity.id {
            Some(id) => Some(v.data.identities.iter().find(|i| i.id == id).ok_or_else(CmdError::not_found)?.clone()),
            None => None,
        };
        let (password, private_key, key_passphrase) = match existing {
            Some(e) => (e.password.clone(), e.private_key.clone(), e.key_passphrase.clone()),
            None => (None, None, None),
        };
        let record = Identity {
            id: identity.id.unwrap_or_else(Uuid::new_v4),
            label,
            username,
            password: identity.password.apply(password),
            private_key: identity.private_key.apply(private_key).map(normalize_pem),
            key_passphrase: identity.key_passphrase.apply(key_passphrase),
        };
        validate_key(&record)?;
        if record.password.is_none() && record.private_key.is_none() {
            return Err(CmdError::invalid(tr!("err.identity.empty")));
        }

        let view = IdentityView::from(&record);
        match v.data.identities.iter_mut().find(|i| i.id == record.id) {
            Some(slot) => *slot = record,
            None => v.data.identities.push(record),
        }
        Ok(view)
    })
}

/// Clé collée depuis un éditeur : espaces superflus, fins de ligne Windows.
fn normalize_pem(pem: Secret) -> Secret {
    let mut clean = pem.expose().trim().replace("\r\n", "\n");
    clean.push('\n');
    Secret::new(clean)
}

const MAX_BCRYPT_ROUNDS: u32 = 1024;

fn validate_key(identity: &Identity) -> CmdResult<()> {
    let Some(pem) = &identity.private_key else { return Ok(()) };
    let key = PrivateKey::from_openssh(pem.expose()).map_err(|_| {
        CmdError::invalid(tr!("err.identity.keyUnreadable"))
    })?;
    if key.is_encrypted() {
        // Le nombre de tours vient du fichier : une clé fabriquée pourrait occuper
        // l'application pendant des heures (ssh-keygen en met 16 par défaut).
        if let Kdf::Bcrypt { rounds, .. } = key.kdf() {
            if *rounds > MAX_BCRYPT_ROUNDS {
                return Err(CmdError::invalid(tr!("err.identity.tooManyRounds", max = MAX_BCRYPT_ROUNDS)));
            }
        }
        if let Some(passphrase) = &identity.key_passphrase {
            key.decrypt(passphrase.expose().as_bytes())
                .map_err(|_| CmdError::invalid(tr!("err.identity.badPassphrase")))?;
        }
    }
    Ok(())
}

/// Copie le mot de passe d'une identité dans le presse-papiers, depuis le backend :
/// il ne passe pas par l'interface. Renvoie le délai d'effacement (0 : jamais).
#[tauri::command]
pub fn identity_copy_password(app: AppHandle, state: State<'_, AppState>, id: Uuid) -> CmdResult<u32> {
    let (password, clear_after) = state.with_vault(|v| {
        let identity = v.data.identities.iter().find(|i| i.id == id).ok_or_else(CmdError::not_found)?;
        let password = identity.password.clone().filter(|p| !p.is_empty());
        let password = password.ok_or_else(|| CmdError::invalid(tr!("err.identity.noPassword")))?;
        Ok((password, v.data.settings.clipboard_clear_seconds))
    })?;
    crate::clipboard::copy_secret(&app, password.expose(), clear_after)
        .map_err(|e| CmdError::new("clipboard", tr!("err.clipboard.unavailable", detail = e)))?;
    Ok(clear_after)
}

#[tauri::command(async)]
pub fn identity_delete(state: State<'_, AppState>, id: Uuid) -> CmdResult<()> {
    state.mutate_vault(|v| {
        let before = v.data.identities.len();
        v.data.identities.retain(|i| i.id != id);
        if v.data.identities.len() == before {
            return Err(CmdError::not_found());
        }
        for host in v.data.hosts.iter_mut().filter(|h| h.identity_id == Some(id)) {
            host.identity_id = None;
        }
        Ok(())
    })
}

/// Génère une paire de clés Ed25519 et l'enregistre directement dans le trousseau.
#[tauri::command(async)]
pub fn identity_generate_key(
    state: State<'_, AppState>,
    label: String,
    username: Option<String>,
) -> CmdResult<IdentityView> {
    let label = required(&label, &tr!("field.name"))?;
    let username = optional(username, &tr!("field.username"))?;
    let mut key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .map_err(|_| CmdError::new("internal", tr!("err.keygen")))?;
    key.set_comment(format!("{}:{label}", crate::identity::SLUG));
    let pem = key.to_openssh(LineEnding::LF).map_err(|_| CmdError::new("internal", tr!("err.keyEncode")))?;

    state.mutate_vault(|v| {
        let record = Identity {
            id: Uuid::new_v4(),
            label,
            username,
            password: None,
            private_key: Some(Secret::new(pem.as_str())),
            key_passphrase: None,
        };
        let view = IdentityView::from(&record);
        v.data.identities.push(record);
        Ok(view)
    })
}

// ───────────────────────────── Réglages ─────────────────────────────

/// Délais de grâce proposés pour le fournisseur d'identité (minutes ; 0 = à chaque fois).
const OIDC_REAUTH_CHOICES: &[u32] = &[0, 5, 15, 30, 60];

pub const TERMINAL_THEMES: &[&str] = &["night", "day", "catppuccin-mocha", "dracula", "nord"];

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>) -> CmdResult<Settings> {
    state.with_vault(|v| {
        let mut settings = v.data.settings.clone();
        // Thème inconnu (ancienne version, valeur retirée) : thème par défaut.
        if !TERMINAL_THEMES.contains(&settings.terminal_theme.as_str()) {
            settings.terminal_theme = Settings::default().terminal_theme;
        }
        Ok(settings)
    })
}

#[tauri::command(async)]
pub fn settings_save(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    if settings.auto_lock_minutes > 240
        || settings.clipboard_clear_seconds > 600
        || !(8..=32).contains(&settings.terminal_font_size)
        || !TERMINAL_THEMES.contains(&settings.terminal_theme.as_str())
        || !OIDC_REAUTH_CHOICES.contains(&settings.oidc_reauth_minutes)
    {
        return Err(CmdError::invalid(tr!("err.settingsRange")));
    }
    state.mutate_vault(|v| {
        v.data.settings = settings.clone();
        Ok(settings)
    })
}

// ───────────────────────────── SSH ─────────────────────────────

#[tauri::command]
pub fn ssh_open(
    app: AppHandle,
    host_id: Uuid,
    cols: u32,
    rows: u32,
    events: Channel<SessionEvent>,
    data: Channel<InvokeResponseBody>,
) -> CmdResult<Uuid> {
    ssh::open(&app, host_id, cols, rows, events, data)
}

#[tauri::command]
pub fn ssh_write(
    state: State<'_, AppState>,
    ssh: State<'_, SshManager>,
    session_id: Uuid,
    data: String,
) -> CmdResult<()> {
    state.require_unlocked()?;
    ssh.write(session_id, data.into_bytes())
}

/// Colle le presse-papiers dans un terminal. Le texte est lu et envoyé par le
/// backend, sans passer par l'interface.
#[tauri::command]
pub fn ssh_paste(
    state: State<'_, AppState>,
    clipboard: State<'_, crate::clipboard::SecretClipboard>,
    ssh: State<'_, SshManager>,
    session_id: Uuid,
    bracketed: bool,
) -> CmdResult<()> {
    state.require_unlocked()?;
    let text = clipboard
        .read_text()
        .map_err(|e| CmdError::new("clipboard", tr!("err.clipboard.unreadable", detail = e)))?;
    if text.is_empty() {
        return Ok(());
    }
    ssh.write(session_id, crate::clipboard::prepare_paste(&text, bracketed))
}

#[tauri::command]
pub fn ssh_resize(ssh: State<'_, SshManager>, session_id: Uuid, cols: u32, rows: u32) -> CmdResult<()> {
    ssh.resize(session_id, cols, rows)
}

#[tauri::command]
pub fn ssh_close(ssh: State<'_, SshManager>, session_id: Uuid) {
    ssh.close(session_id);
}

#[tauri::command]
pub fn ssh_prompt_answer(ssh: State<'_, SshManager>, prompt_id: u64, answer: PromptAnswer) {
    ssh.answer(prompt_id, answer);
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHostView {
    hostname: String,
    port: u16,
    algorithm: String,
    fingerprint: String,
    added_at: u64,
}

#[tauri::command]
pub fn known_hosts_list(state: State<'_, AppState>) -> CmdResult<Vec<KnownHostView>> {
    state.with_vault(|v| {
        Ok(v.data
            .known_hosts
            .iter()
            .map(|k| KnownHostView {
                hostname: k.hostname.clone(),
                port: k.port,
                algorithm: k.algorithm.clone(),
                fingerprint: k.fingerprint.clone(),
                added_at: k.added_at,
            })
            .collect())
    })
}

#[tauri::command(async)]
pub fn known_host_delete(state: State<'_, AppState>, hostname: String, port: u16) -> CmdResult<()> {
    state.mutate_vault(|v| {
        v.data.known_hosts.retain(|k| !(k.hostname == hostname && k.port == port));
        Ok(())
    })
}

// ───────────────────────────── Utilitaires ─────────────────────────────

fn required(value: &str, field: &str) -> CmdResult<String> {
    let v = value.trim();
    if v.is_empty() {
        return Err(CmdError::invalid(tr!("err.field.required", field = field)));
    }
    if v.chars().count() > MAX_LABEL || v.chars().any(char::is_control) {
        return Err(CmdError::invalid(tr!("err.field.invalid", field = field)));
    }
    Ok(v.to_string())
}

fn optional(value: Option<String>, field: &str) -> CmdResult<Option<String>> {
    match value.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) => required(v, field).map(Some),
    }
}

fn is_hex_color(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Champs d'un hôte soumis aux mêmes règles à la saisie et à l'import.
#[derive(Debug, Clone, PartialEq)]
struct HostFields {
    label: String,
    hostname: String,
    port: u16,
    username: Option<String>,
    group: Option<String>,
    color: Option<String>,
    tags: Vec<String>,
    notes: String,
}

impl HostFields {
    fn of(h: &vault::Host) -> Self {
        HostFields {
            label: h.label.clone(),
            hostname: h.hostname.clone(),
            port: h.port,
            username: h.username.clone(),
            group: h.group.clone(),
            color: h.color.clone(),
            tags: h.tags.clone(),
            notes: h.notes.clone(),
        }
    }
}

/// Règles d'un hôte (saisie et import) : renvoie les champs normalisés (espaces
/// retirés, champs vides absents), ou l'erreur du premier champ refusé.
fn validate_host(f: HostFields) -> CmdResult<HostFields> {
    let label = required(&f.label, &tr!("field.name"))?;
    let hostname = f.hostname.trim().to_string();
    if !valid_hostname(&hostname) {
        return Err(CmdError::invalid(tr!("err.host.address")));
    }
    if f.port == 0 {
        return Err(CmdError::invalid(tr!("err.host.port")));
    }
    let username = optional(f.username, &tr!("field.username"))?;
    if username.as_deref().is_some_and(|u| u.chars().any(char::is_whitespace)) {
        return Err(CmdError::invalid(tr!("err.host.username")));
    }
    let group = optional(f.group, &tr!("field.group"))?;
    let color = optional(f.color, &tr!("field.color"))?;
    if color.as_deref().is_some_and(|c| !is_hex_color(c)) {
        return Err(CmdError::invalid(tr!("err.host.color")));
    }
    let tags: Vec<String> = f.tags.iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    if tags.len() > 16 || tags.iter().any(|t| t.chars().count() > 32 || t.chars().any(char::is_control)) {
        return Err(CmdError::invalid(tr!("err.host.tags")));
    }
    if f.notes.chars().count() > 4000 {
        return Err(CmdError::invalid(tr!("err.host.notes")));
    }
    Ok(HostFields { label, hostname, port: f.port, username, group, color, tags, notes: f.notes })
}

fn valid_hostname(h: &str) -> bool {
    !h.is_empty() && h.len() <= 253 && !h.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Plafond par catégorie d'un fichier importé (bien au-delà d'un usage réel).
const MAX_IMPORTED: usize = 10_000;

/// Vérifie un fichier importé avec les mêmes règles que la saisie : il a pu être
/// fabriqué par quelqu'un d'autre que l'utilisateur. Un thème de terminal inconnu
/// (version plus récente) est remplacé par le thème par défaut.
pub(crate) fn validate_import(data: &mut ExportData) -> CmdResult<()> {
    let bad = |what: String, label: &str| CmdError::invalid(tr!("err.file.invalidItem", what = what, label = label));
    if data.hosts.len() > MAX_IMPORTED || data.identities.len() > MAX_IMPORTED || data.known_hosts.len() > MAX_IMPORTED {
        return Err(CmdError::invalid(tr!("err.file.tooMany")));
    }
    for h in &data.hosts {
        // Mêmes règles que la saisie ; le fichier doit en plus être déjà normalisé
        // (une valeur que la saisie aurait modifiée trahit un fichier fabriqué).
        let fields = HostFields::of(h);
        if validate_host(fields.clone()).ok() != Some(fields) {
            return Err(bad(tr!("item.host"), &h.label.chars().take(40).collect::<String>()));
        }
    }
    for i in &data.identities {
        let ok = required(&i.label, "").is_ok_and(|l| l == i.label)
            && i.username.as_deref().is_none_or(|u| required(u, "").is_ok_and(|v| v == u))
            && (i.password.is_some() || i.private_key.is_some())
            && validate_key(i).is_ok();
        if !ok {
            return Err(bad(tr!("item.identity"), &i.label.chars().take(40).collect::<String>()));
        }
    }
    for k in &data.known_hosts {
        let ok = valid_hostname(&k.hostname)
            && k.port != 0
            && !k.algorithm.is_empty()
            && k.algorithm.len() <= 64
            && k.fingerprint.starts_with("SHA256:")
            && k.fingerprint.len() <= 128
            && !k.public_key.is_empty()
            && k.public_key.len() <= 16 * 1024;
        if !ok {
            return Err(bad(tr!("item.knownHost"), &k.hostname.chars().take(40).collect::<String>()));
        }
    }
    if let Some(s) = &mut data.settings {
        if !TERMINAL_THEMES.contains(&s.terminal_theme.as_str()) {
            s.terminal_theme = Settings::default().terminal_theme;
        }
        if s.auto_lock_minutes > 240
            || s.clipboard_clear_seconds > 600
            || !(8..=32).contains(&s.terminal_font_size)
            || !OIDC_REAUTH_CHOICES.contains(&s.oidc_reauth_minutes)
        {
            return Err(CmdError::invalid(tr!("err.file.settings")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_with_key(pem: &str, passphrase: Option<&str>) -> Identity {
        Identity {
            id: Uuid::new_v4(),
            label: "test".into(),
            username: None,
            password: None,
            private_key: Some(normalize_pem(Secret::new(pem))),
            key_passphrase: passphrase.map(Secret::new),
        }
    }

    fn fields() -> HostFields {
        HostFields {
            label: "Web".into(),
            hostname: "web.example.org".into(),
            port: 22,
            username: Some("deploy".into()),
            group: None,
            color: Some("#aabbcc".into()),
            tags: vec!["prod".into()],
            notes: String::new(),
        }
    }

    #[test]
    fn host_rules_are_shared_by_input_and_import() {
        // Saisie : normalisée.
        let typed = HostFields { label: "  Web ".into(), tags: vec![" prod ".into(), "".into()], ..fields() };
        assert_eq!(validate_host(typed).unwrap(), fields());
        // Import : déjà normalisé, sinon refusé.
        assert_eq!(validate_host(fields()).ok(), Some(fields()));
        let too_many = HostFields { tags: (0..17).map(|i| format!("t{i}")).collect(), ..fields() };
        assert!(validate_host(too_many).is_err(), "même limite pour la saisie et l'import");
        assert!(validate_host(HostFields { username: Some("a b".into()), ..fields() }).is_err());
        assert!(validate_host(HostFields { color: Some("red".into()), ..fields() }).is_err());
        assert!(validate_host(HostFields { port: 0, ..fields() }).is_err());
    }

    #[test]
    fn pasted_key_with_crlf_and_spaces_is_accepted() {
        let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let pem = key.to_openssh(LineEnding::CRLF).unwrap();
        let identity = identity_with_key(&format!("  \n{}\n\n", pem.as_str()), None);
        validate_key(&identity).unwrap();
        let info = key_info(identity.private_key.as_ref().unwrap()).unwrap();
        assert_eq!(info.algorithm, "ssh-ed25519");
        assert!(info.fingerprint.starts_with("SHA256:"));
        assert!(!info.encrypted);
    }

    #[test]
    fn encrypted_key_passphrase_is_checked() {
        let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).unwrap();
        let encrypted = key.encrypt(&mut rand::rng(), "bonne phrase").unwrap();
        let pem = encrypted.to_openssh(LineEnding::LF).unwrap();
        validate_key(&identity_with_key(&pem, Some("bonne phrase"))).unwrap();
        validate_key(&identity_with_key(&pem, None)).unwrap();
        assert!(validate_key(&identity_with_key(&pem, Some("mauvaise"))).is_err());
        assert!(key_info(&Secret::new(pem.as_str())).unwrap().encrypted);
    }

    #[test]
    fn garbage_key_rejected() {
        assert!(validate_key(&identity_with_key("pas une clé", None)).is_err());
    }

    #[test]
    fn secret_field_semantics() {
        let current = Some(Secret::new("old"));
        assert_eq!(SecretField::Keep.apply(current.clone()).unwrap().expose(), "old");
        assert!(SecretField::Clear.apply(current.clone()).is_none());
        assert!(SecretField::Set(Secret::new("")).apply(current.clone()).is_none());
        assert_eq!(SecretField::Set(Secret::new("new")).apply(current).unwrap().expose(), "new");
    }

    #[test]
    fn field_validation() {
        assert!(required("  ", "Le nom").is_err());
        assert!(required("a\u{7}b", "Le nom").is_err());
        assert_eq!(required("  prod ", "Le nom").unwrap(), "prod");
        assert_eq!(optional(Some("  ".into()), "x").unwrap(), None);
        assert!(is_hex_color("#7c8cff"));
        assert!(!is_hex_color("red"));
        assert!(!is_hex_color("#7c8cfg"));
    }
}
