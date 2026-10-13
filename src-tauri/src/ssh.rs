//! Sessions SSH interactives.
//!
//! Chaque session tourne dans sa propre tâche asynchrone. L'interface reçoit :
//! - le flux du terminal sur un canal binaire (`data`) ;
//! - les changements d'état et les demandes de confirmation/saisie sur un canal JSON (`events`).
//!
//! Elle renvoie les frappes, les redimensionnements et les réponses aux demandes
//! via des commandes. Les secrets restent côté Rust : les réponses saisies (mot de passe,
//! phrase de passe, code 2FA) entrent, rien ne ressort.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use russh::client::{self, KeyboardInteractiveAuthResponse};
use russh::keys::{HashAlg, PrivateKey, PrivateKeyWithHashAlg, PublicKey};
use russh::{ChannelMsg, Disconnect};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{AppHandle, Manager};
use vault::{KnownHost, Secret};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

use crate::i18n::tr;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Temps maximal laissé au serveur pour la négociation et l'authentification, sans
/// compter le temps passé par l'utilisateur à répondre (empreinte, mot de passe, 2FA).
const SERVER_DEADLINE: Duration = Duration::from_secs(45);
const MAX_PROMPT_ATTEMPTS: usize = 3;
const TERM: &str = "xterm-256color";

// ───────────────────────────── Événements vers l'interface ─────────────────────────────

#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SessionEvent {
    Status {
        state: &'static str,
    },
    /// Demande de confirmation d'une clé d'hôte inconnue ou modifiée.
    #[serde(rename_all = "camelCase")]
    HostKey {
        prompt_id: u64,
        changed: bool,
        hostname: String,
        port: u16,
        algorithm: String,
        fingerprint: String,
        previous_fingerprint: Option<String>,
    },
    /// Demande de saisie (utilisateur, mot de passe, phrase de passe, 2FA…).
    #[serde(rename_all = "camelCase")]
    Input {
        prompt_id: u64,
        title: String,
        message: String,
        fields: Vec<InputField>,
        /// Textes rédigés par le serveur (keyboard-interactive) : l'interface les
        /// présente comme tels, sous le nom de l'hôte, pour éviter qu'un serveur
        /// imite une fenêtre de l'application (« mot de passe maître »…).
        from_server: Option<String>,
    },
    Connected,
    /// Explorateur SFTP prêt ; `home` est le dossier de départ.
    SftpReady {
        home: String,
    },
    #[serde(rename_all = "camelCase")]
    Closed {
        error: Option<String>,
    },
}

#[derive(Clone, Serialize)]
pub struct InputField {
    label: String,
    secret: bool,
}

/// Réponse de l'interface : `None` = refus/annulation.
pub type PromptAnswer = Option<Vec<Secret>>;

// ───────────────────────────── Gestionnaire ─────────────────────────────

enum Control {
    Data(Vec<u8>),
    Resize(u32, u32),
    Close,
}

#[derive(Default)]
pub struct SshManager {
    sessions: Mutex<HashMap<Uuid, mpsc::UnboundedSender<Control>>>,
    prompts: Mutex<HashMap<u64, oneshot::Sender<PromptAnswer>>>,
    next_prompt: AtomicU64,
}

impl SshManager {
    fn send(&self, id: Uuid, msg: Control) -> CmdResult<()> {
        let sessions = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
        let tx = sessions.get(&id).ok_or_else(|| CmdError::new("no_session", tr!("err.ssh.sessionClosed")))?;
        tx.send(msg).map_err(|_| CmdError::new("no_session", tr!("err.ssh.sessionClosed")))
    }

    pub fn write(&self, id: Uuid, data: Vec<u8>) -> CmdResult<()> {
        self.send(id, Control::Data(data))
    }

    pub fn resize(&self, id: Uuid, cols: u32, rows: u32) -> CmdResult<()> {
        self.send(id, Control::Resize(cols.clamp(1, 1000), rows.clamp(1, 1000)))
    }

    pub fn close(&self, id: Uuid) {
        let _ = self.send(id, Control::Close);
    }

    /// Ferme toutes les sessions et annule les demandes en attente (verrouillage).
    pub fn close_all(&self) {
        let sessions = std::mem::take(&mut *self.sessions.lock().unwrap_or_else(|p| p.into_inner()));
        for tx in sessions.values() {
            let _ = tx.send(Control::Close);
        }
        self.prompts.lock().unwrap_or_else(|p| p.into_inner()).clear();
    }

    pub fn answer(&self, prompt_id: u64, answer: PromptAnswer) {
        if let Some(tx) = self.prompts.lock().unwrap_or_else(|p| p.into_inner()).remove(&prompt_id) {
            let _ = tx.send(answer);
        }
    }

    fn register_prompt(&self) -> (u64, oneshot::Receiver<PromptAnswer>) {
        let id = self.next_prompt.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = oneshot::channel();
        self.prompts.lock().unwrap_or_else(|p| p.into_inner()).insert(id, tx);
        (id, rx)
    }

    fn remove_session(&self, id: Uuid) {
        self.sessions.lock().unwrap_or_else(|p| p.into_inner()).remove(&id);
    }
}

// ───────────────────────────── Ouverture d'une session ─────────────────────────────

/// Tout ce qu'il faut pour se connecter, copié depuis le coffre à l'ouverture.
pub(crate) struct Target {
    hostname: String,
    port: u16,
    username: Option<String>,
    password: Option<Secret>,
    private_key: Option<Secret>,
    key_passphrase: Option<Secret>,
}

impl Target {
    pub(crate) fn load(app: &AppHandle, host_id: Uuid) -> CmdResult<Self> {
        app.state::<AppState>().with_vault(|v| {
            let host = v.data.hosts.iter().find(|h| h.id == host_id).ok_or_else(CmdError::not_found)?;
            let identity = host.identity_id.and_then(|id| v.data.identities.iter().find(|i| i.id == id));
            Ok(Target {
                hostname: host.hostname.clone(),
                port: host.port,
                username: host.username.clone().or_else(|| identity.and_then(|i| i.username.clone())),
                password: identity.and_then(|i| i.password.clone()),
                private_key: identity.and_then(|i| i.private_key.clone()),
                key_passphrase: identity.and_then(|i| i.key_passphrase.clone()),
            })
        })
    }
}

pub fn open(
    app: &AppHandle,
    host_id: Uuid,
    cols: u32,
    rows: u32,
    events: Channel<SessionEvent>,
    data: Channel<InvokeResponseBody>,
) -> CmdResult<Uuid> {
    let target = Target::load(app, host_id)?;
    let id = Uuid::new_v4();
    let (tx, rx) = mpsc::unbounded_channel();
    app.state::<SshManager>().sessions.lock().unwrap_or_else(|p| p.into_inner()).insert(id, tx);

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let asker = Asker::new(&app, &events);
        let error = run(asker, target, cols, rows, rx, &events, &data).await.err();
        app.state::<SshManager>().remove_session(id);
        let _ = events.send(SessionEvent::Closed { error });
    });
    Ok(id)
}

/// Pose des questions à l'utilisateur via l'interface et attend la réponse.
#[derive(Clone)]
pub(crate) struct Asker {
    app: AppHandle,
    events: Channel<SessionEvent>,
    /// Nombre de questions en attente de réponse : pendant ce temps, le délai laissé
    /// au serveur ne s'écoule pas.
    waiting: Arc<AtomicUsize>,
}

/// Décrémente le compteur de questions en attente, même si la tâche est annulée.
struct Waiting<'a>(&'a AtomicUsize);

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Asker {
    pub(crate) fn new(app: &AppHandle, events: &Channel<SessionEvent>) -> Self {
        Self { app: app.clone(), events: events.clone(), waiting: Arc::new(AtomicUsize::new(0)) }
    }

    async fn ask(&self, build: impl FnOnce(u64) -> SessionEvent) -> PromptAnswer {
        let (id, rx) = self.app.state::<SshManager>().register_prompt();
        self.waiting.fetch_add(1, Ordering::AcqRel);
        let _waiting = Waiting(&self.waiting);
        if self.events.send(build(id)).is_err() {
            return None;
        }
        rx.await.unwrap_or(None)
    }

    fn is_waiting(&self) -> bool {
        self.waiting.load(Ordering::Acquire) > 0
    }

    async fn input(&self, title: &str, message: &str, fields: Vec<InputField>) -> Option<Vec<Secret>> {
        self.ask_input(title, message, fields, None).await
    }

    async fn ask_input(
        &self,
        title: &str,
        message: &str,
        fields: Vec<InputField>,
        from_server: Option<String>,
    ) -> Option<Vec<Secret>> {
        let answer = self
            .ask(|prompt_id| SessionEvent::Input {
                prompt_id,
                title: title.to_string(),
                message: message.to_string(),
                fields: fields.clone(),
                from_server: from_server.clone(),
            })
            .await?;
        (answer.len() == fields.len()).then_some(answer)
    }

    async fn secret(&self, title: &str, message: &str, label: &str) -> Option<Secret> {
        self.input(title, message, vec![InputField { label: label.into(), secret: true }])
            .await
            .and_then(|mut v| v.pop())
    }
}

/// Algorithmes de russh par défaut (pas de SHA-1 ni de CBC pour l'échange de clés, le
/// chiffrement et l'intégrité), sans `ssh-rsa` pour la clé d'hôte : signature SHA-1,
/// refusée par défaut par OpenSSH depuis la version 8.8. Les clés RSA restent
/// acceptées avec `rsa-sha2-256` / `rsa-sha2-512`.
fn preferred_algorithms() -> russh::Preferred {
    let key = russh::Preferred::DEFAULT
        .key
        .iter()
        .filter(|a| !matches!(a, russh::keys::Algorithm::Rsa { hash: None }))
        .cloned()
        .collect::<Vec<_>>();
    russh::Preferred { key: key.into(), ..russh::Preferred::DEFAULT }
}

/// Ouvre une connexion SSH authentifiée : TCP, vérification de la clé d'hôte, puis
/// authentification. `target` est consommé : ses secrets sont effacés de la mémoire
/// (Secret: ZeroizeOnDrop) dès la fin de l'authentification.
pub(crate) async fn establish(asker: &Asker, target: Target) -> Result<client::Handle<HostKeyVerifier>, String> {
    let events = &asker.events;
    let _ = events.send(SessionEvent::Status { state: "connecting" });

    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        nodelay: true,
        preferred: preferred_algorithms(),
        ..Default::default()
    });
    // Le délai ne couvre que l'ouverture TCP : la vérification de l'empreinte par
    // l'utilisateur peut prendre le temps qu'il faut.
    let stream = match tokio::time::timeout(
        CONNECT_TIMEOUT,
        tokio::net::TcpStream::connect((target.hostname.as_str(), target.port)),
    )
    .await
    {
        Err(_) => return Err(tr!("err.ssh.timeout", host = target.hostname, port = target.port)),
        Ok(Err(e)) => return Err(describe_io_error(&e, &target)),
        Ok(Ok(stream)) => stream,
    };
    let _ = stream.set_nodelay(true);

    let rejection = Arc::new(Mutex::new(None));
    let handler = HostKeyVerifier {
        asker: asker.clone(),
        hostname: target.hostname.clone(),
        port: target.port,
        rejection: rejection.clone(),
    };
    let stalled = tr!("err.ssh.stalled", host = target.hostname, port = target.port);
    let handshake_and_auth = async {
        let mut session = match client::connect_stream(config, stream, handler).await {
            Ok(s) => s,
            Err(russh::Error::NoCommonAlgo { kind: russh::AlgorithmKind::Key, theirs, .. })
                if theirs.iter().any(|a| a == "ssh-rsa") =>
            {
                return Err(tr!("err.ssh.legacyHostKey", host = target.hostname, port = target.port));
            }
            Err(e) => {
                let reason = rejection.lock().unwrap_or_else(|p| p.into_inner()).take();
                return Err(reason.unwrap_or_else(|| {
                    tr!("err.ssh.handshake", host = target.hostname, port = target.port, detail = e)
                }));
            }
        };
        let _ = events.send(SessionEvent::Status { state: "authenticating" });
        authenticate(&mut session, asker, &target).await?;
        Ok(session)
    };
    let session = server_deadline(|| asker.is_waiting(), SERVER_DEADLINE, handshake_and_auth, stalled).await?;
    drop(target);
    Ok(session)
}

/// Exécute `work` en lui laissant au plus `limit` de temps serveur : le décompte est
/// suspendu tant qu'une question attend la réponse de l'utilisateur. Un serveur qui
/// bloque la négociation ne retient donc pas la connexion indéfiniment.
async fn server_deadline<T>(
    user_is_answering: impl Fn() -> bool,
    limit: Duration,
    work: impl std::future::Future<Output = Result<T, String>>,
    stalled: String,
) -> Result<T, String> {
    const TICK: Duration = Duration::from_millis(250);
    tokio::pin!(work);
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut used = Duration::ZERO;
    loop {
        tokio::select! {
            result = &mut work => return result,
            _ = ticker.tick() => {
                if !user_is_answering() {
                    used += TICK;
                    if used >= limit {
                        return Err(stalled);
                    }
                }
            }
        }
    }
}


async fn run(
    asker: Asker,
    target: Target,
    cols: u32,
    rows: u32,
    mut control: mpsc::UnboundedReceiver<Control>,
    events: &Channel<SessionEvent>,
    data: &Channel<InvokeResponseBody>,
) -> Result<(), String> {
    let session = establish(&asker, target).await?;
    let channel = session.channel_open_session().await.map_err(|e| tr!("err.ssh.channel", detail = e))?;
    channel
        .request_pty(false, TERM, cols, rows, 0, 0, &[])
        .await
        .map_err(|e| tr!("err.ssh.pty", detail = e))?;
    channel.request_shell(false).await.map_err(|e| tr!("err.ssh.shell", detail = e))?;
    let (mut reader, writer) = channel.split();
    let _ = events.send(SessionEvent::Connected);

    loop {
        tokio::select! {
            msg = control.recv() => match msg {
                Some(Control::Data(bytes)) => {
                    if writer.data_bytes(bytes).await.is_err() { break; }
                }
                Some(Control::Resize(c, r)) => {
                    let _ = writer.window_change(c, r, 0, 0).await;
                }
                Some(Control::Close) | None => {
                    let _ = writer.close().await;
                    break;
                }
            },
            msg = reader.wait() => match msg {
                Some(ChannelMsg::Data { data: bytes }) | Some(ChannelMsg::ExtendedData { data: bytes, .. }) => {
                    let _ = data.send(InvokeResponseBody::Raw(bytes.to_vec()));
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => break,
                Some(_) => {}
            },
        }
    }

    let _ = session.disconnect(Disconnect::ByApplication, "", crate::i18n::language()).await;
    Ok(())
}

fn describe_io_error(e: &std::io::Error, target: &Target) -> String {
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => {
            tr!("err.ssh.refused", host = target.hostname, port = target.port)
        }
        _ if e.to_string().contains("failed to lookup address") || e.kind() == std::io::ErrorKind::NotFound => {
            tr!("err.ssh.unknownHost", host = target.hostname)
        }
        _ => tr!("err.ssh.connect", host = target.hostname, port = target.port, detail = e),
    }
}

// ───────────────────────────── Authentification ─────────────────────────────

async fn authenticate(
    session: &mut client::Handle<HostKeyVerifier>,
    asker: &Asker,
    target: &Target,
) -> Result<(), String> {
    let username = match &target.username {
        Some(u) => u.clone(),
        None => {
            let answer = asker
                .input(
                    &tr!("ssh.prompt.username"),
                    &tr!("ssh.prompt.whichAccount", host = target.hostname),
                    vec![InputField { label: tr!("ssh.prompt.user"), secret: false }],
                )
                .await
                .ok_or_else(|| tr!("err.ssh.cancelled"))?;
            let user = answer[0].expose().trim().to_string();
            if user.is_empty() {
                return Err(tr!("err.ssh.emptyUser"));
            }
            user
        }
    };
    let cancelled = || tr!("err.ssh.cancelled");
    let ssh_err = |e: russh::Error| tr!("err.ssh.auth", detail = e);

    // 1. Clé privée.
    if let Some(pem) = &target.private_key {
        if let Some(key) = load_key(pem, target.key_passphrase.as_ref(), asker).await? {
            let hash = session.best_supported_rsa_hash().await.map_err(ssh_err)?.flatten();
            let res = session
                .authenticate_publickey(&username, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
                .map_err(ssh_err)?;
            if res.success() {
                return Ok(());
            }
        }
    }

    // 2. Mot de passe enregistré.
    if let Some(password) = &target.password {
        let res = session.authenticate_password(&username, password.expose()).await.map_err(ssh_err)?;
        if res.success() {
            return Ok(());
        }
    }

    // 3. Keyboard-interactive (codes 2FA, PAM…).
    let mut stored_password_used = target.password.is_none();
    let mut reply = session.authenticate_keyboard_interactive_start(&username, None).await.map_err(ssh_err)?;
    for _ in 0..8 {
        match reply {
            KeyboardInteractiveAuthResponse::Success => return Ok(()),
            KeyboardInteractiveAuthResponse::Failure { .. } => break,
            KeyboardInteractiveAuthResponse::InfoRequest { name, instructions, prompts } => {
                let responses: Vec<String> = if prompts.is_empty() {
                    Vec::new()
                } else if !stored_password_used
                    && prompts.len() == 1
                    && !prompts[0].echo
                    && is_password_prompt(&prompts[0].prompt)
                {
                    stored_password_used = true;
                    vec![target.password.as_ref().map(|p| p.expose().to_string()).unwrap_or_default()]
                } else {
                    // Titre du serveur, vide s'il n'en donne pas (l'interface affiche alors
                    // le sien, traduit).
                    let title = server_text(&name, 80);
                    let message = server_text(&instructions, 600);
                    let fields = prompts
                        .iter()
                        .take(8)
                        .map(|p| InputField { label: server_text(&p.prompt, 80), secret: !p.echo })
                        .collect();
                    let host = format!("{}:{}", target.hostname, target.port);
                    let answers =
                        asker.ask_input(&title, &message, fields, Some(host)).await.ok_or_else(cancelled)?;
                    answers.iter().map(|s| s.expose().to_string()).collect()
                };
                reply = session.authenticate_keyboard_interactive_respond(responses).await.map_err(ssh_err)?;
            }
        }
    }

    // 4. Mot de passe saisi.
    for attempt in 0..MAX_PROMPT_ATTEMPTS {
        let message = if attempt == 0 {
            tr!("ssh.prompt.passwordFor", user = username, host = target.hostname)
        } else {
            tr!("ssh.prompt.passwordRetry")
        };
        let label = tr!("ssh.prompt.password");
        let password = asker.secret(&label, &message, &label).await.ok_or_else(cancelled)?;
        let res = session.authenticate_password(&username, password.expose()).await.map_err(ssh_err)?;
        if res.success() {
            return Ok(());
        }
    }
    Err(tr!("err.ssh.authRefused"))
}

fn is_password_prompt(prompt: &str) -> bool {
    let p = prompt.to_lowercase();
    p.contains("password") || p.contains("mot de passe")
}

/// Déchiffre la clé privée ; demande la phrase de passe si elle n'est pas enregistrée.
async fn load_key(pem: &Secret, stored: Option<&Secret>, asker: &Asker) -> Result<Option<PrivateKey>, String> {
    let attempt = |pass: Option<&str>| russh::keys::decode_secret_key(pem.expose(), pass);
    match attempt(stored.map(Secret::expose)) {
        Ok(key) => return Ok(Some(key)),
        Err(russh::keys::Error::KeyIsEncrypted) => {}
        Err(_) if stored.is_some() => {}
        Err(e) => return Err(tr!("err.ssh.keyUnusable", detail = e)),
    }
    for attempt_no in 0..MAX_PROMPT_ATTEMPTS {
        let message = if attempt_no == 0 {
            tr!("ssh.prompt.keyProtected")
        } else {
            tr!("ssh.prompt.passphraseRetry")
        };
        let label = tr!("ssh.prompt.passphrase");
        let Some(pass) = asker.secret(&label, &message, &label).await else {
            return Ok(None); // L'utilisateur passe : on essaie les autres méthodes.
        };
        if let Ok(key) = attempt(Some(pass.expose())) {
            return Ok(Some(key));
        }
    }
    Ok(None)
}

// ───────────────────────────── Vérification de la clé d'hôte ─────────────────────────────

pub(crate) struct HostKeyVerifier {
    asker: Asker,
    hostname: String,
    port: u16,
    /// Raison d'un refus, à afficher à l'utilisateur. Refuser via `Ok(false)` plutôt
    /// qu'une erreur : russh interrompt alors proprement la négociation.
    rejection: Arc<Mutex<Option<String>>>,
}

impl HostKeyVerifier {
    fn reject(&self, reason: impl Into<String>) -> Result<bool, russh::Error> {
        *self.rejection.lock().unwrap_or_else(|p| p.into_inner()) = Some(reason.into());
        Ok(false)
    }
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// Clé publique au format OpenSSH, sans commentaire (sert de référence de comparaison).
fn encode(key: &PublicKey) -> String {
    let mut key = key.clone();
    key.set_comment("");
    key.to_openssh().unwrap_or_default().trim().to_string()
}

impl client::Handler for HostKeyVerifier {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_key: &russh::keys::PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let key = match server_key {
            russh::keys::PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            russh::keys::PublicKeyOrCertificate::Certificate(cert) => PublicKey::from(cert.public_key().clone()),
        };
        let encoded = encode(&key);
        let fp = fingerprint(&key);
        let algorithm = key.algorithm().to_string();

        let state = self.asker.app.state::<AppState>();
        let known: Option<KnownHost> = match state.with_vault(|v| {
            Ok(v.data.known_hosts.iter().find(|k| k.hostname == self.hostname && k.port == self.port).cloned())
        }) {
            Ok(k) => k,
            Err(e) => return self.reject(e.message),
        };

        if let Some(k) = &known {
            if k.public_key == encoded {
                return Ok(true);
            }
        }

        let changed = known.is_some();
        let answer = self
            .asker
            .ask(|prompt_id| SessionEvent::HostKey {
                prompt_id,
                changed,
                hostname: self.hostname.clone(),
                port: self.port,
                algorithm: algorithm.clone(),
                fingerprint: fp.clone(),
                previous_fingerprint: known.as_ref().map(|k| k.fingerprint.clone()),
            })
            .await;
        // Remplacer une clé connue exige une réponse explicite, vérifiée ici et pas
        // seulement dans l'interface.
        let approved = match &answer {
            Some(a) if changed => a.len() == 1 && a[0].expose() == REPLACE_KEY_ANSWER,
            Some(_) => true,
            None => false,
        };
        if !approved {
            let reason = if changed {
                tr!("err.ssh.hostKeyChanged")
            } else {
                tr!("err.ssh.hostKeyRejected")
            };
            return self.reject(reason);
        }

        let record = KnownHost {
            hostname: self.hostname.clone(),
            port: self.port,
            algorithm,
            fingerprint: fp,
            public_key: encoded,
            added_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        };
        let saved = state.mutate_vault(|v| {
            v.data.known_hosts.retain(|k| !(k.hostname == record.hostname && k.port == record.port));
            v.data.known_hosts.push(record);
            Ok(())
        });
        match saved {
            Ok(()) => Ok(true),
            Err(e) => self.reject(e.message),
        }
    }
}

/// Réponse attendue pour remplacer une clé d'hôte qui a changé.
const REPLACE_KEY_ANSWER: &str = "replace-host-key";

/// Texte fourni par le serveur, rendu inoffensif pour l'affichage (voir
/// [`crate::text::neutralize`]).
fn server_text(text: &str, max_chars: usize) -> String {
    crate::text::neutralize(text, max_chars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha1_host_keys_are_not_offered() {
        let key = preferred_algorithms().key;
        assert!(!key.iter().any(|a| matches!(a, russh::keys::Algorithm::Rsa { hash: None })));
        assert!(key.iter().any(|a| matches!(a, russh::keys::Algorithm::Rsa { hash: Some(_) })));
        assert!(key.contains(&russh::keys::Algorithm::Ed25519));
    }

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    #[test]
    fn deadline_lets_fast_work_finish() {
        let r = block_on(server_deadline(|| false, Duration::from_millis(500), async { Ok::<_, String>(1) }, "lent".into()));
        assert_eq!(r, Ok(1));
    }

    #[test]
    fn deadline_stops_a_stalled_server() {
        let start = std::time::Instant::now();
        let r = block_on(server_deadline(
            || false,
            Duration::from_millis(500),
            std::future::pending::<Result<(), String>>(),
            "lent".into(),
        ));
        assert_eq!(r, Err("lent".to_string()));
        assert!(start.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn deadline_ignores_time_spent_answering() {
        // L'utilisateur répond pendant 1,5 s, bien au-delà de la limite de 0,5 s.
        let r = block_on(server_deadline(
            || true,
            Duration::from_millis(500),
            async {
                tokio::time::sleep(Duration::from_millis(1500)).await;
                Ok::<_, String>("connecté")
            },
            "lent".into(),
        ));
        assert_eq!(r, Ok("connecté"));
    }

    #[test]
    fn server_text_is_neutralized() {
        assert_eq!(server_text("  Code\x1b[31m OTP\u{202E}fdp.exe ", 80), "Code[31m OTPfdp.exe");
        assert_eq!(server_text("ligne 1\nligne 2", 80), "ligne 1\nligne 2");
        assert_eq!(server_text(&"x".repeat(500), 80).len(), 80);
    }

    #[test]
    fn password_prompts_detected() {
        assert!(is_password_prompt("Password: "));
        assert!(is_password_prompt("demo@host's password:"));
        assert!(is_password_prompt("Mot de passe : "));
        assert!(!is_password_prompt("Verification code: "));
    }

    #[test]
    fn host_key_encoding_ignores_comment() {
        let line = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICP2n5c8rDkpV0QdW8bTADypwf22u6srDZo7f9EiUUrR test";
        let mut a = PublicKey::from_openssh(line).unwrap();
        let mut b = a.clone();
        a.set_comment("root@serveur");
        b.set_comment("");
        assert_eq!(encode(&a), encode(&b));
        assert_eq!(fingerprint(&a), "SHA256:HypGnLYMiCcC715etqbiLql6Z5m7nD1PeWxfxsbN5rQ");
    }
}
