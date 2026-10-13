//! Déverrouillage par un fournisseur d'identité OpenID Connect (Pocket ID, Authentik,
//! Authelia, Keycloak, Microsoft Entra ID, Google, ou tout fournisseur conforme).
//!
//! OIDC prouve une identité mais ne fournit aucun secret capable de déchiffrer le coffre.
//! Le schéma retenu (voir docs/securite.md) :
//!
//! 1. Activation (coffre déverrouillé, mot de passe maître ressaisi) : authentification
//!    auprès du fournisseur, puis une clé aléatoire `K` emballe la DEK dans un slot dédié.
//!    `K` est rangée dans le trousseau système (Secret Service, Trousseau macOS).
//!    Le compte autorisé (instance, client, `sub`, groupe, réglages) est lié
//!    cryptographiquement au slot (données authentifiées).
//! 2. Déverrouillage : authentification (Authorization Code + PKCE S256, `state`,
//!    `nonce`, signature du jeton vérifiée avec les clés publiques de l'instance),
//!    contrôle du `sub` et du groupe, puis lecture de `K` et ouverture.
//!    Variante depuis un téléphone (Device Authorization Grant, RFC 8628) : un QR code
//!    mène à la page de validation de l'instance, Quayline interroge l'instance jusqu'à
//!    la validation ; mêmes contrôles du jeton, sans port local.
//!
//! Le mot de passe maître reste la racine et le moyen de secours.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, State};
use vault::{OidcBinding, Secret, Vault};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use url::Url;
use zeroize::Zeroizing;

use crate::error::{CmdError, CmdResult};
use crate::i18n::tr;
use crate::platform::{keyring, SecretRef};
use crate::state::AppState;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
/// Scopes des liaisons Pocket ID sans scopes enregistrés (créées avant les fournisseurs
/// génériques, ou avec les réglages par défaut). Le minimum utile : l'identifiant du
/// compte (`openid`), un nom à afficher (`profile`) et les groupes ; pas d'e-mail.
const LEGACY_SCOPES: &str = "openid profile groups";
const DEFAULT_GROUPS_CLAIM: &str = "groups";

/// Modèles de fournisseurs connus : identifiant, nom affiché, et origines admises pour la
/// page de validation de la connexion par téléphone, en plus de celle de l'instance.
const PROVIDERS: &[(&str, &str, &[&str])] = &[
    ("pocket-id", "Pocket ID", &[]),
    ("authentik", "Authentik", &[]),
    ("authelia", "Authelia", &[]),
    ("keycloak", "Keycloak", &[]),
    (
        "entra",
        "Microsoft Entra ID",
        &[
            "https://microsoft.com",
            "https://www.microsoft.com",
            "https://login.microsoft.com",
            "https://login.microsoftonline.com",
        ],
    ),
    ("google", "Google", &["https://www.google.com", "https://google.com"]),
    ("generic", "", &[]),
];

/// Liaison sans modèle : créée avant les fournisseurs génériques, donc Pocket ID.
fn provider(id: Option<&str>) -> (&'static str, &'static str, &'static [&'static str]) {
    let id = id.unwrap_or("pocket-id");
    PROVIDERS.iter().copied().find(|(p, _, _)| *p == id).unwrap_or(PROVIDERS[PROVIDERS.len() - 1])
}

/// Nom affiché du fournisseur, traduit pour le modèle générique.
fn provider_name(id: Option<&str>) -> String {
    match provider(id) {
        ("generic", _, _) => tr!("oidc.genericName"),
        (_, name, _) => name.into(),
    }
}

/// Réglages du client OIDC, à l'activation comme au déverrouillage.
struct ClientConfig {
    provider: Option<String>,
    issuer: String,
    client_id: String,
    client_secret: Option<Zeroizing<String>>,
    redirect_port: u16,
    scopes: String,
    groups_claim: String,
}

impl ClientConfig {
    fn name(&self) -> String {
        provider_name(self.provider.as_deref())
    }
}

impl From<&OidcBinding> for ClientConfig {
    fn from(b: &OidcBinding) -> Self {
        Self {
            provider: b.provider.clone(),
            issuer: b.issuer.clone(),
            client_id: b.client_id.clone(),
            client_secret: b.client_secret.clone().map(Zeroizing::new),
            redirect_port: b.redirect_port,
            scopes: b.scopes.clone().unwrap_or_else(|| LEGACY_SCOPES.into()),
            groups_claim: b.groups_claim.clone().unwrap_or_else(|| DEFAULT_GROUPS_CLAIM.into()),
        }
    }
}

/// Permet d'annuler une authentification en cours (bouton « Annuler »), et garde la
/// connexion par téléphone entre son lancement et son attente.
#[derive(Default)]
pub struct OidcState {
    cancel: Mutex<Option<oneshot::Sender<()>>>,
    device: Mutex<Option<DeviceFlow>>,
}

impl OidcState {
    fn set_cancel(&self, tx: oneshot::Sender<()>) {
        *self.cancel.lock().unwrap_or_else(|p| p.into_inner()) = Some(tx);
    }

    fn device(&self) -> std::sync::MutexGuard<'_, Option<DeviceFlow>> {
        self.device.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn oidc_err(message: impl Into<String>) -> CmdError {
    CmdError::new("oidc", message)
}

// ───────────────────────────── Configuration ─────────────────────────────

/// Valide et normalise l'adresse de l'instance : HTTPS obligatoire (HTTP toléré
/// uniquement en local, pour les tests), sans « / » final, ni requête ni fragment.
pub fn normalize_issuer(input: &str) -> CmdResult<String> {
    let url = Url::parse(input.trim()).map_err(|_| oidc_err(tr!("err.oidc.badIssuer")))?;
    check_endpoint(&url)?;
    if url.query().is_some() || url.fragment().is_some() || !url.username().is_empty() {
        return Err(oidc_err(tr!("err.oidc.issuerExtras")));
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

fn is_loopback(url: &Url) -> bool {
    matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
}

fn check_endpoint(url: &Url) -> CmdResult<()> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if is_loopback(url) => Ok(()),
        _ => Err(oidc_err(tr!("err.oidc.httpsOnly"))),
    }
}

pub fn redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/callback")
}

#[derive(Deserialize)]
struct Discovery {
    issuer: String,
    authorization_endpoint: String,
    token_endpoint: String,
    jwks_uri: String,
    /// Connexion depuis un téléphone (RFC 8628), si le fournisseur la propose.
    #[serde(default)]
    device_authorization_endpoint: Option<String>,
}

fn http_client() -> CmdResult<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent(format!("{}/{}", crate::identity::NAME, env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| oidc_err(tr!("err.oidc.httpClient", detail = e)))
}

async fn discover(http: &reqwest::Client, issuer: &str) -> CmdResult<Discovery> {
    let url = format!("{issuer}/.well-known/openid-configuration");
    let response = http
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| oidc_err(tr!("err.oidc.unreachableAt", url = url, detail = e)))?;
    let doc: Discovery = crate::http::json(response).await.map_err(|_| oidc_err(tr!("err.oidc.badDiscovery")))?;
    // L'issuer annoncé doit être exactement celui configuré (protection contre la substitution).
    if doc.issuer.trim_end_matches('/') != issuer {
        return Err(oidc_err(tr!("err.oidc.issuerMismatch", announced = doc.issuer, issuer = issuer)));
    }
    for endpoint in [&doc.authorization_endpoint, &doc.token_endpoint, &doc.jwks_uri]
        .into_iter()
        .chain(doc.device_authorization_endpoint.as_ref())
    {
        check_endpoint(&Url::parse(endpoint).map_err(|_| oidc_err(tr!("err.oidc.badEndpoint")))?)?;
    }
    Ok(doc)
}

// ───────────────────────────── Authentification ─────────────────────────────

/// Identité vérifiée renvoyée par le fournisseur.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub subject: String,
    pub name: Option<String>,
    pub email: Option<String>,
    pub groups: Vec<String>,
    /// Trop de groupes pour le jeton (Entra) : ils ne sont pas transmis.
    #[serde(skip)]
    groups_overage: bool,
    /// Connexion par navigateur dont le jeton ne dit pas quand l'utilisateur s'est
    /// authentifié (`auth_time` absent) : la réauthentification n'a pas pu être vérifiée.
    pub reauth_unverified: bool,
}

impl Identity {
    /// Nom affiché (jamais l'adresse e-mail : elle n'est pas utilisée).
    fn display(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.subject.clone())
    }
}

#[derive(Deserialize)]
struct IdClaims {
    sub: String,
    nonce: Option<String>,
    /// Heure de l'authentification de l'utilisateur chez le fournisseur.
    auth_time: Option<u64>,
    name: Option<String>,
    preferred_username: Option<String>,
    email: Option<String>,
    /// Le reste du jeton : les groupes y sont lus selon la donnée configurée.
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

impl IdClaims {
    fn into_identity(self, groups_claim: &str) -> Identity {
        let groups = claim_strings(&self.extra, groups_claim);
        // Entra remplace les groupes par un renvoi vers Graph au-delà d'un certain nombre.
        let groups_overage = groups.is_empty()
            && self.extra.get("_claim_names").and_then(|n| n.get(groups_claim)).is_some();
        Identity {
            subject: self.sub,
            name: self.name.or(self.preferred_username),
            email: self.email,
            groups,
            groups_overage,
            reauth_unverified: false,
        }
    }
}

/// Valeurs textuelles d'une donnée du jeton : nom exact (« groups »,
/// « http://schemas…/groups »), sinon chemin pointé (« realm_access.roles »).
/// Une liste ou une simple chaîne sont acceptées.
fn claim_strings(claims: &serde_json::Map<String, serde_json::Value>, name: &str) -> Vec<String> {
    let value = claims.get(name).or_else(|| {
        let mut parts = name.split('.');
        let first = claims.get(parts.next()?)?;
        parts.try_fold(first, |v, part| v.get(part))
    });
    match value {
        Some(serde_json::Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    id_token: String,
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    B64URL.encode(bytes)
}

/// Onglet du navigateur laissé en attente après le retour du fournisseur : la page finale
/// n'est envoyée qu'une fois le déverrouillage (ou l'activation) réellement abouti, pour
/// ne jamais annoncer un succès que l'application refuse ensuite (autre compte, groupe…).
struct BrowserReply {
    stream: tokio::net::TcpStream,
    app: AppHandle,
}

impl BrowserReply {
    async fn finish<T>(self, result: &CmdResult<T>, success: &str) {
        self.send(result.as_ref().err(), success).await;
    }

    async fn send(mut self, error: Option<&CmdError>, success: &str) {
        let body = match error {
            None => page(true, &tr!("oidc.page.success"), &escape(success)),
            Some(e) => page(false, &tr!("oidc.page.denied"), &escape(&e.message)),
        };
        let _ = self.stream.write_all(body.as_bytes()).await;
        crate::bring_to_front(&self.app);
    }
}

/// Déroule le flux complet : navigateur → retour local → échange du code → vérification.
/// L'onglet reste en attente : l'appelant y répond avec [`BrowserReply::finish`].
async fn authenticate(app: &AppHandle, cfg: &ClientConfig) -> CmdResult<(Identity, BrowserReply)> {
    let http = http_client()?;
    let doc = discover(&http, &cfg.issuer).await?;
    let port = cfg.redirect_port;

    let verifier = Zeroizing::new(random_token());
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_token();
    let nonce = random_token();
    let redirect = redirect_uri(port);
    // Délai de grâce (réglage) : dans ce délai après la dernière connexion par le
    // navigateur, la session du fournisseur suffit ; sinon, nouvelle connexion.
    let grace = app.state::<AppState>().oidc_grace_window();
    let force_login = grace.is_none();
    let max_age = grace.map_or(MAX_AUTH_AGE, |g| g.as_secs());

    let listener = TcpListener::bind(("127.0.0.1", port)).await.map_err(|e| {
        oidc_err(tr!("err.oidc.portUnavailable", port = port, detail = e))
    })?;

    let mut auth_url = Url::parse(&doc.authorization_endpoint).map_err(|_| oidc_err(tr!("err.oidc.badAuthUrl")))?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &cfg.client_id)
        .append_pair("redirect_uri", &redirect)
        .append_pair("scope", &cfg.scopes)
        .append_pair("state", &state)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        // Âge maximal de l'authentification chez le fournisseur (OpenID Connect Core
        // §3.1.2.1), vérifié ensuite avec `auth_time` (fresh_login).
        .append_pair("max_age", &max_age.to_string());
    // Hors délai de grâce, nouvelle authentification exigée : une session déjà ouverte
    // dans le navigateur ne suffit pas. Google refuse les valeurs de `prompt` autres
    // que none, consent et select_account (seul `max_age` s'applique alors).
    if force_login && cfg.provider.as_deref() != Some("google") {
        auth_url.query_pairs_mut().append_pair("prompt", "login");
    }

    let (cancel_tx, cancel_rx) = oneshot::channel();
    app.state::<OidcState>().set_cancel(cancel_tx);

    let url = auth_url.to_string();
    tauri::async_runtime::spawn_blocking(move || open::that_detached(url))
        .await
        .ok()
        .and_then(|r| r.ok())
        .ok_or_else(|| oidc_err(tr!("err.oidc.browser")))?;

    let name = cfg.name();
    let (code, stream) = tokio::select! {
        r = tokio::time::timeout(LOGIN_TIMEOUT, wait_for_callback(&listener, &state, &name)) => {
            r.map_err(|_| oidc_err(tr!("err.oidc.timeout", provider = name)))??
        }
        _ = cancel_rx => return Err(CmdError::new("cancelled", tr!("err.oidc.cancelled"))),
    };
    drop(listener);
    let reply = BrowserReply { stream, app: app.clone() };

    let checked = exchange_code(&http, &doc, cfg, &code, &redirect, &verifier, &nonce)
        .await
        .and_then(|(identity, auth_time)| {
            let reauth_unverified = fresh_login(auth_time, now_secs(), max_age, &name)?;
            Ok(Identity { reauth_unverified, ..identity })
        });
    match checked {
        Ok(identity) => {
            app.state::<AppState>().record_oidc_login();
            Ok((identity, reply))
        }
        Err(e) => {
            reply.send(Some(&e), "").await;
            Err(e)
        }
    }
}

/// Échange le code contre le jeton d'identité, puis le vérifie. Renvoie aussi l'heure
/// de l'authentification chez le fournisseur (`auth_time`), contrôlée par l'appelant.
async fn exchange_code(
    http: &reqwest::Client,
    doc: &Discovery,
    cfg: &ClientConfig,
    code: &str,
    redirect: &str,
    verifier: &str,
    nonce: &str,
) -> CmdResult<(Identity, Option<u64>)> {
    let mut form = vec![
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect),
        ("client_id", cfg.client_id.as_str()),
        ("code_verifier", verifier),
    ];
    if let Some(secret) = &cfg.client_secret {
        form.push(("client_secret", secret.as_str()));
    }
    let response = http
        .post(&doc.token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| oidc_err(tr!("err.oidc.codeExchange", detail = e)))?;
    if !response.status().is_success() {
        let detail = crate::http::json::<TokenError>(response).await.ok().map(|e| e.error_description.unwrap_or(e.error));
        return Err(oidc_err(tr!(
            "err.oidc.codeExchangeRefused",
            provider = cfg.name(),
            detail = detail.map(|d| format!(" ({d})")).unwrap_or_default()
        )));
    }
    let tokens: TokenResponse = crate::http::json(response).await.map_err(|_| oidc_err(tr!("err.oidc.badToken")))?;

    let claims = verify_id_token(http, doc, &cfg.client_id, &tokens.id_token).await?;
    if claims.nonce.as_deref() != Some(nonce) {
        return Err(oidc_err(tr!("err.oidc.nonceReplay")));
    }
    let auth_time = claims.auth_time;
    Ok((claims.into_identity(&cfg.groups_claim), auth_time))
}

/// Délai maximal depuis l'authentification chez le fournisseur quand une nouvelle
/// connexion est exigée : le temps de se connecter. Une marge d'une minute couvre en
/// plus les horloges décalées.
const MAX_AUTH_AGE: u64 = 300;

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Vérifie que l'utilisateur s'est authentifié il y a moins de `max_age` secondes. `Ok(true)` si le fournisseur ne
/// donne pas l'heure (`auth_time` absent) : accepté, mais signalé à l'activation.
fn fresh_login(auth_time: Option<u64>, now: u64, max_age: u64, provider: &str) -> CmdResult<bool> {
    let Some(auth_time) = auth_time else { return Ok(true) };
    let age = now.saturating_sub(auth_time);
    if age > max_age + 60 {
        return Err(oidc_err(tr!("err.oidc.staleLogin", provider = provider, minutes = age / 60)));
    }
    Ok(false)
}

// ───────────────────────── Connexion depuis un téléphone ─────────────────────────

#[derive(Deserialize)]
struct DeviceAuthorization {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    expires_in: u64,
    interval: Option<u64>,
}

/// Connexion par téléphone lancée, en attente de validation.
struct DeviceFlow {
    binding: OidcBinding,
    cfg: ClientConfig,
    doc: Discovery,
    device_code: Zeroizing<String>,
    nonce: String,
    interval: Duration,
    deadline: Instant,
}

/// QR code sous forme de matrice (dessinée par l'interface, sans image ni HTML généré).
#[derive(Serialize)]
pub struct QrMatrix {
    size: usize,
    /// Modules ligne par ligne : `true` pour un module sombre.
    modules: Vec<bool>,
}

fn qr_matrix(data: &str) -> CmdResult<QrMatrix> {
    let code = qrcode::QrCode::new(data.as_bytes()).map_err(|_| oidc_err(tr!("err.oidc.qr")))?;
    Ok(QrMatrix {
        size: code.width(),
        modules: code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect(),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStart {
    /// Code à comparer avec celui affiché sur le téléphone (protection contre
    /// l'hameçonnage par code).
    user_code: String,
    /// Adresse de la page de validation, pour la saisir à la main.
    verification_uri: String,
    qr: QrMatrix,
    expires_in: u64,
}

/// La page de validation doit appartenir à l'instance configurée (ou à un domaine connu
/// du fournisseur, comme microsoft.com pour Entra) : un QR code ne doit jamais envoyer
/// le téléphone ailleurs.
fn check_verification_uri(uri: &str, cfg: &ClientConfig) -> CmdResult<Url> {
    let url = Url::parse(uri).map_err(|_| oidc_err(tr!("err.oidc.badVerification")))?;
    check_endpoint(&url)?;
    let issuer = Url::parse(&cfg.issuer).map_err(|_| oidc_err(tr!("err.oidc.badIssuer")))?;
    let known = provider(cfg.provider.as_deref()).2;
    let allowed = url.origin() == issuer.origin()
        || known.iter().any(|o| Url::parse(o).is_ok_and(|o| o.origin() == url.origin()));
    if !allowed {
        return Err(oidc_err(tr!("err.oidc.foreignVerification", provider = cfg.name())));
    }
    Ok(url)
}

/// Code court affiché : quelques caractères lisibles, rien d'autre.
fn check_user_code(code: &str) -> CmdResult<()> {
    if code.is_empty() || code.len() > 32 || !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(oidc_err(tr!("err.oidc.badUserCode")));
    }
    Ok(())
}

#[derive(Deserialize)]
struct TokenError {
    error: String,
    error_description: Option<String>,
}

/// Interroge l'instance jusqu'à la validation sur le téléphone (RFC 8628 §3.4–3.5).
async fn poll_device(app: &AppHandle, flow: &mut DeviceFlow) -> CmdResult<Identity> {
    let http = http_client()?;
    let (cancel_tx, mut cancel_rx) = oneshot::channel();
    app.state::<OidcState>().set_cancel(cancel_tx);
    loop {
        tokio::select! {
            _ = tokio::time::sleep(flow.interval) => {}
            _ = &mut cancel_rx => return Err(CmdError::new("cancelled", tr!("err.oidc.cancelled"))),
        }
        if Instant::now() >= flow.deadline {
            return Err(oidc_err(tr!("err.oidc.expired")));
        }
        let mut form = vec![
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", flow.device_code.as_str()),
            ("client_id", flow.cfg.client_id.as_str()),
        ];
        if let Some(secret) = &flow.cfg.client_secret {
            form.push(("client_secret", secret.as_str()));
        }
        let response = http.post(&flow.doc.token_endpoint).form(&form).send().await;
        // Coupure réseau passagère : on réessaie au prochain intervalle.
        let Ok(response) = response else { continue };
        if response.status().is_success() {
            let tokens: TokenResponse = crate::http::json(response).await.map_err(|_| oidc_err(tr!("err.oidc.badToken")))?;
            let claims = verify_id_token(&http, &flow.doc, &flow.cfg.client_id, &tokens.id_token).await?;
            // Le jeton vient directement de l'instance, en échange du code de l'appareil :
            // le nonce n'est pas exigé (RFC 8628), mais vérifié s'il est renvoyé.
            if claims.nonce.as_deref().is_some_and(|n| n != flow.nonce) {
                return Err(oidc_err(tr!("err.oidc.nonce")));
            }
            return Ok(claims.into_identity(&flow.cfg.groups_claim));
        }
        let error: TokenError = crate::http::json(response).await.map_err(|_| oidc_err(tr!("err.oidc.badReply")))?;
        match error.error.as_str() {
            "authorization_pending" => {}
            "slow_down" => flow.interval += Duration::from_secs(5),
            "access_denied" => return Err(oidc_err(tr!("err.oidc.phoneDenied"))),
            "expired_token" => return Err(oidc_err(tr!("err.oidc.expired"))),
            other => {
                return Err(oidc_err(tr!(
                    "err.oidc.refusedBy",
                    provider = flow.cfg.name(),
                    detail = error.error_description.as_deref().unwrap_or(other)
                )))
            }
        }
    }
}

async fn verify_id_token(
    http: &reqwest::Client,
    doc: &Discovery,
    client_id: &str,
    token: &str,
) -> CmdResult<IdClaims> {
    let header = jsonwebtoken::decode_header(token).map_err(|_| oidc_err(tr!("err.oidc.unreadableToken")))?;
    // Jamais de « none » ni de HMAC (clé symétrique) : signatures asymétriques uniquement.
    const ALLOWED: [Algorithm; 9] = [
        Algorithm::RS256,
        Algorithm::RS384,
        Algorithm::RS512,
        Algorithm::PS256,
        Algorithm::PS384,
        Algorithm::PS512,
        Algorithm::ES256,
        Algorithm::ES384,
        Algorithm::EdDSA,
    ];
    if !ALLOWED.contains(&header.alg) {
        return Err(oidc_err(tr!("err.oidc.algorithm", alg = format!("{:?}", header.alg))));
    }
    let response = http
        .get(&doc.jwks_uri)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| oidc_err(tr!("err.oidc.jwksUnreachable", detail = e)))?;
    let jwks: JwkSet = crate::http::json(response).await.map_err(|_| oidc_err(tr!("err.oidc.badJwks")))?;
    let jwk = match &header.kid {
        Some(kid) => jwks.find(kid),
        None if jwks.keys.len() == 1 => jwks.keys.first(),
        None => None,
    }
    .ok_or_else(|| oidc_err(tr!("err.oidc.keyNotFound")))?;
    let key = DecodingKey::from_jwk(jwk).map_err(|_| oidc_err(tr!("err.oidc.badKey")))?;

    let mut validation = Validation::new(header.alg);
    // L'issuer tel qu'annoncé (déjà comparé à celui configuré) : certains fournisseurs,
    // comme Authentik, le terminent par « / », et le jeton le reprend à l'identique.
    validation.set_issuer(&[doc.issuer.as_str()]);
    validation.set_audience(&[client_id]);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.leeway = 60;
    jsonwebtoken::decode::<IdClaims>(token, &key, &validation)
        .map(|d| d.claims)
        .map_err(|e| oidc_err(tr!("err.oidc.tokenRejected", detail = e)))
}


/// Logo de l'application (icons/app-icon.svg), en ligne : la page n'a droit à aucune
/// ressource externe.
const LOGO: &str = "<svg viewBox=\"0 0 64 64\" width=\"56\" height=\"56\" aria-hidden=\"true\"><defs>\
<linearGradient id=\"g\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\"><stop offset=\"0\" stop-color=\"#7c8cff\"/>\
<stop offset=\"1\" stop-color=\"#38d6c4\"/></linearGradient></defs>\
<rect x=\"2\" y=\"2\" width=\"60\" height=\"60\" rx=\"15\" fill=\"url(#g)\"/>\
<path d=\"M18 22l10 10-10 10\" fill=\"none\" stroke=\"#fff\" stroke-width=\"5\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>\
<path d=\"M33 43h14\" stroke=\"#fff\" stroke-width=\"5\" stroke-linecap=\"round\"/></svg>";

/// Page affichée dans le navigateur au retour du fournisseur, aux couleurs de l'application
/// (thème clair ou sombre selon le système). `title` et `message` doivent être échappés.
fn page(success: bool, title: &str, message: &str) -> String {
    let (mark, color) = if success { ("✓", "#3ddc97") } else { ("✕", "#ff6b81") };
    let html = format!(
        "<!doctype html><html lang=\"{lang}\"><meta charset=\"utf-8\">\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><title>{name}</title>\
         <style>:root{{color-scheme:light dark;--bg:#0d0f14;--card:#171a23;--border:#262b3a;--text:#e6e8ef;\
         --muted:#9097ab}}@media (prefers-color-scheme:light){{:root{{--bg:#f4f5f9;--card:#fff;--border:#e1e4ec;\
         --text:#1a1d27;--muted:#5b6275}}}}body{{font-family:system-ui,-apple-system,\"Segoe UI\",sans-serif;\
         background:var(--bg);color:var(--text);display:grid;place-items:center;min-height:100vh;margin:0;\
         padding:24px;box-sizing:border-box}}main{{max-width:420px;padding:36px 32px;text-align:center;\
         background:var(--card);border:1px solid var(--border);border-radius:18px}}\
         .brand{{display:flex;align-items:center;justify-content:center;gap:10px;font-weight:650;\
         margin-bottom:24px}}.mark{{display:inline-grid;place-items:center;width:44px;height:44px;\
         border-radius:50%;font-size:22px;font-weight:700;color:{color};background:{color}22;\
         margin-bottom:12px}}h1{{font-size:20px;margin:0 0 8px}}p{{color:var(--muted);margin:0;\
         line-height:1.5}}</style><main><div class=\"brand\">{LOGO}<span>{name}</span></div>\
         <div class=\"mark\">{mark}</div><h1>{title}</h1><p>{message}</p></main></html>",
        name = crate::identity::NAME,
        lang = crate::i18n::language()
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Security-Policy: default-src 'none'; \
         style-src 'unsafe-inline'\r\nCache-Control: no-store\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{html}",
        html.len()
    )
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// En-tête d'une requête HTTP, lu jusqu'à la ligne vide (il peut arriver en plusieurs
/// morceaux) ; `None` au-delà de 8 Kio ou si la connexion se ferme avant.
async fn read_request_head(stream: &mut tokio::net::TcpStream) -> Option<Vec<u8>> {
    const MAX: usize = 8192;
    let mut head = Vec::with_capacity(1024);
    let mut buf = [0u8; 1024];
    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buf).await.ok().filter(|&n| n > 0)?;
        head.extend_from_slice(&buf[..n]);
        if head.len() > MAX {
            return None;
        }
    }
    Some(head)
}

/// Attend la redirection du navigateur vers http://127.0.0.1:<port>/callback.
async fn wait_for_callback(
    listener: &TcpListener,
    expected_state: &str,
    provider_name: &str,
) -> CmdResult<(String, tokio::net::TcpStream)> {
    loop {
        let (mut stream, _) = listener.accept().await.map_err(|e| oidc_err(e.to_string()))?;
        let Ok(Some(head)) = tokio::time::timeout(Duration::from_secs(5), read_request_head(&mut stream)).await else {
            continue;
        };
        let request = String::from_utf8_lossy(&head);
        let target = request.lines().next().and_then(|l| l.strip_prefix("GET ")).and_then(|l| l.split(' ').next());
        let Some(url) = target.and_then(|t| Url::parse(&format!("http://127.0.0.1{t}")).ok()) else {
            continue;
        };
        if url.path() != "/callback" {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
            continue;
        }
        let params: HashMap<String, String> = url.query_pairs().into_owned().collect();
        if params.get("state").map(String::as_str) != Some(expected_state) {
            // Requête forgée ou ancienne : ignorée, on continue d'attendre la bonne.
            let refused = page(false, &tr!("oidc.page.requestRefused"), &tr!("oidc.page.requestRefusedHint"));
            let _ = stream.write_all(refused.as_bytes()).await;
            continue;
        }
        if let Some(error) = params.get("error") {
            let detail = params.get("error_description").unwrap_or(error);
            let _ = stream.write_all(page(false, &tr!("oidc.page.refused"), &escape(detail)).as_bytes()).await;
            return Err(oidc_err(tr!("err.oidc.authRefusedBy", provider = provider_name, detail = detail)));
        }
        if let Some(code) = params.get("code") {
            // Réponse différée : voir BrowserReply.
            return Ok((code.clone(), stream));
        }
    }
}

// ───────────────────────────── Trousseau système ─────────────────────────────

fn secret_ref(binding: &OidcBinding) -> SecretRef<'_> {
    SecretRef {
        // Nom historique, gardé pour retrouver les clés déjà rangées (tous fournisseurs).
        purpose: "pocket-id-unlock",
        fields: vec![("issuer", binding.issuer.as_str()), ("subject", binding.subject.as_str())],
    }
}

fn keyring_label(binding: &OidcBinding) -> String {
    let name = match binding.provider.as_deref() {
        Some("generic") => "OIDC",
        other => provider(other).1,
    };
    tr!("oidc.keyringLabel", app = crate::identity::NAME, provider = name)
}

async fn keyring_store(binding: &OidcBinding, key: &[u8; 32]) -> CmdResult<()> {
    keyring::store(&keyring_label(binding), &secret_ref(binding), key).await.map_err(oidc_err)
}

async fn keyring_load(binding: &OidcBinding) -> CmdResult<Zeroizing<[u8; 32]>> {
    let secret = keyring::load(&keyring_label(binding), &secret_ref(binding)).await.map_err(oidc_err)?.ok_or_else(|| {
        oidc_err(tr!("err.oidc.keyMissing"))
    })?;
    if secret.len() != 32 {
        return Err(oidc_err(tr!("err.oidc.keyCorrupt")));
    }
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&secret);
    Ok(key)
}

pub(crate) async fn keyring_delete(binding: &OidcBinding) {
    keyring::delete(&secret_ref(binding)).await;
}

// ───────────────────────────── Commandes ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OidcStatus {
    provider: String,
    provider_name: String,
    issuer: String,
    client_id: String,
    redirect_port: u16,
    redirect_uri: String,
    required_group: Option<String>,
    scopes: String,
    groups_claim: String,
    has_client_secret: bool,
    /// Connexion par téléphone proposée (Google l'exige d'un autre type de client).
    phone: bool,
    account: String,
}

impl From<&OidcBinding> for OidcStatus {
    fn from(b: &OidcBinding) -> Self {
        let cfg = ClientConfig::from(b);
        let (id, name, _) = provider(b.provider.as_deref());
        Self {
            provider: id.into(),
            provider_name: name.into(),
            issuer: b.issuer.clone(),
            client_id: b.client_id.clone(),
            redirect_port: b.redirect_port,
            redirect_uri: redirect_uri(b.redirect_port),
            required_group: b.required_group.clone(),
            scopes: cfg.scopes,
            groups_claim: cfg.groups_claim,
            has_client_secret: b.client_secret.is_some(),
            phone: id != "google",
            account: account_name(&b.display_name),
        }
    }
}

/// Nom du compte lié, sans l'adresse e-mail que les liaisons créées avant octobre 2026
/// enregistraient (« Nom <adresse> ») : elle n'est pas utilisée et n'a pas à s'afficher
/// sur l'écran de verrouillage.
fn account_name(display_name: &str) -> String {
    match display_name.rsplit_once(" <") {
        Some((name, rest)) if rest.ends_with('>') && rest.contains('@') && !name.trim().is_empty() => name.trim().into(),
        _ if display_name.contains('@') && !display_name.contains(' ') => tr!("oidc.linkedAccount"),
        _ => display_name.into(),
    }
}

/// Lisible sans déverrouiller : l'écran de verrouillage affiche le bouton de connexion.
#[tauri::command]
pub fn oidc_status(state: State<'_, AppState>) -> CmdResult<Option<OidcStatus>> {
    Ok(Vault::oidc_binding(&state.vault_path).map_err(CmdError::from)?.map(|(_, b)| OidcStatus::from(&b)))
}

#[tauri::command]
pub fn oidc_redirect_uri(port: u16) -> String {
    redirect_uri(port)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrollInput {
    #[serde(default)]
    provider: Option<String>,
    issuer: String,
    client_id: String,
    redirect_port: u16,
    required_group: Option<String>,
    #[serde(default)]
    scopes: Option<String>,
    #[serde(default)]
    groups_claim: Option<String>,
    #[serde(default)]
    client_secret: Option<Secret>,
}

/// Scopes séparés par des espaces, `openid` obligatoire (RFC 6749 §3.3 pour les caractères).
fn normalize_scopes(input: &str) -> CmdResult<String> {
    let scopes: Vec<&str> = input.split_whitespace().collect();
    if !scopes.contains(&"openid") {
        return Err(oidc_err(tr!("err.oidc.scopesOpenid")));
    }
    if scopes.iter().any(|s| !s.chars().all(|c| c.is_ascii_graphic() && c != '"' && c != '\\')) {
        return Err(oidc_err(tr!("err.oidc.badScopes")));
    }
    Ok(scopes.join(" "))
}

/// Valide les réglages saisis à l'activation.
fn config_from_input(input: EnrollInput) -> CmdResult<(ClientConfig, Option<String>)> {
    let provider_id = input.provider.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
    if let Some(id) = &provider_id {
        if !PROVIDERS.iter().any(|(p, _, _)| p == id) {
            return Err(oidc_err(tr!("err.oidc.unknownProvider")));
        }
    }
    let issuer = normalize_issuer(&input.issuer)?;
    let client_id = input.client_id.trim().to_string();
    if client_id.is_empty() || client_id.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(oidc_err(tr!("err.oidc.badClientId")));
    }
    if input.redirect_port < 1024 {
        return Err(oidc_err(tr!("err.oidc.badPort")));
    }
    let required_group = input.required_group.map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
    let scopes = match input.scopes.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => normalize_scopes(s)?,
        None => LEGACY_SCOPES.into(),
    };
    let groups_claim = input
        .groups_claim
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| DEFAULT_GROUPS_CLAIM.into());
    if groups_claim.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(oidc_err(tr!("err.oidc.badGroupsClaim")));
    }
    let client_secret = input
        .client_secret
        .map(|s| Zeroizing::new(s.expose().trim().to_string()))
        .filter(|s| !s.is_empty());
    if client_secret.as_ref().is_some_and(|s| s.chars().any(char::is_control)) {
        return Err(oidc_err(tr!("err.oidc.badSecret")));
    }
    if provider_id.as_deref() == Some("google") && client_secret.is_none() {
        return Err(oidc_err(tr!("err.oidc.googleSecret")));
    }
    let cfg = ClientConfig {
        provider: provider_id,
        issuer,
        client_id,
        client_secret,
        redirect_port: input.redirect_port,
        scopes,
        groups_claim,
    };
    Ok((cfg, required_group))
}

fn check_group(identity: &Identity, required: Option<&str>) -> CmdResult<()> {
    let Some(group) = required else { return Ok(()) };
    // Keycloak peut transmettre le chemin complet du groupe (« /admins »).
    if identity.groups.iter().any(|g| g == group || g.strip_prefix('/') == Some(group)) {
        return Ok(());
    }
    if identity.groups_overage {
        return Err(oidc_err(tr!("err.oidc.groupsOverage")));
    }
    Err(oidc_err(tr!("err.oidc.notInGroup", account = identity.display(), group = group)))
}

/// Active le déverrouillage par un fournisseur d'identité (mot de passe maître exigé).
#[tauri::command]
pub async fn oidc_enroll(app: AppHandle, input: EnrollInput, password: Secret) -> CmdResult<Identity> {
    let (cfg, required_group) = config_from_input(input)?;

    // Le mot de passe maître confirme que c'est bien le propriétaire qui active.
    crate::commands::confirm_master_password(&app, password).await?;

    let (identity, reply) = authenticate(&app, &cfg).await?;
    let result = bind_identity(&app, &cfg, required_group, &identity).await;
    let done = tr!("oidc.page.enrolled", provider = cfg.name());
    reply.finish(&result, &done).await;
    result.map(|()| identity)
}

/// Lie le coffre au compte authentifié : nouveau slot, clé rangée dans le trousseau.
async fn bind_identity(
    app: &AppHandle,
    cfg: &ClientConfig,
    required_group: Option<String>,
    identity: &Identity,
) -> CmdResult<()> {
    check_group(identity, required_group.as_deref())?;

    // Liaison sans les nouveaux champs pour Pocket ID avec ses réglages historiques :
    // même forme que les liaisons existantes.
    let legacy = matches!(cfg.provider.as_deref(), None | Some("pocket-id"))
        && cfg.scopes == LEGACY_SCOPES
        && cfg.groups_claim == DEFAULT_GROUPS_CLAIM
        && cfg.client_secret.is_none();
    let binding = OidcBinding {
        issuer: cfg.issuer.clone(),
        client_id: cfg.client_id.clone(),
        subject: identity.subject.clone(),
        redirect_port: cfg.redirect_port,
        required_group,
        display_name: identity.display(),
        provider: if legacy { None } else { cfg.provider.clone() },
        scopes: (!legacy).then(|| cfg.scopes.clone()),
        groups_claim: (!legacy).then(|| cfg.groups_claim.clone()),
        client_secret: cfg.client_secret.as_ref().map(|s| s.to_string()),
    };
    let mut key = Zeroizing::new([0u8; 32]);
    OsRng.fill_bytes(key.as_mut());

    // Ancienne liaison éventuelle : sa clé est retirée du trousseau.
    let previous = app.state::<AppState>().with_vault(|v| Ok(v.oidc().cloned()))?;
    keyring_store(&binding, &key).await?;
    let saved = app.state::<AppState>().with_vault_mut_nosave(|v| v.set_oidc(binding.clone(), &key).map_err(Into::into));
    if let Err(e) = saved {
        keyring_delete(&binding).await;
        return Err(e);
    }
    if let Some(old) = previous.filter(|old| old.subject != binding.subject || old.issuer != binding.issuer) {
        keyring_delete(&old).await;
    }
    Ok(())
}

#[tauri::command]
pub async fn oidc_disable(app: AppHandle) -> CmdResult<()> {
    let binding = app.state::<AppState>().with_vault(|v| Ok(v.oidc().cloned()))?;
    app.state::<AppState>().with_vault_mut_nosave(|v| v.remove_oidc().map_err(Into::into))?;
    if let Some(b) = binding {
        keyring_delete(&b).await;
    }
    Ok(())
}

/// Liaison lue dans le fichier du coffre, sans déverrouiller.
fn stored_binding(app: &AppHandle) -> CmdResult<OidcBinding> {
    let state = app.state::<AppState>();
    let (_, binding) = Vault::oidc_binding(&state.vault_path)?
        .ok_or_else(|| oidc_err(tr!("err.oidc.notEnabled")))?;
    // Pas encore authentifiée : si elle a été modifiée, le slot refusera de s'ouvrir
    // (données authentifiées), même avec la clé du trousseau.
    normalize_issuer(&binding.issuer)?;
    Ok(binding)
}

#[tauri::command]
pub async fn oidc_unlock(app: AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let _guard = state.begin_unlock()?;
    let binding = stored_binding(&app)?;
    let (identity, reply) = authenticate(&app, &ClientConfig::from(&binding)).await?;
    let result = finish_unlock(&app, binding, identity).await;
    let done = tr!("oidc.page.unlocked", app = crate::identity::NAME);
    reply.finish(&result, &done).await;
    result
}

/// Après authentification : contrôle du compte et du groupe, puis ouverture du coffre
/// avec la clé du trousseau système.
async fn finish_unlock(app: &AppHandle, binding: OidcBinding, identity: Identity) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let name = provider_name(binding.provider.as_deref());
    if identity.subject != binding.subject {
        return Err(oidc_err(tr!("err.oidc.wrongAccount", account = identity.display(), provider = name)));
    }
    check_group(&identity, binding.required_group.as_deref())?;

    let key = keyring_load(&binding).await?;
    let path = state.vault_path.clone();
    let vault = tauri::async_runtime::spawn_blocking(move || Vault::unlock_with_oidc_key(path, &binding, &key))
        .await
        .map_err(|_| CmdError::new("internal", tr!("err.interrupted")))?
        .map_err(|e| match e {
            vault::Error::InvalidCredentials => oidc_err(tr!("err.oidc.keyMismatch")),
            other => other.into(),
        })?;
    state.set_vault(vault);
    Ok(())
}

/// Lance la connexion depuis un téléphone : renvoie le QR code et le code à comparer.
/// Le déverrouillage se fait ensuite avec [`oidc_device_wait`].
#[tauri::command]
pub async fn oidc_device_start(app: AppHandle) -> CmdResult<DeviceStart> {
    let binding = stored_binding(&app)?;
    let cfg = ClientConfig::from(&binding);

    let http = http_client()?;
    let doc = discover(&http, &cfg.issuer).await?;
    let endpoint = doc.device_authorization_endpoint.clone().ok_or_else(|| {
        oidc_err(match binding.provider {
            None => tr!("err.oidc.noPhonePocket"),
            Some(_) => tr!("err.oidc.noPhone", provider = cfg.name()),
        })
    })?;
    let nonce = random_token();
    let mut form = vec![("client_id", cfg.client_id.as_str()), ("scope", cfg.scopes.as_str()), ("nonce", nonce.as_str())];
    if let Some(secret) = &cfg.client_secret {
        form.push(("client_secret", secret.as_str()));
    }
    let response = http
        .post(&endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| oidc_err(tr!("err.oidc.unreachable", detail = e)))?;
    if !response.status().is_success() {
        let detail = crate::http::json::<TokenError>(response).await.ok().map(|e| e.error_description.unwrap_or(e.error));
        return Err(oidc_err(tr!(
            "err.oidc.phoneRefused",
            provider = cfg.name(),
            detail = detail.map(|d| format!(" ({d})")).unwrap_or_default()
        )));
    }
    let auth: DeviceAuthorization =
        crate::http::json(response).await.map_err(|_| oidc_err(tr!("err.oidc.badPhoneReply")))?;
    check_user_code(&auth.user_code)?;
    let page = check_verification_uri(&auth.verification_uri, &cfg)?;
    let qr_target = match &auth.verification_uri_complete {
        Some(uri) => check_verification_uri(uri, &cfg)?,
        None => page.clone(),
    };
    let expires_in = auth.expires_in.clamp(30, 1800);
    let start = DeviceStart {
        user_code: auth.user_code,
        verification_uri: page.to_string(),
        qr: qr_matrix(qr_target.as_str())?,
        expires_in,
    };
    *app.state::<OidcState>().device() = Some(DeviceFlow {
        binding,
        cfg,
        doc,
        device_code: Zeroizing::new(auth.device_code),
        nonce,
        interval: Duration::from_secs(auth.interval.unwrap_or(5).clamp(1, 60)),
        deadline: Instant::now() + Duration::from_secs(expires_in),
    });
    Ok(start)
}

/// Attend la validation sur le téléphone, puis déverrouille.
#[tauri::command]
pub async fn oidc_device_wait(app: AppHandle) -> CmdResult<()> {
    let state = app.state::<AppState>();
    let _guard = state.begin_unlock()?;
    let mut flow = app
        .state::<OidcState>()
        .device()
        .take()
        .ok_or_else(|| oidc_err(tr!("err.oidc.noPhoneFlow")))?;
    let identity = poll_device(&app, &mut flow).await?;
    crate::bring_to_front(&app);
    finish_unlock(&app, flow.binding, identity).await
}

#[tauri::command]
pub fn oidc_cancel(oidc: State<'_, OidcState>) {
    oidc.device().take();
    if let Some(tx) = oidc.cancel.lock().unwrap_or_else(|p| p.into_inner()).take() {
        let _ = tx.send(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issuer_normalization() {
        assert_eq!(normalize_issuer("https://id.example.org/").unwrap(), "https://id.example.org");
        assert_eq!(normalize_issuer(" https://id.example.org ").unwrap(), "https://id.example.org");
        assert!(normalize_issuer("http://id.example.org").is_err(), "HTTP distant refusé");
        assert!(normalize_issuer("http://127.0.0.1:8080").is_ok(), "HTTP local toléré");
        assert!(normalize_issuer("https://id.example.org/?x=1").is_err());
        assert!(normalize_issuer("https://user@id.example.org").is_err());
        assert!(normalize_issuer("ftp://id.example.org").is_err());
        assert!(normalize_issuer("pas une url").is_err());
    }

    #[test]
    fn login_must_be_recent() {
        let now = 1_000_000;
        let m = MAX_AUTH_AGE;
        assert!(!fresh_login(Some(now - 30), now, m, "X").unwrap());
        assert!(!fresh_login(Some(now - m - 60), now, m, "X").unwrap(), "marge d'horloge");
        assert!(fresh_login(Some(now - m - 61), now, m, "X").is_err());
        assert!(fresh_login(Some(now + 30), now, m, "X").is_ok(), "horloge du fournisseur en avance");
        assert!(fresh_login(None, now, m, "X").unwrap(), "non vérifiable : signalé");
        // Délai de grâce de 15 minutes : une connexion d'il y a 10 minutes suffit.
        assert!(!fresh_login(Some(now - 600), now, 900, "X").unwrap());
        assert!(fresh_login(Some(now - 1000), now, 900, "X").is_err());
    }

    #[test]
    fn group_check() {
        let id = Identity {
            subject: "s".into(),
            name: Some("A".into()),
            email: None,
            groups: vec!["admins".into(), "/equipe/ops".into()],
            groups_overage: false,
            reauth_unverified: false,
        };
        assert!(check_group(&id, None).is_ok());
        assert!(check_group(&id, Some("admins")).is_ok());
        assert!(check_group(&id, Some("equipe/ops")).is_ok(), "chemin complet Keycloak");
        assert!(check_group(&id, Some("autres")).is_err());
        assert!(check_group(&id, Some("dmins")).is_err());
    }

    fn cfg(provider: Option<&str>, issuer: &str) -> ClientConfig {
        ClientConfig {
            provider: provider.map(Into::into),
            issuer: issuer.into(),
            client_id: "app".into(),
            client_secret: None,
            redirect_port: 47615,
            scopes: LEGACY_SCOPES.into(),
            groups_claim: DEFAULT_GROUPS_CLAIM.into(),
        }
    }

    #[test]
    fn verification_uri_must_belong_to_issuer() {
        let c = cfg(None, "https://id.example.org");
        assert!(check_verification_uri("https://id.example.org/device?code=AB", &c).is_ok());
        assert!(check_verification_uri("https://evil.example.net/device", &c).is_err());
        assert!(check_verification_uri("http://id.example.org/device", &c).is_err());
        assert!(check_verification_uri("https://id.example.org:8443/device", &c).is_err());
        assert!(check_verification_uri("https://microsoft.com/devicelogin", &c).is_err());
    }

    #[test]
    fn verification_uri_known_provider_domains() {
        let entra = cfg(Some("entra"), "https://login.microsoftonline.com/tenant/v2.0");
        assert!(check_verification_uri("https://microsoft.com/devicelogin", &entra).is_ok());
        assert!(check_verification_uri("https://login.microsoft.com/device", &entra).is_ok());
        assert!(check_verification_uri("https://microsoft.com.evil.net/devicelogin", &entra).is_err());
        assert!(check_verification_uri("http://microsoft.com/devicelogin", &entra).is_err());
        let keycloak = cfg(Some("keycloak"), "https://kc.example.org/realms/r");
        assert!(check_verification_uri("https://microsoft.com/devicelogin", &keycloak).is_err());
    }

    #[test]
    fn groups_claim_lookup() {
        let claims: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            r#"{"groups":["a","b"],"realm_access":{"roles":["admin"]},"role":"solo",
                "http://schemas.example/groups":["x"],"n":[1,"y"]}"#,
        )
        .unwrap();
        assert_eq!(claim_strings(&claims, "groups"), ["a", "b"]);
        assert_eq!(claim_strings(&claims, "realm_access.roles"), ["admin"]);
        assert_eq!(claim_strings(&claims, "role"), ["solo"]);
        assert_eq!(claim_strings(&claims, "http://schemas.example/groups"), ["x"]);
        assert_eq!(claim_strings(&claims, "n"), ["y"]);
        assert!(claim_strings(&claims, "absent").is_empty());
        assert!(claim_strings(&claims, "realm_access.absent").is_empty());
    }

    #[test]
    fn entra_groups_overage_is_detected() {
        let claims: IdClaims = serde_json::from_str(
            r#"{"sub":"s","_claim_names":{"groups":"src1"},"_claim_sources":{"src1":{}}}"#,
        )
        .unwrap();
        let id = claims.into_identity("groups");
        assert!(id.groups_overage);
        assert_eq!(check_group(&id, Some("g")).unwrap_err().message, tr!("err.oidc.groupsOverage"));
    }

    fn input(provider: Option<&str>) -> EnrollInput {
        EnrollInput {
            provider: provider.map(Into::into),
            issuer: "https://id.example.org/".into(),
            client_id: " app ".into(),
            redirect_port: 47615,
            required_group: Some("  ".into()),
            scopes: None,
            groups_claim: None,
            client_secret: None,
        }
    }

    #[test]
    fn enroll_input_validation() {
        let (c, group) = config_from_input(input(None)).unwrap();
        assert_eq!((c.issuer.as_str(), c.client_id.as_str()), ("https://id.example.org", "app"));
        assert_eq!(c.scopes, LEGACY_SCOPES);
        assert_eq!(c.groups_claim, "groups");
        assert!(group.is_none());

        let mut i = input(Some("keycloak"));
        i.scopes = Some("  openid   profile ".into());
        assert_eq!(config_from_input(i).unwrap().0.scopes, "openid profile");

        let mut i = input(Some("keycloak"));
        i.scopes = Some("profile email".into());
        assert!(config_from_input(i).is_err(), "openid obligatoire");

        assert!(config_from_input(input(Some("inconnu"))).is_err());
        assert!(config_from_input(input(Some("google"))).is_err(), "secret exigé par Google");
        let mut i = input(Some("google"));
        i.client_secret = Some(Secret::new("GOCSPX-abc"));
        assert_eq!(config_from_input(i).unwrap().0.client_secret.as_deref().map(String::as_str), Some("GOCSPX-abc"));
    }

    #[test]
    fn legacy_binding_uses_pocket_id_settings() {
        let b = OidcBinding {
            issuer: "https://id.example.org".into(),
            client_id: "app".into(),
            subject: "s".into(),
            redirect_port: 47615,
            required_group: None,
            display_name: "A".into(),
            provider: None,
            scopes: None,
            groups_claim: None,
            client_secret: None,
        };
        let c = ClientConfig::from(&b);
        assert_eq!((c.scopes.as_str(), c.groups_claim.as_str(), c.name().as_str()), (LEGACY_SCOPES, "groups", "Pocket ID"));
        assert_eq!(keyring_label(&b), tr!("oidc.keyringLabel", app = crate::identity::NAME, provider = "Pocket ID"));
        let status = OidcStatus::from(&b);
        assert!(status.phone && status.provider == "pocket-id");
        let google = OidcBinding { provider: Some("google".into()), ..b };
        assert!(!OidcStatus::from(&google).phone);
    }

    #[test]
    fn user_code_is_plain() {
        assert!(check_user_code("ABCD-EFGH").is_ok());
        assert!(check_user_code("<b>x</b>").is_err());
        assert!(check_user_code("").is_err());
    }

    #[test]
    fn qr_matrix_is_square() {
        let qr = qr_matrix("https://id.example.org/device?code=ABCD-EFGH").unwrap();
        assert_eq!(qr.modules.len(), qr.size * qr.size);
        assert!(qr.modules.iter().any(|m| *m));
    }

    /// Le retour du navigateur est accepté sans réponse immédiate : la page finale ne part
    /// qu'une fois le déverrouillage tranché (pas de « Connexion réussie » trompeuse).
    #[tokio::test]
    async fn callback_reply_is_deferred() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = tokio::spawn(async move {
            let mut c = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
            c.write_all(b"GET /callback?state=s1&code=abc HTTP/1.1\r\nHost: x\r\n\r\n").await.unwrap();
            let mut out = String::new();
            c.read_to_string(&mut out).await.unwrap();
            out
        });
        let (code, mut stream) = wait_for_callback(&listener, "s1", "Pocket ID").await.unwrap();
        assert_eq!(code, "abc");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!client.is_finished(), "aucune réponse avant la décision");
        stream.write_all(page(false, &tr!("oidc.page.denied"), "autre compte").as_bytes()).await.unwrap();
        drop(stream);
        let out = client.await.unwrap();
        assert!(out.contains(&tr!("oidc.page.denied")) && !out.contains(&tr!("oidc.page.success")));
    }

    #[test]
    fn account_name_hides_email() {
        assert_eq!(account_name("Administrateur PocketID <adm@example.org>"), "Administrateur PocketID");
        assert_eq!(account_name("Alice"), "Alice");
        assert_eq!(account_name("alice@example.org"), tr!("oidc.linkedAccount"));
        assert_eq!(account_name("Équipe <ops>"), "Équipe <ops>");
        let id = Identity {
            subject: "s".into(),
            name: None,
            email: Some("a@example.org".into()),
            groups: vec![],
            groups_overage: false,
            reauth_unverified: false,
        };
        assert_eq!(id.display(), "s", "jamais l'adresse e-mail");
    }

    #[test]
    fn html_is_escaped() {
        assert_eq!(escape("<script>&\""), "&lt;script&gt;&amp;&quot;");
    }

    // ── Faux fournisseur local : découverte, clés publiques et jeton signé ──

    /// Clé Ed25519 générée pour ces tests uniquement.
    const TEST_KEY_PKCS8: &str = "MC4CAQAwBQYDK2VwBCIEIBjRAUFKHCYiRlh8RNdclwcDNF58P13Daf4MchANlMoH";
    const TEST_JWK_X: &str = "YoGagyTRGhdbzB-b67BIT9j8qb8wwb4Zc-cbqJFcMGM";

    /// Sert la découverte (issuer annoncé `announced`, relatif à l'adresse du serveur)
    /// et les clés publiques ; renvoie l'adresse de base.
    async fn fake_provider(path: &'static str, announced: &'static str) -> String {
        // Comme au démarrage de l'application (lib.rs).
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let b = base.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else { return };
                let mut buf = vec![0u8; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let target = req.split(' ').nth(1).unwrap_or("").to_string();
                let body = if target == format!("{path}/.well-known/openid-configuration") {
                    format!(
                        r#"{{"issuer":"{b}{announced}","authorization_endpoint":"{b}/authorize","token_endpoint":"{b}/token","jwks_uri":"{b}/jwks"}}"#
                    )
                } else if target == "/jwks" {
                    format!(r#"{{"keys":[{{"kty":"OKP","crv":"Ed25519","kid":"k1","alg":"EdDSA","x":"{TEST_JWK_X}"}}]}}"#)
                } else {
                    String::new()
                };
                let status = if body.is_empty() { "404 Not Found" } else { "200 OK" };
                let resp = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(resp.as_bytes()).await;
            }
        });
        base
    }

    fn sign(claims: serde_json::Value) -> String {
        let mut header = jsonwebtoken::Header::new(Algorithm::EdDSA);
        header.kid = Some("k1".into());
        let der = base64::engine::general_purpose::STANDARD.decode(TEST_KEY_PKCS8).unwrap();
        let key = jsonwebtoken::EncodingKey::from_ed_der(&der);
        jsonwebtoken::encode(&header, &claims, &key).unwrap()
    }

    fn exp() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 300
    }

    /// Authentik annonce (et signe) un issuer terminé par « / » : accepté, comme les
    /// groupes lus par chemin pointé ; mauvais issuer ou audience refusés.
    #[tokio::test]
    async fn id_token_verified_against_announced_issuer() {
        let base = fake_provider("/application/o/quayline", "/application/o/quayline/").await;
        let issuer = normalize_issuer(&format!("{base}/application/o/quayline/")).unwrap();
        let http = http_client().unwrap();
        let doc = discover(&http, &issuer).await.unwrap();
        let announced = format!("{base}/application/o/quayline/");

        let token = sign(serde_json::json!({
            "iss": announced, "aud": "app", "sub": "u1", "exp": exp(), "nonce": "n",
            "name": "Alice", "realm_access": { "roles": ["ops"] }
        }));
        let claims = verify_id_token(&http, &doc, "app", &token).await.unwrap();
        assert_eq!(claims.nonce.as_deref(), Some("n"));
        let id = claims.into_identity("realm_access.roles");
        assert_eq!((id.subject.as_str(), id.groups.as_slice()), ("u1", ["ops".to_string()].as_slice()));

        let wrong_aud = sign(serde_json::json!({ "iss": announced, "aud": "other", "sub": "u1", "exp": exp() }));
        assert!(verify_id_token(&http, &doc, "app", &wrong_aud).await.is_err());
        let wrong_iss = sign(serde_json::json!({ "iss": format!("{base}/autre"), "aud": "app", "sub": "u1", "exp": exp() }));
        assert!(verify_id_token(&http, &doc, "app", &wrong_iss).await.is_err());
    }

    /// Un issuer annoncé différent de celui configuré est refusé dès la découverte.
    #[tokio::test]
    async fn discovery_rejects_substituted_issuer() {
        let base = fake_provider("", "/ailleurs").await;
        let http = http_client().unwrap();
        assert!(discover(&http, &base).await.is_err());
    }
}
