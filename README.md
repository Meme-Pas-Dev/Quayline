# Quayline

**[English](#english)** · **[Français](#français)**

## English

A security-focused SSH and SFTP client for Linux (Fedora, KDE Plasma and GNOME),
macOS and Windows 11 (preview). Servers, credentials and keys are kept in an encrypted vault, on your machine
only.

The interface is available in English and French (Settings → General → Language).

Website: **[memepas.dev/en/quayline](https://memepas.dev/en/quayline/)** (screenshots,
downloads for your system).

### Features

- **Tabbed SSH terminal** and dual-pane **SFTP browser** (drag and drop, background
  transfers, permissions); each one opens from the other. Remote files can be edited in
  a built-in editor or in the system's default application, changes being sent back to
  the server.
- **Local terminal** in a tab, next to SSH sessions (RPM, macOS and Windows; not in the
  Flatpak, which stays isolated from the system).
- **Encrypted vault** for servers, passwords and SSH keys, with encrypted export and
  import for backups or moving to another computer.
- **Unlocking** with the master password, a **FIDO2 security key** (YubiKey,
  Nitrokey…) or an **OpenID Connect identity provider** (Pocket ID; Authentik,
  Authelia, Keycloak, Microsoft Entra ID and Google in beta), including from a phone
  via a QR code. The provider asks you to sign in again once a grace period has passed
  (15 minutes by default, adjustable).
- **Automatic lock** after inactivity, when the session or screen is locked, and
  before sleep.
- **Host key** verification (blocking alert if it changes), password copy with
  automatic clipboard clearing (excluded from clipboard history), light or dark theme.
- **Updates**: checking for a new version (asked once, can be turned off); on macOS,
  installation from the app.

### Security

The full model is described in [docs/securite.md](docs/securite.md) (in French). In
short:

- **Vault**: master password → Argon2id (256 MiB, 3 passes) → key protecting a random
  data key → XChaCha20-Poly1305. Everything is encrypted, including the server list;
  any tampering with the file is detected. Only what the lock screen needs stays
  readable without the password: the identity provider's address and account name, and
  the names given to security keys.
- **Memory**: the data key and secrets are wiped when the vault is locked, core dumps
  are disabled, the process cannot be traced.
- **One copy at a time**: a second launch brings the open window to the front, and the
  vault file is reserved for the copy that unlocked it: two copies cannot overwrite
  each other's changes.
- **Isolated interface**: no secret is ever sent back to the web interface; strict
  content security policy, no remote resources.
- **Signed packages**: the RPM, the Flatpak and the Windows installer come with
  checksums signed with the project key
  ([packaging/signing-key.asc](packaging/signing-key.asc)); COPR packages are signed by
  Fedora's infrastructure; the macOS app is signed and notarized by Apple. Signing keys
  are only available to builds of version tags.

To report a vulnerability, see [SECURITY.md](SECURITY.md).

> ⚠️ The master password **cannot be recovered**. Back up the vault file (it is
> encrypted) or use the encrypted export, and keep the password somewhere safe.

### Install

#### Fedora

The simplest way is the COPR repository: packages built and signed by Fedora's
infrastructure, updated with the rest of the system (`sudo dnf upgrade`). Quayline does
not appear in the Discover or GNOME Software catalog yet: install it from a terminal.

```bash
sudo dnf copr enable matt-mpd/quayline
sudo dnf install quayline
```

Or install the RPM by hand: from the latest [release](https://github.com/Meme-Pas-Dev/Quayline/releases), download
the RPM (`Quayline-X.Y.Z-N.x86_64.rpm`), `SHA256SUMS` and `SHA256SUMS.asc`, then check
the signature and checksum before installing:

```bash
curl -fsSLO https://raw.githubusercontent.com/Meme-Pas-Dev/Quayline/main/packaging/signing-key.asc
gpg --dearmor < signing-key.asc > quayline.gpg
gpgv --keyring ./quayline.gpg SHA256SUMS.asc SHA256SUMS   # checksum signature
sha256sum -c --ignore-missing SHA256SUMS                    # package checksum
sudo dnf install ./Quayline-*.x86_64.rpm
```

For a manual install, update with the same steps and the new release (`dnf` replaces the previous
version and keeps the vault).

The Flatpak is checked the same way with `SHA256SUMS.flatpak` and
`SHA256SUMS.flatpak.asc`, then installed with `flatpak install --user ./quayline.flatpak`
(GNOME runtime from Flathub). It is not updated automatically: the app announces new
versions, to install the same way (Flathub is planned). Its vault is stored in
`~/.var/app/dev.memepas.quayline/`.

#### macOS (Apple Silicon)

Download `Quayline-X.Y.Z-macos-arm64.dmg` from the
[releases](https://github.com/Meme-Pas-Dev/Quayline/releases), then drag Quayline into
Applications. The app is signed (Developer ID) and notarized by Apple, and installs
its updates itself once you agree. The vault is stored in
`~/Library/Application Support/dev.memepas.quayline/`.

#### Windows 11 (preview)

Download `Quayline-X.Y.Z-windows-x64-setup.exe` from the
[releases](https://github.com/Meme-Pas-Dev/Quayline/releases) and run it. The installer
offers to install for the current user only (no administrator account needed) or for
all users. It is not signed yet: Windows SmartScreen shows “Windows protected your PC”
(“More info” → “Run anyway”). You can check it against `SHA256SUMS.windows`, signed
with the project key (`SHA256SUMS.windows.asc`):

```powershell
Get-FileHash .\Quayline-*-windows-x64-setup.exe   # compare with SHA256SUMS.windows
```

Security keys (FIDO2) go through Windows' own window (key choice, PIN, touch). The
vault is stored in `%APPDATA%\dev.memepas.quayline\`.

### Usage

- **Identity provider unlock**: Settings → Security → Identity provider (OIDC). The
  instructions adapt to the chosen provider; the master password remains valid at all
  times.
- **FIDO2 security keys**: Settings → Security → Security keys → Add a key. No secret is stored:
  the key recomputes it at each unlock. Register a second key as a backup.
- **Backup**: Settings → Data → Backup and transfer (encrypted export and import).
- **Reset**: Settings → Data → Reset Quayline erases all data (vault open, master
  password required). Make an encrypted export first.
- **Forgotten master password**: the vault cannot be recovered. To start over, quit
  Quayline and delete its data folder:

  | System | Folder |
  |---|---|
  | Fedora (RPM, COPR) | `~/.local/share/dev.memepas.quayline` |
  | Flatpak | `~/.var/app/dev.memepas.quayline` |
  | macOS | `~/Library/Application Support/dev.memepas.quayline` |
  | Windows | `%APPDATA%\dev.memepas.quayline` |

  If identity provider unlock was enabled, also delete the Quayline entry from the
  system keychain (GNOME Keyring / KWallet, Keychain Access, Credential Manager).
- **Shortcuts**: `Ctrl+Shift+L` (`Cmd+Shift+L` on macOS) locks; `Ctrl+Shift+T`
  (`Cmd+T` on macOS) opens a local terminal;
  `Ctrl+Shift+C` / `Ctrl+Shift+V` (`Cmd+C` / `Cmd+V` on macOS) copy and paste in the
  terminal. Right-click a session
  or a server for other actions.

### Build

```bash
# Fedora: system dependencies
sudo dnf install webkit2gtk4.1-devel openssl-devel librsvg2-devel systemd-devel nodejs
sudo dnf group install c-development
# Rust 1.90 or later: https://rustup.rs

npm ci
npm run tauri dev                      # development run
npm run tauri build -- --bundles rpm   # RPM package in target/release/bundle/rpm/
```

Tests:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
npm run check
```

Code layout:

```
crates/vault/   Vault: format, cryptography, data model
src-tauri/      Tauri application: commands, state, process hardening
src/            Interface (Svelte, TypeScript)
```

Translations live in `src/lib/i18n/en.json` and `fr.json`, shared by the interface and
the backend; `npm run check` verifies that both catalogs match.

### Code signing policy

Windows installers are built by GitHub Actions from this repository, at each version
tag; nothing is built or signed on a personal computer. Windows installers are not
code-signed yet (free signing from the SignPath Foundation is planned); their
checksums are signed with the project GPG key.

- Committers and reviewers: members of the
  [Meme-Pas-Dev](https://github.com/Meme-Pas-Dev) organization.
- Approvers: the owner of the Meme-Pas-Dev organization, who approves each release.
- Every release is built from a tag of this repository; macOS builds are signed with an
  Apple Developer ID, Linux packages and Windows checksums with the project GPG key
  ([packaging/signing-key.asc](packaging/signing-key.asc)).

**Privacy**: Quayline sends no telemetry. It only connects to the servers and identity
provider you configure and, if you allow it (asked once), to GitHub to check for a new
version; on macOS, an update is downloaded from GitHub only after your consent.

### License

Quayline is free software, distributed under the
[GNU GPL version 3 or later](LICENSE). Any modified version that is redistributed
must be released under the same license, with its source code.

---

## Français

Client SSH et SFTP pour Linux (Fedora, KDE Plasma et GNOME), macOS et Windows 11
(préversion), pensé pour la sécurité. Serveurs, identifiants et clés sont rangés dans un coffre chiffré, sur votre
machine uniquement.

L'interface est en français et en anglais (Réglages → Général → Langue).

Site : **[memepas.dev/fr/quayline](https://memepas.dev/fr/quayline/)** (captures d'écran,
téléchargement pour votre système).

### Fonctionnalités

- **Terminal SSH** à onglets, **explorateur SFTP** en double panneau (glisser-déposer,
  transferts en arrière-plan, permissions), ouverture de l'un depuis l'autre. Les
  fichiers distants se modifient dans un éditeur intégré ou dans l'application par
  défaut du système, les changements étant renvoyés au serveur.
- **Terminal local** dans un onglet, à côté des sessions SSH (RPM, macOS et Windows ;
  pas dans le Flatpak, qui reste isolé du système).
- **Coffre chiffré** pour les serveurs, mots de passe et clés SSH, avec export et import
  chiffrés pour la sauvegarde ou le passage à un autre ordinateur.
- **Déverrouillage** par mot de passe maître, par **clé de sécurité FIDO2** (YubiKey,
  Nitrokey…) ou par un **fournisseur d'identité OpenID Connect** (Pocket ID ; Authentik,
  Authelia, Keycloak, Microsoft Entra ID et Google en bêta), y compris depuis un
  téléphone par QR code. Le fournisseur redemande de se connecter passé un délai de
  grâce (15 minutes par défaut, réglable).
- **Verrouillage automatique** après inactivité, au verrouillage de la session ou de
  l'écran, avant la mise en veille.
- Vérification des **clés d'hôte** (alerte bloquante si elle change), copie des mots de
  passe avec effacement automatique du presse-papiers (exclue de l'historique du
  presse-papiers), thème clair ou sombre.
- **Mises à jour** : recherche d'une nouvelle version (question posée une fois,
  désactivable) ; sur macOS, installation depuis l'application.

### Sécurité

Le modèle complet est décrit dans [docs/securite.md](docs/securite.md). En bref :

- **Coffre** : mot de passe maître → Argon2id (256 Mio, 3 passes) → clé qui protège une
  clé de données aléatoire → XChaCha20-Poly1305. Tout est chiffré, y compris la liste des
  serveurs ; toute altération du fichier est détectée. Seul ce dont l'écran de
  verrouillage a besoin reste lisible sans le mot de passe : l'adresse et le nom du compte
  du fournisseur d'identité, et les noms donnés aux clés de sécurité.
- **Mémoire** : clé de données et secrets effacés au verrouillage du coffre, core dumps
  désactivés, processus non traçable.
- **Une seule copie à la fois** : un second lancement ramène la fenêtre ouverte au
  premier plan, et le fichier du coffre est réservé à la copie qui l'a déverrouillé :
  deux copies ne peuvent pas écraser les modifications l'une de l'autre.
- **Interface isolée** : aucun secret ne repart vers l'interface web ; politique de
  sécurité stricte, aucune ressource distante.
- **Paquets signés** : le RPM, le Flatpak et l'installateur Windows sont accompagnés
  d'empreintes signées avec la clé du projet
  ([packaging/signing-key.asc](packaging/signing-key.asc)) ; les paquets COPR sont signés
  par l'infrastructure de Fedora ; l'application macOS est signée et notarisée par Apple.
  Les clés de signature ne sont accessibles qu'aux builds des étiquettes de version.

Pour signaler une faille : voir [SECURITY.md](SECURITY.md).

> ⚠️ Le mot de passe maître est **irrécupérable**. Sauvegardez le fichier du coffre (il
> est chiffré) ou utilisez l'export chiffré, et gardez le mot de passe en lieu sûr.

### Installer

#### Fedora

Le plus simple est le dépôt COPR : paquets compilés et signés par l'infrastructure de
Fedora, mis à jour avec le reste du système (`sudo dnf upgrade`). Quayline n'apparaît
pas encore dans le catalogue de Discover ni de GNOME Logiciels : l'installer depuis un
terminal.

```bash
sudo dnf copr enable matt-mpd/quayline
sudo dnf install quayline
```

Ou installer le RPM à la main : télécharger depuis la dernière [release](https://github.com/Meme-Pas-Dev/Quayline/releases)
le RPM (`Quayline-X.Y.Z-N.x86_64.rpm`), `SHA256SUMS` et `SHA256SUMS.asc`, puis vérifier
la signature et l'empreinte avant d'installer :

```bash
curl -fsSLO https://raw.githubusercontent.com/Meme-Pas-Dev/Quayline/main/packaging/signing-key.asc
gpg --dearmor < signing-key.asc > quayline.gpg
gpgv --keyring ./quayline.gpg SHA256SUMS.asc SHA256SUMS   # signature des empreintes
sha256sum -c --ignore-missing SHA256SUMS                    # empreinte du paquet
sudo dnf install ./Quayline-*.x86_64.rpm
```

Pour une installation manuelle, la mise à jour suit la même procédure avec la nouvelle release (`dnf` remplace
l'ancienne version et garde le coffre).

Le Flatpak se vérifie de la même façon avec `SHA256SUMS.flatpak` et
`SHA256SUMS.flatpak.asc`, puis s'installe avec `flatpak install --user ./quayline.flatpak`
(runtime GNOME de Flathub). Il ne se met pas à jour tout seul : l'application annonce
les nouvelles versions, à installer de la même façon (Flathub est prévu). Son coffre est
rangé dans `~/.var/app/dev.memepas.quayline/`.

#### macOS (Apple Silicon)

Télécharger `Quayline-X.Y.Z-macos-arm64.dmg` dans les [releases](https://github.com/Meme-Pas-Dev/Quayline/releases), puis
glisser Quayline dans Applications. L'application est signée (Developer ID) et
notarisée par Apple, et installe elle-même ses mises à jour après votre accord. Le
coffre est rangé dans `~/Library/Application Support/dev.memepas.quayline/`.

#### Windows 11 (préversion)

Télécharger `Quayline-X.Y.Z-windows-x64-setup.exe` dans les
[releases](https://github.com/Meme-Pas-Dev/Quayline/releases) et le lancer.
L'installateur propose d'installer pour l'utilisateur actif seulement (sans compte
administrateur) ou pour tous les utilisateurs. Il n'est pas encore signé : Windows
SmartScreen affiche « Windows a protégé votre ordinateur » (« Informations
complémentaires » → « Exécuter quand même »). Il se vérifie avec `SHA256SUMS.windows`,
signé avec la clé du projet (`SHA256SUMS.windows.asc`) :

```powershell
Get-FileHash .\Quayline-*-windows-x64-setup.exe   # à comparer avec SHA256SUMS.windows
```

Les clés de sécurité (FIDO2) passent par la fenêtre de Windows (choix de la clé, code
PIN, toucher). Le coffre est rangé dans `%APPDATA%\dev.memepas.quayline\`.

### Utilisation

- **Déverrouillage par fournisseur d'identité** : Réglages → Sécurité → Fournisseur
  d'identité (OIDC). La marche à suivre s'adapte au fournisseur choisi ; le mot de passe
  maître reste valable à tout moment.
- **Clés de sécurité FIDO2** : Réglages → Sécurité → Clés de sécurité → Ajouter une clé. Aucun
  secret n'est stocké : il est recalculé par la clé à chaque déverrouillage.
  Enregistrez une seconde clé en secours.
- **Sauvegarde** : Réglages → Données → Sauvegarde et transfert (export et import
  chiffrés).
- **Réinitialisation** : Réglages → Données → Réinitialiser Quayline efface toutes les
  données (coffre ouvert, mot de passe maître exigé). Faites d'abord un export chiffré.
- **Mot de passe maître oublié** : le coffre ne peut pas être récupéré. Pour repartir de
  zéro, quittez Quayline et supprimez son dossier de données :

  | Système | Dossier |
  |---|---|
  | Fedora (RPM, COPR) | `~/.local/share/dev.memepas.quayline` |
  | Flatpak | `~/.var/app/dev.memepas.quayline` |
  | macOS | `~/Library/Application Support/dev.memepas.quayline` |
  | Windows | `%APPDATA%\dev.memepas.quayline` |

  Si le déverrouillage par fournisseur d'identité était activé, supprimez aussi l'entrée
  Quayline du trousseau du système (Trousseau GNOME / KWallet, Trousseau d'accès,
  Gestionnaire d'identifiants).
- **Raccourcis** : `Ctrl+Maj+L` (`Cmd+Maj+L` sur macOS) verrouille ; `Ctrl+Maj+T`
  (`Cmd+T` sur macOS) ouvre un terminal local ;
  `Ctrl+Maj+C` / `Ctrl+Maj+V` (`Cmd+C` / `Cmd+V` sur macOS) copient et collent dans le
  terminal. Clic droit sur une
  session ou un serveur pour les autres actions.

### Compiler

```bash
# Fedora : dépendances système
sudo dnf install webkit2gtk4.1-devel openssl-devel librsvg2-devel systemd-devel nodejs
sudo dnf group install c-development
# Rust 1.90 ou plus récent : https://rustup.rs

npm ci
npm run tauri dev                      # lancement en développement
npm run tauri build -- --bundles rpm   # paquet RPM dans target/release/bundle/rpm/
```

Tests :

```bash
cargo test --workspace
cargo clippy --workspace --all-targets
npm run check
```

Organisation du code :

```
crates/vault/   Coffre : format, cryptographie, modèle de données
src-tauri/      Application Tauri : commandes, état, durcissement du processus
src/            Interface (Svelte, TypeScript)
```

Les traductions sont dans `src/lib/i18n/en.json` et `fr.json`, communes à l'interface et
au backend ; `npm run check` vérifie que les deux catalogues concordent.

### Politique de signature du code

Les installateurs Windows sont construits par GitHub Actions à partir de ce dépôt, à
chaque étiquette de version ; rien n'est construit ni signé sur un ordinateur
personnel. Les installateurs Windows ne sont pas encore signés (une signature gratuite
par la SignPath Foundation est prévue) ; leurs empreintes sont signées avec la clé GPG
du projet.

- Contributeurs et relecteurs : membres de l'organisation
  [Meme-Pas-Dev](https://github.com/Meme-Pas-Dev).
- Approbateurs : le propriétaire de l'organisation Meme-Pas-Dev, qui approuve chaque
  version.
- Chaque version est construite depuis une étiquette de ce dépôt ; l'application macOS
  est signée avec un Developer ID Apple, les paquets Linux et les empreintes Windows
  avec la clé GPG du projet ([packaging/signing-key.asc](packaging/signing-key.asc)).

**Vie privée** : Quayline n'envoie aucune télémétrie. Il ne se connecte qu'aux serveurs
et au fournisseur d'identité que vous configurez et, si vous l'autorisez (question
posée une fois), à GitHub pour rechercher une nouvelle version ; sur macOS, une mise à
jour n'est téléchargée depuis GitHub qu'après votre accord.

### Licence

Quayline est un logiciel libre, distribué sous licence
[GNU GPL version 3 ou ultérieure](LICENSE). Toute version modifiée et redistribuée doit
l'être sous la même licence, avec son code source.
