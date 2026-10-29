//! Vérification des mises à jour : dernière release du dépôt public sur GitHub.
//!
//! La recherche ne télécharge rien. Les adresses (page de la release, `.dmg`) restent
//! côté backend : l'interface demande seulement d'ouvrir la page ou d'installer, elle
//! ne peut donc ni faire ouvrir ni faire télécharger une adresse de son choix.
//!
//! Sur macOS, l'installation (après accord) télécharge le `.dmg`, contrôle son empreinte
//! (SHA256SUMS.macos), puis la signature de l'application qu'il contient : même équipe
//! Developer ID que l'application en cours, notarisation acceptée par Gatekeeper, numéro
//! de version attendu. L'application est alors remplacée et redémarrée.

use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::error::{CmdError, CmdResult};
use crate::i18n::tr;

const TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct UpdateState {
    found: Mutex<Option<Found>>,
    installing: std::sync::atomic::AtomicBool,
}

/// Dernière version trouvée par [`update_check`].
#[derive(Clone)]
struct Found {
    version: String,
    release_url: String,
    /// `.dmg` et fichier d'empreintes, si la release les fournit.
    macos: Option<(Asset, Asset)>,
}

impl UpdateState {
    fn found(&self) -> std::sync::MutexGuard<'_, Option<Found>> {
        self.found.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    /// Date de publication (AAAA-MM-JJ), si GitHub la donne.
    published: Option<String>,
    /// Installation possible depuis l'application (macOS, `.dmg` présent).
    installable: bool,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize, Clone)]
struct Asset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

fn update_err(message: String) -> CmdError {
    CmdError::new("update", message)
}

/// « v1.2.3 » ou « 1.2.3 » → (1, 2, 3). Rien pour une préversion (« 1.3.0-rc1 »).
fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let mut parts = s.strip_prefix('v').unwrap_or(s).split('.');
    let mut next = || parts.next()?.parse::<u64>().ok();
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

/// Adresse sous `https://github.com/<dépôt>/releases/`, et nulle part ailleurs.
fn check_release_url(url: &str) -> bool {
    let prefix = format!("https://github.com/{}/releases/", crate::identity::REPOSITORY);
    url.len() > prefix.len()
        && url.is_char_boundary(prefix.len())
        && url[..prefix.len()].eq_ignore_ascii_case(&prefix)
        && !url.contains(['?', '#', '\\', ' '])
}

/// `.dmg` macOS de la version et son fichier d'empreintes.
fn macos_assets(assets: &[Asset], version: &str) -> Option<(Asset, Asset)> {
    let find = |name: &str| {
        assets.iter().find(|a| a.name == name && check_release_url(&a.browser_download_url)).cloned()
    };
    Some((find(&format!("{}-{version}-macos-arm64.dmg", crate::identity::NAME))?, find("SHA256SUMS.macos")?))
}

/// Nouvelle version disponible, ou rien si l'application est à jour.
#[tauri::command]
pub async fn update_check(app: AppHandle, state: State<'_, UpdateState>) -> CmdResult<Option<UpdateInfo>> {
    // Version d'une boutique (Flathub) : c'est elle qui livre les mises à jour.
    if crate::identity::UPDATES_BY_STORE {
        return Ok(None);
    }
    let current = app.package_info().version.to_string();
    let current = parse_version(&current).ok_or_else(|| update_err(tr!("err.update.version")))?;

    let http = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(format!("{}/{}", crate::identity::NAME, env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| update_err(tr!("err.update.unreachable", detail = e)))?;
    let url = format!("https://api.github.com/repos/{}/releases/latest", crate::identity::REPOSITORY);
    let response = http
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| update_err(tr!("err.update.unreachable", detail = e)))?;
    if !response.status().is_success() {
        return Err(update_err(tr!("err.update.unreachable", detail = response.status())));
    }
    let release: Release = crate::http::json(response).await.map_err(|_| update_err(tr!("err.update.badReply")))?;

    let Some(latest) = parse_version(&release.tag_name) else {
        return Err(update_err(tr!("err.update.badReply")));
    };
    if latest <= current {
        *state.found() = None;
        return Ok(None);
    }
    if !check_release_url(&release.html_url) {
        return Err(update_err(tr!("err.update.badReply")));
    }
    let version = format!("{}.{}.{}", latest.0, latest.1, latest.2);
    let macos = macos_assets(&release.assets, &version);
    let installable = cfg!(target_os = "macos") && macos.is_some() && !crate::identity::is_flatpak();
    *state.found() = Some(Found { version: version.clone(), release_url: release.html_url, macos });
    Ok(Some(UpdateInfo {
        version,
        published: release.published_at.and_then(|d| d.get(..10).map(str::to_owned)),
        installable,
    }))
}

/// Ouvre dans le navigateur la page de la release trouvée par [`update_check`].
#[tauri::command]
pub async fn update_open(app: AppHandle) -> CmdResult<()> {
    let url = app.state::<UpdateState>().found().as_ref().map(|f| f.release_url.clone());
    let url = url.ok_or_else(|| update_err(tr!("err.update.none")))?;
    tauri::async_runtime::spawn_blocking(move || open::that_detached(url))
        .await
        .ok()
        .and_then(|r| r.ok())
        .ok_or_else(|| update_err(tr!("err.oidc.browser")))
}

// ───────────────────────────── Installation (macOS) ─────────────────────────────

/// Taille maximale acceptée pour le `.dmg`.
const MAX_DMG: u64 = 400 * 1024 * 1024;
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[derive(Serialize, Clone)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum InstallProgress {
    Download { done: u64, total: u64 },
    Verify,
    Install,
}

/// Télécharge, vérifie et installe la version trouvée par [`update_check`]. L'interface
/// propose ensuite de redémarrer ([`update_restart`]).
#[tauri::command]
pub async fn update_install(app: AppHandle, progress: tauri::ipc::Channel<InstallProgress>) -> CmdResult<()> {
    if !cfg!(target_os = "macos") || crate::identity::is_flatpak() {
        return Err(update_err(tr!("err.update.unsupported")));
    }
    let state = app.state::<UpdateState>();
    let found = state.found().clone().ok_or_else(|| update_err(tr!("err.update.none")))?;
    let (dmg, sums) = found.macos.clone().ok_or_else(|| update_err(tr!("err.update.unsupported")))?;
    if state.installing.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Err(CmdError::new("busy", tr!("err.busy")));
    }
    let result = install(&found.version, &dmg, &sums, &progress).await;
    state.installing.store(false, std::sync::atomic::Ordering::SeqCst);
    result
}

/// Relance l'application (après une installation réussie).
#[tauri::command]
pub fn update_restart(app: AppHandle) {
    app.restart();
}

async fn install(
    version: &str,
    dmg: &Asset,
    sums: &Asset,
    progress: &tauri::ipc::Channel<InstallProgress>,
) -> CmdResult<()> {
    let bundle = current_bundle()?;
    let dir = TempDir::new()?;
    let http = reqwest::Client::builder()
        .timeout(DOWNLOAD_TIMEOUT)
        .user_agent(format!("{}/{}", crate::identity::NAME, env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| update_err(tr!("err.update.download", detail = e)))?;

    // Empreinte attendue, puis le .dmg (empreinte calculée pendant le téléchargement).
    let sums_text = download_text(&http, &sums.browser_download_url).await?;
    let expected = expected_sha256(&sums_text, &dmg.name).ok_or_else(|| update_err(tr!("err.update.checksum")))?;
    let dmg_path = dir.path.join(&dmg.name);
    let actual = download_file(&http, dmg, &dmg_path, progress).await?;
    if !actual.eq_ignore_ascii_case(&expected) {
        return Err(update_err(tr!("err.update.checksum")));
    }

    let _ = progress.send(InstallProgress::Verify);
    let version = version.to_owned();
    let dir_path = dir.path.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let staged = stage(&dmg_path, &dir_path, &bundle)?;
        verify(&staged, &bundle, &version)?;
        swap(&staged, &bundle)
    })
    .await
    .map_err(|_| CmdError::new("internal", tr!("err.interrupted")))?
    .inspect(|()| {
        let _ = progress.send(InstallProgress::Install);
    })
}

/// `…/Quayline.app`, d'après l'exécutable en cours (`…/Quayline.app/Contents/MacOS/…`).
fn current_bundle() -> CmdResult<std::path::PathBuf> {
    let exe = std::env::current_exe().and_then(|p| p.canonicalize()).map_err(|e| update_err(tr!("err.update.install", detail = e)))?;
    let bundle = exe.ancestors().nth(3).map(std::path::Path::to_path_buf);
    match bundle {
        Some(b) if b.extension().is_some_and(|e| e == "app") => Ok(b),
        _ => Err(update_err(tr!("err.update.notBundle"))),
    }
}

/// Dossier temporaire supprimé à la fin, quoi qu'il arrive.
struct TempDir {
    path: std::path::PathBuf,
}

impl TempDir {
    fn new() -> CmdResult<Self> {
        use rand_core::RngCore;
        let mut rand = [0u8; 8];
        rand_core::OsRng.fill_bytes(&mut rand);
        let path = std::env::temp_dir().join(format!("{}-update-{}", crate::identity::SLUG, hex(&rand)));
        create_private_dir(&path).map_err(|e| update_err(tr!("err.update.install", detail = e)))?;
        Ok(Self { path })
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Dossier réservé à l'utilisateur (0700 sous Unix ; sous Windows, les ACL du dossier
/// temporaire de l'utilisateur s'appliquent).
fn create_private_dir(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new().mode(0o700).create(path)
    }
    #[cfg(not(unix))]
    std::fs::create_dir(path)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Ligne « <empreinte>  <nom> » de SHA256SUMS (format de `shasum -a 256`).
fn expected_sha256(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hash.to_owned())
    })
}

async fn download_text(http: &reqwest::Client, url: &str) -> CmdResult<String> {
    let response = http.get(url).send().await.map_err(|e| update_err(tr!("err.update.download", detail = e)))?;
    if !response.status().is_success() {
        return Err(update_err(tr!("err.update.download", detail = response.status())));
    }
    let bytes = crate::http::body(response, 64 * 1024).await.map_err(|_| update_err(tr!("err.update.checksum")))?;
    String::from_utf8(bytes).map_err(|_| update_err(tr!("err.update.checksum")))
}

/// Écrit le fichier et renvoie son empreinte SHA-256 (hexadécimal).
async fn download_file(
    http: &reqwest::Client,
    asset: &Asset,
    path: &std::path::Path,
    progress: &tauri::ipc::Channel<InstallProgress>,
) -> CmdResult<String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncWriteExt;

    let err = |e: &dyn std::fmt::Display| update_err(tr!("err.update.download", detail = e));
    let mut response = http.get(&asset.browser_download_url).send().await.map_err(|e| err(&e))?;
    if !response.status().is_success() {
        return Err(err(&response.status()));
    }
    let total = response.content_length().unwrap_or(asset.size);
    if total > MAX_DMG {
        return Err(update_err(tr!("err.update.tooLarge")));
    }
    let mut file = tokio::fs::File::create(path).await.map_err(|e| err(&e))?;
    let mut hasher = Sha256::new();
    let (mut done, mut last) = (0u64, 0u64);
    while let Some(chunk) = response.chunk().await.map_err(|e| err(&e))? {
        done += chunk.len() as u64;
        if done > MAX_DMG {
            return Err(update_err(tr!("err.update.tooLarge")));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(|e| err(&e))?;
        if done - last >= 1024 * 1024 || done == total {
            last = done;
            let _ = progress.send(InstallProgress::Download { done, total });
        }
    }
    file.sync_all().await.map_err(|e| err(&e))?;
    Ok(hex(&hasher.finalize()))
}

/// Lance un outil du système (chemin absolu) et renvoie sa sortie, ou une erreur.
fn run(program: &str, args: &[&std::ffi::OsStr]) -> CmdResult<String> {
    run_with_input(program, args, b"")
}

/// Comme [`run`], avec `input` envoyé sur l'entrée standard.
fn run_with_input(program: &str, args: &[&std::ffi::OsStr], input: &[u8]) -> CmdResult<String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let err = |e: std::io::Error| update_err(tr!("err.update.install", detail = e));
    let mut child = Command::new(program)
        .args(args)
        // hdiutil affiche la licence avec un « pager » : sans attente de touche.
        .env("PAGER", "cat")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(err)?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input);
    }
    let out = child.wait_with_output().map_err(err)?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if out.status.success() {
        Ok(text)
    } else {
        Err(update_err(tr!("err.update.install", detail = format!("{program}: {}", error_summary(&text)))))
    }
}

/// Fin de la sortie d'un outil en échec (dernières lignes, 300 caractères au plus) :
/// le message d'erreur y figure, pas le reste (une licence, par exemple).
fn error_summary(output: &str) -> String {
    let lines: Vec<&str> = output.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let tail = lines[lines.len().saturating_sub(3)..].join(" ");
    match tail.char_indices().rev().nth(299) {
        Some((i, _)) => format!("…{}", &tail[i..]),
        None => tail,
    }
}

/// Monte le `.dmg` en lecture seule, copie l'application à côté de l'actuelle (même
/// volume, pour un remplacement par renommage), puis démonte.
fn stage(dmg: &std::path::Path, dir: &std::path::Path, bundle: &std::path::Path) -> CmdResult<std::path::PathBuf> {
    let app_name = format!("{}.app", crate::identity::NAME);
    let parent = bundle.parent().ok_or_else(|| update_err(tr!("err.update.notBundle")))?;
    // Dossier modifiable ? (/Applications demande un compte administrateur ; une
    // application lancée sans avoir été déplacée est en lecture seule.)
    let probe = parent.join(format!(".{app_name}.probe"));
    std::fs::write(&probe, b"").map_err(|_| update_err(tr!("err.update.notWritable", path = parent.display())))?;
    let _ = std::fs::remove_file(&probe);

    let mount = dir.join("mnt");
    create_private_dir(&mount).map_err(|e| update_err(tr!("err.update.install", detail = e)))?;
    // Le .dmg affiche la licence (GPL) à l'ouverture et demande de l'accepter : réponse
    // « Y » envoyée sur l'entrée standard, comme un clic sur « Accepter » dans le Finder.
    run_with_input(
        "/usr/bin/hdiutil",
        &["attach".as_ref(), "-nobrowse".as_ref(), "-readonly".as_ref(), "-noautoopen".as_ref(), "-mountpoint".as_ref(), mount.as_os_str(), dmg.as_os_str()],
        b"Y\n",
    )?;
    let staged = parent.join(format!(".{app_name}.update"));
    let _ = std::fs::remove_dir_all(&staged);
    let copied = run("/usr/bin/ditto", &[mount.join(&app_name).as_os_str(), staged.as_os_str()]);
    let _ = run("/usr/bin/hdiutil", &["detach".as_ref(), "-force".as_ref(), mount.as_os_str()]);
    match copied {
        Ok(_) => Ok(staged),
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staged);
            Err(e)
        }
    }
}

/// Équipe Developer ID (« TeamIdentifier ») d'une application signée.
fn team_id(app: &std::path::Path) -> Option<String> {
    let out = run("/usr/bin/codesign", &["-dv".as_ref(), app.as_os_str()]).ok()?;
    let id = out.lines().find_map(|l| l.strip_prefix("TeamIdentifier="))?.trim().to_owned();
    (id.len() == 10 && id.bytes().all(|b| b.is_ascii_alphanumeric())).then_some(id)
}

/// Signature de la même équipe que l'application en cours, notarisation acceptée par
/// Gatekeeper, version attendue. Supprime la copie si un contrôle échoue.
fn verify(staged: &std::path::Path, bundle: &std::path::Path, version: &str) -> CmdResult<()> {
    let checked = (|| {
        let team = team_id(bundle).ok_or_else(|| update_err(tr!("err.update.unsigned")))?;
        let requirement = format!("=anchor apple generic and certificate leaf[subject.OU] = \"{team}\"");
        run(
            "/usr/bin/codesign",
            &["--verify".as_ref(), "--deep".as_ref(), "--strict".as_ref(), "-R".as_ref(), requirement.as_ref(), staged.as_os_str()],
        )
        .map_err(|_| update_err(tr!("err.update.signature")))?;
        run("/usr/sbin/spctl", &["--assess".as_ref(), "--type".as_ref(), "execute".as_ref(), staged.as_os_str()])
            .map_err(|_| update_err(tr!("err.update.signature")))?;
        let plist = staged.join("Contents/Info.plist");
        let found = run("/usr/libexec/PlistBuddy", &["-c".as_ref(), "Print :CFBundleShortVersionString".as_ref(), plist.as_os_str()])?;
        if found.trim() != version {
            return Err(update_err(tr!("err.update.wrongVersion", version = found.trim())));
        }
        Ok(())
    })();
    if checked.is_err() {
        let _ = std::fs::remove_dir_all(staged);
    }
    checked
}

/// Remplace l'application par la copie vérifiée ; l'ancienne est rétablie en cas d'échec.
fn swap(staged: &std::path::Path, bundle: &std::path::Path) -> CmdResult<()> {
    let err = |e: std::io::Error| update_err(tr!("err.update.install", detail = e));
    let old = bundle.with_file_name(format!(".{}.app.old", crate::identity::NAME));
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(bundle, &old).map_err(|e| {
        let _ = std::fs::remove_dir_all(staged);
        err(e)
    })?;
    if let Err(e) = std::fs::rename(staged, bundle) {
        let _ = std::fs::rename(&old, bundle);
        let _ = std::fs::remove_dir_all(staged);
        return Err(err(e));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v1.5.0"), Some((1, 5, 0)));
        assert_eq!(parse_version("1.10.2"), Some((1, 10, 2)));
        assert!(parse_version("v1.6.0-rc1").is_none());
        assert!(parse_version("v1.6").is_none());
        assert!(parse_version("v1.6.0.1").is_none());
        assert!(parse_version("1.10.0") > parse_version("1.9.9"));
    }

    #[test]
    fn error_summaries() {
        assert_eq!(error_summary("licence\n\nligne 2\nAgree Y/N?\nhdiutil: attach failed\n"), "ligne 2 Agree Y/N? hdiutil: attach failed");
        let long = "x".repeat(1000);
        let summary = error_summary(&long);
        assert!(summary.starts_with('…') && summary.chars().count() == 301);
    }

    #[test]
    fn checksums() {
        let h = "a".repeat(64);
        let sums = format!("{h}  Quayline-1.6.0-macos-arm64.dmg\n{}  autre.dmg\n", "b".repeat(64));
        assert_eq!(expected_sha256(&sums, "Quayline-1.6.0-macos-arm64.dmg"), Some(h.clone()));
        assert_eq!(expected_sha256(&format!("{h} *x.dmg"), "x.dmg"), Some(h));
        assert!(expected_sha256(&sums, "absent.dmg").is_none());
        assert!(expected_sha256("zz  x.dmg", "x.dmg").is_none());
    }

    #[test]
    fn assets() {
        let base = format!("https://github.com/{}/releases/download/v1.6.0/", crate::identity::REPOSITORY);
        let asset = |name: &str, url: String| Asset { name: name.into(), browser_download_url: url, size: 1 };
        let dmg = format!("{}-1.6.0-macos-arm64.dmg", crate::identity::NAME);
        let ok = [asset(&dmg, format!("{base}{dmg}")), asset("SHA256SUMS.macos", format!("{base}SHA256SUMS.macos"))];
        assert!(macos_assets(&ok, "1.6.0").is_some());
        assert!(macos_assets(&ok, "1.7.0").is_none());
        let foreign = [asset(&dmg, format!("https://evil.example/{dmg}")), ok[1].clone()];
        assert!(macos_assets(&foreign, "1.6.0").is_none());
    }

    #[test]
    fn release_urls() {
        let base = format!("https://github.com/{}/releases/", crate::identity::REPOSITORY);
        assert!(check_release_url(&format!("{base}tag/v1.6.0")));
        assert!(check_release_url(&format!("{}tag/v1.6.0", base.to_uppercase().replace("HTTPS://GITHUB.COM", "https://github.com"))));
        assert!(!check_release_url(&base));
        assert!(!check_release_url("https://github.com/someone/else/releases/tag/v9.9.9"));
        assert!(!check_release_url(&format!("https://evil.example/{base}")));
        assert!(!check_release_url(&format!("{base}tag/v1.6.0?x=1")));
    }
}
