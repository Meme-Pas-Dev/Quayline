//! Export et import chiffrés du contenu du coffre.
//!
//! Le fichier est chiffré par une phrase de passe propre à l'export, indépendante du
//! mot de passe maître et des moyens de déverrouillage (qui restent liés à
//! l'installation et ne sont jamais exportés) :
//!
//! ```text
//! phrase de passe ──Argon2id(sel)──▶ clé ──XChaCha20-Poly1305──▶ contenu exporté
//! ```
//!
//! Disposition sur disque, sur le modèle du coffre :
//!
//! ```text
//! "QLEXPORT" | u32 LE longueur de l'en-tête | en-tête JSON | contenu chiffré
//! ```
//!
//! L'en-tête ne contient que de quoi dériver la clé (paramètres, sel, nonce) ; il sert
//! de données associées, et même le nombre d'hôtes reste chiffré.

use std::collections::HashMap;

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::vault::{b64, random_vec, serialize_zeroizing, NONCE_LEN, SALT_LEN};
use crate::{check_password_policy, Error, Host, Identity, KdfParams, KnownHost, Result, Settings, VaultData};

const MAGIC: &[u8; 8] = b"QLEXPORT";
const FORMAT_VERSION: u32 = 1;
const MAX_HEADER_LEN: usize = 4 * 1024;
/// Taille maximale d'un fichier d'export (des milliers d'hôtes tiennent en quelques Mio).
pub const MAX_EXPORT_LEN: usize = 32 * 1024 * 1024;

/// Ce que l'utilisateur choisit d'exporter.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportSelection {
    pub hosts: bool,
    pub identities: bool,
    pub known_hosts: bool,
    pub settings: bool,
}

/// Contenu (déchiffré) d'un fichier d'export.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ExportData {
    /// Horodatage Unix (secondes) de l'export.
    pub created_at: u64,
    /// Version de l'application qui a produit le fichier (informatif).
    pub app_version: String,
    pub hosts: Vec<Host>,
    pub identities: Vec<Identity>,
    pub known_hosts: Vec<KnownHost>,
    pub settings: Option<Settings>,
}

impl ExportData {
    pub fn from_vault(data: &VaultData, selection: ExportSelection, created_at: u64, app_version: &str) -> Self {
        let mut hosts = if selection.hosts { data.hosts.clone() } else { Vec::new() };
        // Sans les identités, la référence ne mènerait nulle part dans le fichier.
        if !selection.identities {
            for host in &mut hosts {
                host.identity_id = None;
            }
        }
        Self {
            created_at,
            app_version: app_version.to_string(),
            hosts,
            identities: if selection.identities { data.identities.clone() } else { Vec::new() },
            known_hosts: if selection.known_hosts { data.known_hosts.clone() } else { Vec::new() },
            settings: selection.settings.then(|| data.settings.clone()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.hosts.is_empty() && self.identities.is_empty() && self.known_hosts.is_empty() && self.settings.is_none()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Header {
    version: u32,
    kdf: KdfParams,
    #[serde(with = "b64")]
    salt: Vec<u8>,
    #[serde(with = "b64")]
    nonce: Vec<u8>,
}

/// Chiffre `data` avec `passphrase` (même politique que le mot de passe maître).
pub fn seal(data: &ExportData, passphrase: &[u8], kdf: KdfParams) -> Result<Vec<u8>> {
    check_password_policy(passphrase)?;
    let header = Header { version: FORMAT_VERSION, kdf, salt: random_vec(SALT_LEN), nonce: random_vec(NONCE_LEN) };
    let header_json = serde_json::to_vec(&header).map_err(|_| Error::Crypto)?;

    let mut out = Vec::with_capacity(12 + header_json.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&(header_json.len() as u32).to_le_bytes());
    out.extend_from_slice(&header_json);

    let key = kdf.derive(passphrase, &header.salt)?;
    let plain = serialize_zeroizing(data)?;
    let ciphertext = XChaCha20Poly1305::new(key.as_ref().into())
        .encrypt(XNonce::from_slice(&header.nonce), Payload { msg: &plain, aad: &out })
        .map_err(|_| Error::Crypto)?;
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Déchiffre un fichier d'export. Une phrase de passe incorrecte et un fichier modifié
/// sont indiscernables : les deux donnent [`Error::InvalidCredentials`].
pub fn open(raw: &[u8], passphrase: &[u8]) -> Result<ExportData> {
    if raw.len() > MAX_EXPORT_LEN {
        return Err(Error::Corrupt("fichier d'export trop volumineux"));
    }
    if raw.len() < 12 || &raw[..8] != MAGIC {
        return Err(Error::Corrupt("ce fichier n'est pas un export Quayline"));
    }
    let header_len = u32::from_le_bytes(raw[8..12].try_into().unwrap()) as usize;
    if header_len > MAX_HEADER_LEN || raw.len() < 12 + header_len {
        return Err(Error::Corrupt("en-tête tronqué"));
    }
    let header: Header =
        serde_json::from_slice(&raw[12..12 + header_len]).map_err(|_| Error::Corrupt("en-tête illisible"))?;
    if header.version != FORMAT_VERSION {
        return Err(Error::UnsupportedVersion(header.version));
    }
    if header.nonce.len() != NONCE_LEN || header.salt.len() != SALT_LEN {
        return Err(Error::Corrupt("en-tête invalide"));
    }
    // Bornes des paramètres Argon2 vérifiées par `derive`.
    let key = header.kdf.derive(passphrase, &header.salt)?;
    let (aad, ciphertext) = raw.split_at(12 + header_len);
    let plain = Zeroizing::new(
        XChaCha20Poly1305::new(key.as_ref().into())
            .decrypt(XNonce::from_slice(&header.nonce), Payload { msg: ciphertext, aad })
            .map_err(|_| Error::InvalidCredentials)?,
    );
    serde_json::from_slice(&plain).map_err(|_| Error::Corrupt("contenu illisible"))
}

// ───────────────────────────── Fusion ─────────────────────────────

/// Que faire d'un élément importé qui existe déjà dans le coffre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Duplicates {
    /// Garder l'élément existant.
    Skip,
    /// Remplacer l'élément existant par l'élément importé.
    Replace,
    /// Garder les deux (l'élément importé reçoit un nouvel identifiant et un nom
    /// suffixé). Pour une clé d'hôte, revient à garder l'existante : un serveur n'a
    /// qu'une clé approuvée.
    KeepBoth,
}

/// Nombre d'éléments nouveaux et déjà présents, par catégorie.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub new: usize,
    pub duplicates: usize,
}

/// Aperçu d'un import, avant toute modification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub created_at: u64,
    pub app_version: String,
    pub hosts: Counts,
    pub identities: Counts,
    /// Clés d'hôte : identiques comptées à part, elles ne changent rien.
    pub known_hosts: Counts,
    pub known_hosts_identical: usize,
    pub settings: bool,
}

/// Résultat d'un import.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: usize,
    pub replaced: usize,
    pub skipped: usize,
    pub settings: bool,
}

/// Suffixe des éléments importés gardés en double.
pub const IMPORTED_SUFFIX: &str = " (importé)";

fn same_identity(a: &Identity, b: &Identity) -> bool {
    a.id == b.id || (a.label == b.label && a.username == b.username)
}

fn same_host(a: &Host, b: &Host) -> bool {
    a.id == b.id || (a.label == b.label && a.hostname == b.hostname && a.port == b.port && a.username == b.username)
}

fn same_server(a: &KnownHost, b: &KnownHost) -> bool {
    a.hostname == b.hostname && a.port == b.port
}

pub fn preview(data: &VaultData, import: &ExportData) -> ImportPreview {
    fn count<T>(items: &[T], exists: impl Fn(&T) -> bool) -> Counts {
        let duplicates = items.iter().filter(|i| exists(i)).count();
        Counts { new: items.len() - duplicates, duplicates }
    }
    let identical = |k: &KnownHost| data.known_hosts.iter().any(|e| same_server(e, k) && e.public_key == k.public_key);
    let differing: Vec<&KnownHost> = import.known_hosts.iter().filter(|k| !identical(k)).collect();
    ImportPreview {
        created_at: import.created_at,
        app_version: import.app_version.clone(),
        hosts: count(&import.hosts, |h| data.hosts.iter().any(|e| same_host(e, h))),
        identities: count(&import.identities, |i| data.identities.iter().any(|e| same_identity(e, i))),
        known_hosts: Counts {
            new: differing.iter().filter(|k| !data.known_hosts.iter().any(|e| same_server(e, k))).count(),
            duplicates: differing.iter().filter(|k| data.known_hosts.iter().any(|e| same_server(e, k))).count(),
        },
        known_hosts_identical: import.known_hosts.len() - differing.len(),
        settings: import.settings.is_some(),
    }
}

/// Libellé d'un élément gardé en double, dans la limite de `max_chars`.
fn suffixed(label: &str, max_chars: usize) -> String {
    let keep = max_chars.saturating_sub(IMPORTED_SUFFIX.chars().count());
    let mut out: String = label.chars().take(keep).collect();
    out.push_str(IMPORTED_SUFFIX);
    out
}

/// Fusionne `import` dans `data`. Les données importées doivent avoir été validées.
///
/// Les références des hôtes vers les identités suivent les décisions prises pour les
/// identités (identité existante gardée, remplacée ou dupliquée) ; une référence vers
/// une identité absente du fichier comme du coffre est retirée.
pub fn merge(
    data: &mut VaultData,
    import: ExportData,
    duplicates: Duplicates,
    import_settings: bool,
    max_label: usize,
) -> ImportReport {
    let mut report = ImportReport::default();
    let mut identity_ids: HashMap<Uuid, Uuid> = HashMap::new();

    for mut identity in import.identities {
        let imported_id = identity.id;
        match data.identities.iter().position(|e| same_identity(e, &identity)) {
            Some(i) => match duplicates {
                Duplicates::Skip => {
                    identity_ids.insert(imported_id, data.identities[i].id);
                    report.skipped += 1;
                }
                Duplicates::Replace => {
                    identity.id = data.identities[i].id;
                    identity_ids.insert(imported_id, identity.id);
                    data.identities[i] = identity;
                    report.replaced += 1;
                }
                Duplicates::KeepBoth => {
                    identity.id = Uuid::new_v4();
                    identity.label = suffixed(&identity.label, max_label);
                    identity_ids.insert(imported_id, identity.id);
                    data.identities.push(identity);
                    report.added += 1;
                }
            },
            None => {
                identity_ids.insert(imported_id, imported_id);
                data.identities.push(identity);
                report.added += 1;
            }
        }
    }

    for mut host in import.hosts {
        host.identity_id = host
            .identity_id
            .and_then(|id| identity_ids.get(&id).copied())
            .or_else(|| host.identity_id.filter(|id| data.identities.iter().any(|i| i.id == *id)));
        match data.hosts.iter().position(|e| same_host(e, &host)) {
            Some(i) => match duplicates {
                Duplicates::Skip => report.skipped += 1,
                Duplicates::Replace => {
                    host.id = data.hosts[i].id;
                    data.hosts[i] = host;
                    report.replaced += 1;
                }
                Duplicates::KeepBoth => {
                    host.id = Uuid::new_v4();
                    host.label = suffixed(&host.label, max_label);
                    data.hosts.push(host);
                    report.added += 1;
                }
            },
            None => {
                data.hosts.push(host);
                report.added += 1;
            }
        }
    }

    for known in import.known_hosts {
        match data.known_hosts.iter().position(|e| same_server(e, &known)) {
            Some(i) if data.known_hosts[i].public_key == known.public_key => report.skipped += 1,
            Some(i) if duplicates == Duplicates::Replace => {
                data.known_hosts[i] = known;
                report.replaced += 1;
            }
            Some(_) => report.skipped += 1,
            None => {
                data.known_hosts.push(known);
                report.added += 1;
            }
        }
    }

    if import_settings {
        if let Some(settings) = import.settings {
            data.settings = settings;
            report.settings = true;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HostKind, Secret};

    const KDF: KdfParams = KdfParams::INSECURE_FOR_TESTS;
    const PASS: &[u8] = b"une phrase de passe d'export";
    const ALL: ExportSelection = ExportSelection { hosts: true, identities: true, known_hosts: true, settings: true };

    fn identity(label: &str) -> Identity {
        Identity {
            id: Uuid::new_v4(),
            label: label.into(),
            username: Some("matt".into()),
            password: Some(Secret::new("hunter2-hunter2")),
            private_key: None,
            key_passphrase: None,
        }
    }

    fn host(label: &str, identity_id: Option<Uuid>) -> Host {
        Host {
            id: Uuid::new_v4(),
            label: label.into(),
            kind: HostKind::Ssh,
            group: None,
            tags: vec![],
            hostname: format!("{label}.example.org"),
            port: 22,
            username: None,
            identity_id,
            color: None,
            notes: String::new(),
        }
    }

    fn known(hostname: &str, key: &str) -> KnownHost {
        KnownHost {
            hostname: hostname.into(),
            port: 22,
            algorithm: "ssh-ed25519".into(),
            fingerprint: format!("SHA256:{key}"),
            public_key: key.into(),
            added_at: 1,
        }
    }

    fn sample() -> VaultData {
        let id = identity("perso");
        VaultData {
            hosts: vec![host("web", Some(id.id)), host("db", None)],
            identities: vec![id],
            known_hosts: vec![known("web.example.org", "AAAA")],
            settings: Settings { terminal_font_size: 18, ..Settings::default() },
        }
    }

    #[test]
    fn round_trip() {
        let data = sample();
        let export = ExportData::from_vault(&data, ALL, 42, "1.2.1");
        let raw = seal(&export, PASS, KDF).unwrap();
        // Rien de lisible en clair, pas même un nom d'hôte.
        assert!(!raw.windows(3).any(|w| w == b"web"));
        let back = open(&raw, PASS).unwrap();
        assert_eq!(back.created_at, 42);
        assert_eq!(back.hosts.len(), 2);
        assert_eq!(back.identities[0].password.as_ref().unwrap().expose(), "hunter2-hunter2");
        assert_eq!(back.settings.unwrap().terminal_font_size, 18);
    }

    #[test]
    fn wrong_passphrase_and_tampering_are_rejected() {
        let raw = seal(&ExportData::from_vault(&sample(), ALL, 1, ""), PASS, KDF).unwrap();
        assert!(matches!(open(&raw, b"pas la bonne phrase"), Err(Error::InvalidCredentials)));
        let mut tampered = raw.clone();
        *tampered.last_mut().unwrap() ^= 1;
        assert!(matches!(open(&tampered, PASS), Err(Error::InvalidCredentials)));
        // L'en-tête est authentifié lui aussi.
        let mut header = raw.clone();
        let pos = header.windows(4).position(|w| w == b"salt").unwrap();
        header[pos + 8] ^= 1;
        assert!(open(&header, PASS).is_err());
        assert!(matches!(open(b"QLVAULT\0....", PASS), Err(Error::Corrupt(_))));
    }

    #[test]
    fn weak_passphrase_is_refused() {
        let export = ExportData::from_vault(&sample(), ALL, 1, "");
        assert!(matches!(seal(&export, b"court", KDF), Err(Error::WeakPassword)));
    }

    #[test]
    fn selection_drops_dangling_identity_references() {
        let sel = ExportSelection { identities: false, known_hosts: false, settings: false, ..ALL };
        let export = ExportData::from_vault(&sample(), sel, 1, "");
        assert!(export.identities.is_empty() && export.known_hosts.is_empty() && export.settings.is_none());
        assert!(export.hosts.iter().all(|h| h.identity_id.is_none()));
    }

    #[test]
    fn import_into_empty_vault_keeps_references() {
        let source = sample();
        let mut target = VaultData::default();
        let import = ExportData::from_vault(&source, ALL, 1, "");
        let p = preview(&target, &import);
        assert_eq!((p.hosts.new, p.identities.new, p.known_hosts.new), (2, 1, 1));
        let report = merge(&mut target, import, Duplicates::Skip, true, 100);
        assert_eq!(report, ImportReport { added: 4, replaced: 0, skipped: 0, settings: true });
        assert_eq!(target.hosts[0].identity_id, Some(target.identities[0].id));
        assert_eq!(target.settings.terminal_font_size, 18);
    }

    #[test]
    fn duplicates_policies() {
        let source = sample();
        let import = || ExportData::from_vault(&source, ALL, 1, "");

        // Réimport dans le même coffre : tout est en double.
        let p = preview(&source, &import());
        assert_eq!((p.hosts.duplicates, p.identities.duplicates, p.known_hosts_identical), (2, 1, 1));

        let mut skip = source.clone();
        skip.hosts[0].notes = "local".into();
        let r = merge(&mut skip, import(), Duplicates::Skip, false, 100);
        assert_eq!((r.added, r.replaced, r.skipped), (0, 0, 4));
        assert_eq!(skip.hosts[0].notes, "local");

        let mut replace = source.clone();
        replace.hosts[0].notes = "local".into();
        let r = merge(&mut replace, import(), Duplicates::Replace, false, 100);
        assert_eq!((r.added, r.replaced, r.skipped), (0, 3, 1));
        assert_eq!(replace.hosts[0].notes, "");
        assert_eq!(replace.hosts.len(), 2);

        let mut both = source.clone();
        let r = merge(&mut both, import(), Duplicates::KeepBoth, false, 100);
        assert_eq!((r.added, r.skipped), (3, 1));
        assert_eq!(both.hosts.len(), 4);
        assert_eq!(both.identities.len(), 2);
        let copy = &both.hosts[2];
        assert_eq!(copy.label, "web (importé)");
        assert_ne!(copy.id, source.hosts[0].id);
        // La copie de l'hôte pointe vers la copie de l'identité.
        assert_eq!(copy.identity_id, Some(both.identities[1].id));
        assert_eq!(both.identities[1].label, "perso (importé)");
    }

    #[test]
    fn duplicates_are_matched_by_content_across_vaults() {
        let source = sample();
        let mut other = VaultData::default();
        let mut id = identity("perso");
        id.password = Some(Secret::new("ancien-mot-de-passe"));
        other.identities.push(id.clone());
        let r = merge(&mut other, ExportData::from_vault(&source, ALL, 1, ""), Duplicates::Skip, false, 100);
        assert_eq!(r.skipped, 1);
        // L'hôte importé est rattaché à l'identité existante.
        assert_eq!(other.hosts[0].identity_id, Some(id.id));
    }

    #[test]
    fn changed_host_key_is_only_replaced_on_request() {
        let mut data = VaultData { known_hosts: vec![known("web.example.org", "OLD")], ..Default::default() };
        let import = ExportData { known_hosts: vec![known("web.example.org", "NEW")], ..Default::default() };
        let p = preview(&data, &import);
        assert_eq!((p.known_hosts.duplicates, p.known_hosts_identical), (1, 0));
        merge(&mut data, import.clone(), Duplicates::KeepBoth, false, 100);
        assert_eq!(data.known_hosts.len(), 1);
        assert_eq!(data.known_hosts[0].public_key, "OLD");
        merge(&mut data, import, Duplicates::Replace, false, 100);
        assert_eq!(data.known_hosts[0].public_key, "NEW");
    }

    #[test]
    fn suffix_respects_label_limit() {
        let long = "x".repeat(100);
        let s = suffixed(&long, 100);
        assert_eq!(s.chars().count(), 100);
        assert!(s.ends_with(IMPORTED_SUFFIX));
    }
}
