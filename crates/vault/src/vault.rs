//! Format du fichier coffre et opérations cryptographiques.
//!
//! Hiérarchie de clés :
//!
//! ```text
//! mot de passe ──Argon2id(sel)──▶ KEK ──XChaCha20-Poly1305──▶ DEK (aléatoire, 256 bits)
//!                                                              │
//!                                     XChaCha20-Poly1305 ◀─────┘
//!                                              │
//!                                       contenu du coffre
//! ```
//!
//! La DEK est aléatoire et « emballée » par un ou plusieurs *slots*. Le slot mot de passe
//! est la racine ; d'autres slots (Pocket ID + trousseau système, passkey FIDO2…) pourront
//! emballer la même DEK sans rechiffrer le contenu ni affaiblir le mot de passe.
//!
//! Disposition sur disque :
//!
//! ```text
//! "QLVAULT\0" | u32 LE longueur de l'en-tête | en-tête JSON | contenu chiffré
//! ```
//!
//! Les 12 + n premiers octets (magic, longueur, en-tête) servent de données associées (AAD)
//! au chiffrement du contenu : toute modification de l'en-tête (slots, paramètres KDF,
//! nonce) invalide le déchiffrement.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::{Error, Result, VaultData};

const MAGIC: &[u8; 8] = b"QLVAULT\0";
const FORMAT_VERSION: u32 = 1;
pub(crate) const KEY_LEN: usize = 32;
pub(crate) const NONCE_LEN: usize = 24;
pub(crate) const SALT_LEN: usize = 32;
const MAX_HEADER_LEN: usize = 64 * 1024;
/// Préfixe des données authentifiées d'un slot.
const SLOT_AAD_PREFIX: &[u8] = b"vault/slot/";

pub const MIN_PASSWORD_LEN: usize = 12;

/// Paramètres Argon2id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KdfParams {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl KdfParams {
    /// 256 Mio, 3 passes, 4 voies : environ 0,5 à 1 s sur un poste récent.
    /// Bien au-delà des recommandations OWASP (19 Mio / 2 passes).
    pub const DEFAULT: Self = Self { m_cost_kib: 256 * 1024, t_cost: 3, p_cost: 4 };

    /// Paramètres volontairement faibles, réservés aux tests.
    #[doc(hidden)]
    pub const INSECURE_FOR_TESTS: Self = Self { m_cost_kib: 8 * 1024, t_cost: 1, p_cost: 1 };

    /// Bornes acceptées à la lecture : empêche un fichier forgé d'imposer
    /// des paramètres absurdes (épuisement mémoire).
    pub(crate) fn validate(&self) -> Result<()> {
        // Au plus 1 Gio et 10 passes : un fichier forgé ne peut pas bloquer
        // le déverrouillage pendant des heures ni épuiser la mémoire.
        let ok = (8 * 1024..=1024 * 1024).contains(&self.m_cost_kib)
            && (1..=10).contains(&self.t_cost)
            && (1..=16).contains(&self.p_cost);
        if ok { Ok(()) } else { Err(Error::Corrupt("KDF parameters out of bounds")) }
    }

    pub(crate) fn derive(&self, password: &[u8], salt: &[u8]) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        self.validate()?;
        let params = Params::new(self.m_cost_kib, self.t_cost, self.p_cost, Some(KEY_LEN))
            .map_err(|_| Error::Corrupt("invalid KDF parameters"))?;
        let mut out = Zeroizing::new([0u8; KEY_LEN]);
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(password, salt, out.as_mut())
            .map_err(|_| Error::Corrupt("key derivation failed"))?;
        Ok(out)
    }
}

/// Compte Pocket ID (OpenID Connect) autorisé à déverrouiller le coffre.
///
/// Ces champs sont lisibles sans déverrouiller (l'écran de verrouillage en a besoin),
/// mais ils font partie des données authentifiées du slot : les modifier dans le
/// fichier rend le slot inutilisable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OidcBinding {
    /// URL de l'instance (issuer OIDC), sans « / » final.
    pub issuer: String,
    pub client_id: String,
    /// Identifiant stable du compte (`sub`) : seul ce compte peut déverrouiller.
    pub subject: String,
    /// Port de l'adresse de retour locale (http://127.0.0.1:<port>/callback).
    pub redirect_port: u16,
    /// Groupe Pocket ID exigé, le cas échéant.
    pub required_group: Option<String>,
    /// Nom affiché à titre informatif (non utilisé pour l'autorisation).
    pub display_name: String,
    // Champs ajoutés avec les fournisseurs OIDC génériques : omis quand ils sont absents,
    // pour que les liaisons Pocket ID existantes gardent exactement la même forme
    // sérialisée (elle fait partie des données authentifiées du slot).
    /// Modèle de fournisseur (« keycloak », « entra »…) ; absent : Pocket ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Scopes demandés ; absents : ceux de Pocket ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scopes: Option<String>,
    /// Donnée du jeton portant les groupes ; absente : `groups`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub groups_claim: Option<String>,
    /// Secret client, pour les fournisseurs qui l'exigent même d'une application de
    /// bureau (Google). Il n'est pas confidentiel dans ce cas (RFC 8252, §8.5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
}

/// Clé de sécurité FIDO2 autorisée à déverrouiller le coffre.
///
/// La clé du slot est la sortie de l'extension `hmac-secret` de l'authentificateur
/// pour `salt`, obtenue avec vérification de l'utilisateur (code PIN) : elle ne
/// quitte jamais la clé physique autrement et n'est stockée nulle part. Comme pour
/// Pocket ID, ces champs font partie des données authentifiées du slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fido2Binding {
    /// Nom donné par l'utilisateur (« YubiKey bleue »…), informatif.
    pub label: String,
    /// Identifiant de la partie de confiance utilisé à l'enregistrement.
    pub rp_id: String,
    #[serde(with = "b64")]
    pub credential_id: Vec<u8>,
    /// Sel `hmac-secret` (32 octets), propre à ce slot.
    #[serde(with = "b64")]
    pub salt: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum SlotKind {
    Password {
        kdf: KdfParams,
        #[serde(with = "b64")]
        salt: Vec<u8>,
    },
    /// DEK emballée par une clé aléatoire conservée dans le trousseau système
    /// (KWallet / GNOME Keyring), libérée après authentification Pocket ID.
    Oidc(OidcBinding),
    /// DEK emballée par le secret `hmac-secret` d'une clé de sécurité FIDO2.
    Fido2(Fido2Binding),
}

impl SlotKind {
    fn tag(&self) -> &'static str {
        match self {
            SlotKind::Password { .. } => "password",
            SlotKind::Oidc(_) => "oidc",
            SlotKind::Fido2(_) => "fido2",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Slot {
    id: Uuid,
    #[serde(flatten)]
    kind: SlotKind,
    #[serde(with = "b64")]
    nonce: Vec<u8>,
    #[serde(with = "b64")]
    wrapped_key: Vec<u8>,
}

impl Slot {
    fn aad(&self) -> Vec<u8> {
        let mut aad = SLOT_AAD_PREFIX.to_vec();
        aad.extend_from_slice(self.kind.tag().as_bytes());
        aad.push(b'/');
        aad.extend_from_slice(self.id.as_bytes());
        // Le compte ou la clé autorisés sont liés cryptographiquement au slot.
        let binding = match &self.kind {
            SlotKind::Oidc(b) => serde_json::to_vec(b).ok(),
            SlotKind::Fido2(b) => serde_json::to_vec(b).ok(),
            SlotKind::Password { .. } => None,
        };
        if let Some(binding) = binding {
            aad.push(b'/');
            aad.extend_from_slice(&binding);
        }
        aad
    }

    fn seal(kind: SlotKind, kek: &[u8; KEY_LEN], dek: &[u8; KEY_LEN]) -> Result<Self> {
        let mut slot = Slot {
            id: Uuid::new_v4(),
            kind,
            nonce: random_vec(NONCE_LEN),
            wrapped_key: Vec::new(),
        };
        let aad = slot.aad();
        slot.wrapped_key = XChaCha20Poly1305::new(kek.into())
            .encrypt(XNonce::from_slice(&slot.nonce), Payload { msg: dek, aad: &aad })
            .map_err(|_| Error::Crypto)?;
        Ok(slot)
    }

    fn open(&self, kek: &[u8; KEY_LEN]) -> Option<Zeroizing<[u8; KEY_LEN]>> {
        if self.nonce.len() != NONCE_LEN {
            return None;
        }
        let aad = self.aad();
        let plain = Zeroizing::new(
            XChaCha20Poly1305::new(kek.into())
                .decrypt(XNonce::from_slice(&self.nonce), Payload { msg: &self.wrapped_key, aad: &aad })
                .ok()?,
        );
        let mut dek = Zeroizing::new([0u8; KEY_LEN]);
        if plain.len() != KEY_LEN {
            return None;
        }
        dek.copy_from_slice(&plain);
        Some(dek)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Header {
    version: u32,
    slots: Vec<Slot>,
    #[serde(with = "b64")]
    nonce: Vec<u8>,
}

/// Coffre déverrouillé. La DEK est effacée de la mémoire à la destruction :
/// verrouiller l'application revient à détruire cette valeur.
pub struct Vault {
    path: PathBuf,
    dek: Zeroizing<[u8; KEY_LEN]>,
    slots: Vec<Slot>,
    pub data: VaultData,
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault").field("path", &self.path).finish_non_exhaustive()
    }
}

impl Vault {
    pub fn exists(path: &Path) -> bool {
        path.is_file()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Crée un nouveau coffre vide. Refuse d'écraser un fichier existant.
    pub fn create(path: impl Into<PathBuf>, password: &[u8], kdf: KdfParams) -> Result<Self> {
        let path = path.into();
        if path.exists() {
            return Err(Error::AlreadyExists);
        }
        check_password_policy(password)?;
        let mut dek = Zeroizing::new([0u8; KEY_LEN]);
        OsRng.fill_bytes(dek.as_mut());
        let slot = password_slot(password, kdf, &dek)?;
        let vault = Vault { path, dek, slots: vec![slot], data: VaultData::default() };
        vault.save()?;
        Ok(vault)
    }

    /// Déverrouille le coffre avec le mot de passe maître.
    pub fn unlock(path: impl Into<PathBuf>, password: &[u8]) -> Result<Self> {
        let path = path.into();
        let raw = fs::read(&path).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => Error::NotFound,
            _ => Error::Io(e),
        })?;
        let (aad, header, ciphertext) = parse(&raw)?;

        let mut found = None;
        for slot in &header.slots {
            let SlotKind::Password { kdf, salt } = &slot.kind else { continue };
            if let Some(key) = slot.open(&*kdf.derive(password, salt)?) {
                found = Some(key);
                break;
            }
        }
        let dek = found.ok_or(Error::InvalidCredentials)?;
        Self::open_payload(path, aad, header, ciphertext, dek)
    }

    /// Slots lisibles sans déverrouiller ; vide si le coffre n'existe pas.
    fn read_slots(path: &Path) -> Result<Vec<Slot>> {
        let raw = match fs::read(path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(Error::Io(e)),
        };
        let (_, header, _) = parse(&raw)?;
        Ok(header.slots)
    }

    /// Lit la liaison Pocket ID sans déverrouiller (pour l'écran de verrouillage).
    pub fn oidc_binding(path: &Path) -> Result<Option<(Uuid, OidcBinding)>> {
        Ok(Self::read_slots(path)?.into_iter().find_map(|s| match s.kind {
            SlotKind::Oidc(b) => Some((s.id, b)),
            _ => None,
        }))
    }

    /// Clés FIDO2 enregistrées, lisibles sans déverrouiller.
    pub fn fido2_bindings(path: &Path) -> Result<Vec<(Uuid, Fido2Binding)>> {
        Ok(Self::read_slots(path)?
            .into_iter()
            .filter_map(|s| match s.kind {
                SlotKind::Fido2(b) => Some((s.id, b)),
                _ => None,
            })
            .collect())
    }

    /// Déverrouille avec le secret `hmac-secret` renvoyé par la clé FIDO2 du slot `id`.
    pub fn unlock_with_fido2(path: impl Into<PathBuf>, id: Uuid, secret: &[u8; KEY_LEN]) -> Result<Self> {
        let path = path.into();
        let raw = fs::read(&path)?;
        let (aad, header, ciphertext) = parse(&raw)?;
        let dek = header
            .slots
            .iter()
            .filter(|s| s.id == id && matches!(s.kind, SlotKind::Fido2(_)))
            .find_map(|s| s.open(secret))
            .ok_or(Error::InvalidCredentials)?;
        Self::open_payload(path, aad, header, ciphertext, dek)
    }

    /// Déverrouille avec la clé du slot Pocket ID (récupérée dans le trousseau système
    /// après authentification). `binding` est celui validé par l'authentification.
    pub fn unlock_with_oidc_key(path: impl Into<PathBuf>, binding: &OidcBinding, key: &[u8; KEY_LEN]) -> Result<Self> {
        let path = path.into();
        let raw = fs::read(&path)?;
        let (aad, header, ciphertext) = parse(&raw)?;
        let dek = header
            .slots
            .iter()
            .filter(|s| matches!(&s.kind, SlotKind::Oidc(b) if b == binding))
            .find_map(|s| s.open(key))
            .ok_or(Error::InvalidCredentials)?;
        Self::open_payload(path, aad, header, ciphertext, dek)
    }

    fn open_payload(
        path: PathBuf,
        aad: &[u8],
        header: Header,
        ciphertext: &[u8],
        dek: Zeroizing<[u8; KEY_LEN]>,
    ) -> Result<Self> {
        if header.nonce.len() != NONCE_LEN {
            return Err(Error::Corrupt("invalid nonce"));
        }
        // La DEK est correcte : un échec ici signifie que le fichier a été altéré.
        let plain = Zeroizing::new(
            XChaCha20Poly1305::new(dek.as_ref().into())
                .decrypt(XNonce::from_slice(&header.nonce), Payload { msg: ciphertext, aad })
                .map_err(|_| Error::Tampered)?,
        );
        let data: VaultData =
            serde_json::from_slice(&plain).map_err(|_| Error::Corrupt("contenu illisible"))?;

        Ok(Vault { path, dek, slots: header.slots, data })
    }

    /// Chiffre et écrit le coffre de façon atomique (fichier temporaire + rename),
    /// avec des permissions 0600.
    pub fn save(&self) -> Result<()> {
        let header = Header { version: FORMAT_VERSION, slots: self.slots.clone(), nonce: random_vec(NONCE_LEN) };
        let header_json = serde_json::to_vec(&header).map_err(|_| Error::Crypto)?;

        let mut out = Vec::with_capacity(12 + header_json.len());
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
        out.extend_from_slice(&header_json);

        let plain = serialize_zeroizing(&self.data)?;
        let ciphertext = XChaCha20Poly1305::new(self.dek.as_ref().into())
            .encrypt(XNonce::from_slice(&header.nonce), Payload { msg: &plain, aad: &out })
            .map_err(|_| Error::Crypto)?;
        out.extend_from_slice(&ciphertext);

        write_atomic(&self.path, &out)
    }

    /// Vérifie un mot de passe sans toucher à l'état courant (pour confirmer
    /// une action sensible, par exemple afficher un secret).
    pub fn verify_password(&self, password: &[u8]) -> Result<bool> {
        for slot in &self.slots {
            let SlotKind::Password { kdf, salt } = &slot.kind else { continue };
            if slot.open(&*kdf.derive(password, salt)?).is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Remplace le mot de passe maître. La DEK et le contenu ne changent pas.
    ///
    /// La clé de données est renouvelée et tout le contenu rechiffré : une ancienne
    /// copie du coffre, même ouverte avec un ancien moyen de déverrouillage, ne permet
    /// plus de déchiffrer le nouveau. Les autres moyens (Pocket ID, clés FIDO2)
    /// emballaient l'ancienne clé et ne peuvent pas être réemballés sans leur propre
    /// secret : ils sont retirés, à réactiver ensuite.
    pub fn change_password(&mut self, current: &[u8], new: &[u8], kdf: KdfParams) -> Result<()> {
        if !self.verify_password(current)? {
            return Err(Error::InvalidCredentials);
        }
        check_password_policy(new)?;
        let mut new_dek = Zeroizing::new([0u8; KEY_LEN]);
        OsRng.fill_bytes(new_dek.as_mut());
        let slot = password_slot(new, kdf, &new_dek)?;
        let previous_slots = std::mem::replace(&mut self.slots, vec![slot]);
        let previous_dek = std::mem::replace(&mut self.dek, new_dek);
        if let Err(e) = self.save() {
            // Le disque contient toujours l'ancien coffre : rester cohérent.
            self.slots = previous_slots;
            self.dek = previous_dek;
            return Err(e);
        }
        Ok(())
    }
}

impl Vault {
    pub fn oidc(&self) -> Option<&OidcBinding> {
        self.slots.iter().find_map(|s| match &s.kind {
            SlotKind::Oidc(b) => Some(b),
            _ => None,
        })
    }

    /// Active (ou remplace) le déverrouillage Pocket ID. `key` est une clé aléatoire
    /// que l'appelant conserve dans le trousseau système.
    pub fn set_oidc(&mut self, binding: OidcBinding, key: &[u8; KEY_LEN]) -> Result<()> {
        let slot = Slot::seal(SlotKind::Oidc(binding), key, &self.dek)?;
        let previous = self.slots.clone();
        self.slots.retain(|s| !matches!(s.kind, SlotKind::Oidc(_)));
        self.slots.push(slot);
        if let Err(e) = self.save() {
            self.slots = previous;
            return Err(e);
        }
        Ok(())
    }

    /// Désactive le déverrouillage Pocket ID (le mot de passe maître reste la racine).
    pub fn remove_oidc(&mut self) -> Result<()> {
        let previous = self.slots.clone();
        self.slots.retain(|s| !matches!(s.kind, SlotKind::Oidc(_)));
        if let Err(e) = self.save() {
            self.slots = previous;
            return Err(e);
        }
        Ok(())
    }
}

impl Vault {
    pub fn fido2_keys(&self) -> Vec<(Uuid, &Fido2Binding)> {
        self.slots
            .iter()
            .filter_map(|s| match &s.kind {
                SlotKind::Fido2(b) => Some((s.id, b)),
                _ => None,
            })
            .collect()
    }

    /// Ajoute une clé FIDO2. `secret` est la sortie `hmac-secret` pour `binding.salt`.
    pub fn add_fido2(&mut self, binding: Fido2Binding, secret: &[u8; KEY_LEN]) -> Result<Uuid> {
        if binding.salt.len() != KEY_LEN || binding.credential_id.is_empty() {
            return Err(Error::Crypto);
        }
        if self.slots.len() >= MAX_SLOTS {
            return Err(Error::TooManySlots);
        }
        let slot = Slot::seal(SlotKind::Fido2(binding), secret, &self.dek)?;
        let id = slot.id;
        let previous = self.slots.clone();
        self.slots.push(slot);
        if let Err(e) = self.save() {
            self.slots = previous;
            return Err(e);
        }
        Ok(id)
    }

    /// Retire une clé FIDO2. Renvoie `false` si elle n'existait pas.
    pub fn remove_fido2(&mut self, id: Uuid) -> Result<bool> {
        let previous = self.slots.clone();
        self.slots.retain(|s| !(s.id == id && matches!(s.kind, SlotKind::Fido2(_))));
        if self.slots.len() == previous.len() {
            return Ok(false);
        }
        if let Err(e) = self.save() {
            self.slots = previous;
            return Err(e);
        }
        Ok(true)
    }
}

pub fn check_password_policy(password: &[u8]) -> Result<()> {
    let len = std::str::from_utf8(password).map(|s| s.chars().count()).unwrap_or(password.len());
    if len < MIN_PASSWORD_LEN {
        return Err(Error::WeakPassword);
    }
    Ok(())
}

fn password_slot(password: &[u8], kdf: KdfParams, dek: &[u8; KEY_LEN]) -> Result<Slot> {
    let salt = random_vec(SALT_LEN);
    let kek = kdf.derive(password, &salt)?;
    Slot::seal(SlotKind::Password { kdf, salt }, &kek, dek)
}

/// Mot de passe + Pocket ID + clés FIDO2 : largement suffisant.
const MAX_SLOTS: usize = 16;

fn parse(raw: &[u8]) -> Result<(&[u8], Header, &[u8])> {
    if raw.len() < 12 || &raw[..8] != MAGIC {
        return Err(Error::Corrupt("this file is not a valid vault"));
    }
    let header_len = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
    if header_len > MAX_HEADER_LEN || raw.len() < 12 + header_len {
        return Err(Error::Corrupt("truncated header"));
    }
    let header: Header =
        serde_json::from_slice(&raw[12..12 + header_len]).map_err(|_| Error::Corrupt("unreadable header"))?;
    if header.version != FORMAT_VERSION {
        return Err(Error::UnsupportedVersion(header.version));
    }
    if header.slots.is_empty() {
        return Err(Error::Corrupt("no unlock slot"));
    }
    // Un seul mot de passe maître, et un nombre raisonnable de slots : le
    // déverrouillage essaie chaque slot « mot de passe » (un Argon2 par slot).
    let password_slots = header.slots.iter().filter(|s| matches!(s.kind, SlotKind::Password { .. })).count();
    if password_slots != 1 || header.slots.len() > MAX_SLOTS {
        return Err(Error::Corrupt("invalid unlock slots"));
    }
    Ok((&raw[..12 + header_len], header, &raw[12 + header_len..]))
}

/// Sérialise dans un tampon de taille exacte, effacé à la destruction.
/// Deux passes évitent les réallocations qui laisseraient des copies du clair en mémoire.
pub(crate) fn serialize_zeroizing<T: Serialize>(value: &T) -> Result<Zeroizing<Vec<u8>>> {
    struct Counter(usize);
    impl Write for Counter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0 += buf.len();
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value).map_err(|_| Error::Crypto)?;
    let mut buf = Zeroizing::new(Vec::with_capacity(counter.0));
    serde_json::to_writer(&mut *buf, value).map_err(|_| Error::Crypto)?;
    Ok(buf)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    create_private_dir(dir)?;
    let file_name = path.file_name().ok_or(Error::Corrupt("invalid vault path"))?;
    let tmp = dir.join(format!(".{}.tmp-{}", file_name.to_string_lossy(), Uuid::new_v4().simple()));

    let result = (|| {
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        // Synchronise aussi le dossier, pour que le renommage survive à une coupure
        // (sous Windows, un dossier ne s'ouvre pas ainsi ; le renommage y est déjà atomique).
        #[cfg(unix)]
        fs::File::open(dir)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(Error::Io)
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

pub(crate) fn random_vec(len: usize) -> Vec<u8> {
    let mut v = vec![0u8; len];
    OsRng.fill_bytes(&mut v);
    v
}

pub(crate) mod b64 {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&B64.encode(bytes))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        B64.decode(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Host, Identity, Secret};

    const PW: &[u8] = b"correct horse battery staple";
    const KDF: KdfParams = KdfParams::INSECURE_FOR_TESTS;

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/vault.qlv");
        (dir, path)
    }

    fn sample_host() -> Host {
        Host {
            id: Uuid::new_v4(),
            label: "prod".into(),
            kind: crate::HostKind::Ssh,
            group: Some("Serveurs".into()),
            tags: vec![],
            hostname: "prod.example.org".into(),
            port: 22,
            username: Some("root".into()),
            identity_id: None,
            color: None,
            notes: String::new(),
        }
    }

    #[test]
    fn hosts_without_kind_are_ssh() {
        let json = r#"{"id":"6f9619ff-8b86-d011-b42d-00c04fc964ff","label":"a","hostname":"h","port":22}"#;
        let host: Host = serde_json::from_str(json).unwrap();
        assert_eq!(host.kind, crate::HostKind::Ssh);
        let sftp: Host = serde_json::from_str(&json.replace("\"port\"", "\"kind\":\"sftp\",\"port\"")).unwrap();
        assert_eq!(sftp.kind, crate::HostKind::Sftp);
    }

    #[test]
    fn roundtrip() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        v.data.hosts.push(sample_host());
        v.data.identities.push(Identity {
            id: Uuid::new_v4(),
            label: "clé perso".into(),
            username: None,
            password: Some(Secret::new("s3cr3t-password")),
            private_key: None,
            key_passphrase: None,
        });
        v.save().unwrap();
        drop(v);

        let v = Vault::unlock(&path, PW).unwrap();
        assert_eq!(v.data.hosts.len(), 1);
        assert_eq!(v.data.identities[0].password.as_ref().unwrap().expose(), "s3cr3t-password");
    }

    #[test]
    fn nothing_in_clear_on_disk() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        v.data.hosts.push(sample_host());
        v.save().unwrap();
        let raw = fs::read(&path).unwrap();
        let hay = String::from_utf8_lossy(&raw);
        assert!(!hay.contains("prod.example.org"));
        assert!(!hay.contains("Serveurs"));
    }

    #[cfg(unix)]
    #[test]
    fn file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_d, path) = tmp();
        Vault::create(&path, PW, KDF).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn wrong_password_rejected() {
        let (_d, path) = tmp();
        Vault::create(&path, PW, KDF).unwrap();
        assert!(matches!(Vault::unlock(&path, b"not the password!!"), Err(Error::InvalidCredentials)));
    }

    #[test]
    fn weak_password_rejected() {
        let (_d, path) = tmp();
        assert!(matches!(Vault::create(&path, b"short", KDF), Err(Error::WeakPassword)));
        assert!(!path.exists());
    }

    #[test]
    fn refuses_to_overwrite() {
        let (_d, path) = tmp();
        Vault::create(&path, PW, KDF).unwrap();
        assert!(matches!(Vault::create(&path, PW, KDF), Err(Error::AlreadyExists)));
    }

    #[test]
    fn ciphertext_tampering_detected() {
        let (_d, path) = tmp();
        Vault::create(&path, PW, KDF).unwrap();
        let mut raw = fs::read(&path).unwrap();
        let last = raw.len() - 1;
        raw[last] ^= 1;
        fs::write(&path, raw).unwrap();
        assert!(matches!(Vault::unlock(&path, PW), Err(Error::Tampered)));
    }

    #[test]
    fn header_tampering_detected() {
        let (_d, path) = tmp();
        Vault::create(&path, PW, KDF).unwrap();
        let raw = fs::read(&path).unwrap();
        let len = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
        let mut header: serde_json::Value = serde_json::from_slice(&raw[12..12 + len]).unwrap();
        // Ajout d'un champ inconnu : l'AAD change, le contenu doit être rejeté.
        header["extra"] = serde_json::json!(1);
        let new_header = serde_json::to_vec(&header).unwrap();
        let mut forged = MAGIC.to_vec();
        forged.extend_from_slice(&(new_header.len() as u32).to_le_bytes());
        forged.extend_from_slice(&new_header);
        forged.extend_from_slice(&raw[12 + len..]);
        fs::write(&path, forged).unwrap();
        assert!(matches!(Vault::unlock(&path, PW), Err(Error::Tampered)));
    }

    #[test]
    fn absurd_kdf_params_rejected() {
        let (_d, path) = tmp();
        Vault::create(&path, PW, KDF).unwrap();
        let raw = fs::read(&path).unwrap();
        let len = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
        let mut header: serde_json::Value = serde_json::from_slice(&raw[12..12 + len]).unwrap();
        header["slots"][0]["kdf"]["m_cost_kib"] = serde_json::json!(u32::MAX);
        let new_header = serde_json::to_vec(&header).unwrap();
        let mut forged = MAGIC.to_vec();
        forged.extend_from_slice(&(new_header.len() as u32).to_le_bytes());
        forged.extend_from_slice(&new_header);
        forged.extend_from_slice(&raw[12 + len..]);
        fs::write(&path, forged).unwrap();
        assert!(matches!(Vault::unlock(&path, PW), Err(Error::Corrupt(_))));
    }

    #[test]
    fn garbage_rejected() {
        let (_d, path) = tmp();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"hello").unwrap();
        assert!(matches!(Vault::unlock(&path, PW), Err(Error::Corrupt(_))));
    }

    #[test]
    fn nonce_changes_on_every_save() {
        let (_d, path) = tmp();
        let v = Vault::create(&path, PW, KDF).unwrap();
        let a = fs::read(&path).unwrap();
        v.save().unwrap();
        let b = fs::read(&path).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn change_password() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        v.data.hosts.push(sample_host());
        v.save().unwrap();
        assert!(matches!(
            v.change_password(b"wrong password!!", b"another long passphrase", KDF),
            Err(Error::InvalidCredentials)
        ));
        v.change_password(PW, b"another long passphrase", KDF).unwrap();
        drop(v);
        assert!(matches!(Vault::unlock(&path, PW), Err(Error::InvalidCredentials)));
        let v = Vault::unlock(&path, b"another long passphrase").unwrap();
        assert_eq!(v.data.hosts.len(), 1);
    }

    fn binding() -> OidcBinding {
        OidcBinding {
            issuer: "https://id.example.org".into(),
            client_id: "app".into(),
            subject: "user-123".into(),
            redirect_port: 47615,
            required_group: None,
            display_name: "Alice".into(),
            provider: None,
            scopes: None,
            groups_claim: None,
            client_secret: None,
        }
    }

    /// Les liaisons Pocket ID créées avant les fournisseurs génériques doivent garder
    /// la même forme sérialisée, sinon leur slot ne s'ouvre plus (données authentifiées).
    #[test]
    fn legacy_oidc_binding_serialization_is_unchanged() {
        let legacy = r#"{"issuer":"https://id.example.org","client_id":"app","subject":"user-123","redirect_port":47615,"required_group":null,"display_name":"Alice"}"#;
        assert_eq!(serde_json::to_string(&binding()).unwrap(), legacy);
        assert_eq!(serde_json::from_str::<OidcBinding>(legacy).unwrap(), binding());

        let generic = OidcBinding { provider: Some("keycloak".into()), scopes: Some("openid".into()), ..binding() };
        let json = serde_json::to_string(&generic).unwrap();
        assert!(json.starts_with(&legacy[..legacy.len() - 1]) && json.contains(r#""provider":"keycloak""#));
        assert!(!json.contains("client_secret"));
    }

    #[test]
    fn oidc_slot_roundtrip() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        v.data.hosts.push(sample_host());
        let key = [7u8; KEY_LEN];
        v.set_oidc(binding(), &key).unwrap();
        drop(v);

        let (_, b) = Vault::oidc_binding(&path).unwrap().unwrap();
        assert_eq!(b, binding());
        let v = Vault::unlock_with_oidc_key(&path, &b, &key).unwrap();
        assert_eq!(v.data.hosts.len(), 1);
        // Le mot de passe reste valable.
        Vault::unlock(&path, PW).unwrap();
        assert!(matches!(Vault::unlock_with_oidc_key(&path, &b, &[8u8; KEY_LEN]), Err(Error::InvalidCredentials)));
    }

    #[test]
    fn oidc_binding_tampering_detected() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        let key = [7u8; KEY_LEN];
        v.set_oidc(binding(), &key).unwrap();
        drop(v);
        // Un attaquant remplace le compte autorisé dans l'en-tête.
        let raw = fs::read(&path).unwrap();
        let len = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
        let mut header: serde_json::Value = serde_json::from_slice(&raw[12..12 + len]).unwrap();
        for slot in header["slots"].as_array_mut().unwrap() {
            if slot["kind"] == "oidc" {
                slot["subject"] = serde_json::json!("attaquant");
            }
        }
        let new_header = serde_json::to_vec(&header).unwrap();
        let mut forged = MAGIC.to_vec();
        forged.extend_from_slice(&(new_header.len() as u32).to_le_bytes());
        forged.extend_from_slice(&new_header);
        forged.extend_from_slice(&raw[12 + len..]);
        fs::write(&path, forged).unwrap();

        let (_, forged_binding) = Vault::oidc_binding(&path).unwrap().unwrap();
        assert_eq!(forged_binding.subject, "attaquant");
        // Même avec la bonne clé, le slot modifié ne s'ouvre pas.
        assert!(matches!(
            Vault::unlock_with_oidc_key(&path, &forged_binding, &key),
            Err(Error::InvalidCredentials)
        ));
    }

    #[test]
    fn oidc_can_be_removed_and_is_dropped_by_password_change() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        let key = [7u8; KEY_LEN];
        v.set_oidc(binding(), &key).unwrap();
        Vault::unlock_with_oidc_key(&path, &binding(), &key).unwrap();
        v.remove_oidc().unwrap();
        assert!(Vault::oidc_binding(&path).unwrap().is_none());
        assert!(v.oidc().is_none());

        // Le changement de mot de passe renouvelle la clé de données : Pocket ID est retiré.
        v.set_oidc(binding(), &key).unwrap();
        v.change_password(PW, b"another long passphrase", KDF).unwrap();
        assert!(v.oidc().is_none());
        assert!(Vault::oidc_binding(&path).unwrap().is_none());
    }

    #[test]
    fn no_temp_files_left() {
        let (_d, path) = tmp();
        let v = Vault::create(&path, PW, KDF).unwrap();
        v.save().unwrap();
        let entries: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }

    fn fido2_binding(label: &str) -> Fido2Binding {
        Fido2Binding {
            label: label.into(),
            rp_id: "dev.example.app".into(),
            credential_id: vec![1, 2, 3, 4],
            salt: vec![9u8; KEY_LEN],
        }
    }

    #[test]
    fn fido2_slots_roundtrip() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        v.data.hosts.push(sample_host());
        v.save().unwrap();
        let a = v.add_fido2(fido2_binding("principale"), &[1u8; KEY_LEN]).unwrap();
        let b = v.add_fido2(fido2_binding("secours"), &[2u8; KEY_LEN]).unwrap();
        assert_eq!(v.fido2_keys().len(), 2);

        let listed = Vault::fido2_bindings(&path).unwrap();
        assert_eq!(listed.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![a, b]);
        assert_eq!(Vault::unlock_with_fido2(&path, a, &[1u8; KEY_LEN]).unwrap().data.hosts.len(), 1);
        Vault::unlock_with_fido2(&path, b, &[2u8; KEY_LEN]).unwrap();
        // Mauvais secret, ou bon secret présenté pour un autre slot.
        assert!(matches!(Vault::unlock_with_fido2(&path, a, &[2u8; KEY_LEN]), Err(Error::InvalidCredentials)));
        assert!(matches!(Vault::unlock_with_fido2(&path, Uuid::new_v4(), &[1u8; KEY_LEN]), Err(Error::InvalidCredentials)));

        assert!(v.remove_fido2(a).unwrap());
        assert!(!v.remove_fido2(a).unwrap());
        assert!(matches!(Vault::unlock_with_fido2(&path, a, &[1u8; KEY_LEN]), Err(Error::InvalidCredentials)));
        assert_eq!(Vault::fido2_bindings(&path).unwrap().len(), 1);
    }

    #[test]
    fn fido2_binding_tampering_detected() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        let id = v.add_fido2(fido2_binding("clé"), &[1u8; KEY_LEN]).unwrap();

        // Remplacer le sel ou l'identifiant de clé dans le fichier rend le slot inutilisable.
        let raw = fs::read(&path).unwrap();
        let len = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
        let mut header: serde_json::Value = serde_json::from_slice(&raw[12..12 + len]).unwrap();
        for slot in header["slots"].as_array_mut().unwrap() {
            if slot["kind"] == "fido2" {
                slot["label"] = "autre".into();
            }
        }
        let header = serde_json::to_vec(&header).unwrap();
        let mut forged = raw[..8].to_vec();
        forged.extend_from_slice(&(header.len() as u32).to_le_bytes());
        forged.extend_from_slice(&header);
        forged.extend_from_slice(&raw[12 + len..]);
        fs::write(&path, forged).unwrap();

        assert!(matches!(Vault::unlock_with_fido2(&path, id, &[1u8; KEY_LEN]), Err(Error::InvalidCredentials)));
    }

    #[test]
    fn forged_headers_are_rejected() {
        let (_d, path) = tmp();
        Vault::create(&path, PW, KDF).unwrap();
        let raw = fs::read(&path).unwrap();
        let len = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
        let header: serde_json::Value = serde_json::from_slice(&raw[12..12 + len]).unwrap();
        let rewrite = |h: &serde_json::Value| {
            let h = serde_json::to_vec(h).unwrap();
            let mut out = raw[..8].to_vec();
            out.extend_from_slice(&(h.len() as u32).to_le_bytes());
            out.extend_from_slice(&h);
            out.extend_from_slice(&raw[12 + len..]);
            fs::write(&path, out).unwrap();
        };

        // Second slot « mot de passe » : refusé avant toute dérivation.
        let mut two = header.clone();
        let slot = two["slots"][0].clone();
        two["slots"].as_array_mut().unwrap().push(slot);
        rewrite(&two);
        assert!(matches!(Vault::unlock(&path, PW), Err(Error::Corrupt(_))));

        // Paramètres Argon2 démesurés.
        let mut heavy = header.clone();
        heavy["slots"][0]["kdf"]["m_cost_kib"] = (4u64 * 1024 * 1024).into();
        rewrite(&heavy);
        assert!(matches!(Vault::unlock(&path, PW), Err(Error::Corrupt(_))));
    }

    #[test]
    fn password_change_renews_data_key() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        v.data.hosts.push(sample_host());
        v.save().unwrap();
        v.add_fido2(fido2_binding("clé"), &[1u8; KEY_LEN]).unwrap();
        let old_file = fs::read(&path).unwrap();
        let old_dek = *v.dek;

        v.change_password(PW, b"another long passphrase", KDF).unwrap();
        assert_ne!(*v.dek, old_dek, "la clé de données doit changer");
        assert!(v.fido2_keys().is_empty(), "les clés FIDO2 sont retirées");

        // Le nouveau fichier s'ouvre avec le nouveau mot de passe, contenu intact.
        let reopened = Vault::unlock(&path, b"another long passphrase").unwrap();
        assert_eq!(reopened.data.hosts.len(), 1);
        assert!(matches!(Vault::unlock(&path, PW), Err(Error::InvalidCredentials)));
        assert!(Vault::fido2_bindings(&path).unwrap().is_empty());

        // L'ancienne clé de données (tirée d'une vieille copie) ne déchiffre plus le
        // nouveau contenu.
        let raw = fs::read(&path).unwrap();
        let (aad, header, ciphertext) = parse(&raw).unwrap();
        let attempt = XChaCha20Poly1305::new((&old_dek).into())
            .decrypt(XNonce::from_slice(&header.nonce), Payload { msg: ciphertext, aad });
        assert!(attempt.is_err());
        assert_ne!(raw, old_file);
    }

    #[test]
    fn fido2_rejects_bad_salt() {
        let (_d, path) = tmp();
        let mut v = Vault::create(&path, PW, KDF).unwrap();
        let mut b = fido2_binding("clé");
        b.salt = vec![0u8; 8];
        assert!(v.add_fido2(b, &[1u8; KEY_LEN]).is_err());
        assert!(v.fido2_keys().is_empty());
    }
}
