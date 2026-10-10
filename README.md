# Quayline

**[English](#english)** · **[Français](#français)**

## English

A security-focused SSH and SFTP client for Linux (Fedora, KDE Plasma and GNOME) and
macOS. Servers, credentials and keys are kept in an encrypted vault, on your machine
only.

The interface is available in English and French (Settings → Appearance → Language).

### Features

- **Tabbed SSH terminal** and dual-pane **SFTP browser** (drag and drop, background
  transfers); each one opens from the other.
- **Encrypted vault** for servers, passwords and SSH keys, with encrypted export and
  import for backups or moving to another computer.
- **Unlocking** with the master password, a **FIDO2 security key** (YubiKey,
  Nitrokey…) or an **OpenID Connect identity provider** (Pocket ID; Authentik,
  Authelia, Keycloak, Microsoft Entra ID and Google in beta), including from a phone
  via a QR code.
- **Automatic lock** after inactivity, when the session or screen is locked, and
  before sleep.
- **Host key** verification (blocking alert if it changes), password copy with
  automatic clipboard clearing, light or dark theme.

### Security

The full model is described in [docs/securite.md](docs/securite.md) (in French). In
short:

- **Vault**: master password → Argon2id (256 MiB, 3 passes) → key protecting a random
  data key → XChaCha20-Poly1305. Everything is encrypted, including the server list;
  any tampering with the file is detected.
- **Memory**: secrets are wiped after use, core dumps are disabled, the process cannot
  be traced.
- **Isolated interface**: no secret is ever sent back to the web interface; strict
  content security policy, no remote resources.
- **Signed packages**: the RPM and the Flatpak come with checksums signed with the
  project key ([packaging/signing-key.asc](packaging/signing-key.asc)); the macOS app is
  signed and notarized by Apple.

To report a vulnerability, see [SECURITY.md](SECURITY.md).

> ⚠️ The master password **cannot be recovered**. Back up the vault file (it is
> encrypted) or use the encrypted export, and keep the password somewhere safe.

### Install

#### Fedora

From the latest [release](https://github.com/Meme-Pas-Dev/quayline/releases), download
the RPM (`Quayline-X.Y.Z-N.x86_64.rpm`), `SHA256SUMS` and `SHA256SUMS.asc`, then check
the signature and checksum before installing:

```bash
curl -fsSLO https://raw.githubusercontent.com/Meme-Pas-Dev/quayline/main/packaging/signing-key.asc
gpg --dearmor < signing-key.asc > quayline.gpg
gpgv --keyring ./quayline.gpg SHA256SUMS.asc SHA256SUMS   # checksum signature
sha256sum -c --ignore-missing SHA256SUMS                    # package checksum
sudo dnf install ./Quayline-*.x86_64.rpm
```

To update, follow the same steps with the new release (`dnf` replaces the previous
version and keeps the vault).

The Flatpak is checked the same way with `SHA256SUMS.flatpak` and
`SHA256SUMS.flatpak.asc`, then installed with `flatpak install --user ./quayline.flatpak`
(GNOME runtime from Flathub). Its vault is stored in `~/.var/app/dev.memepas.quayline/`.

#### macOS (Apple Silicon)

Download `Quayline-X.Y.Z-macos-arm64.dmg` from the
[releases](https://github.com/Meme-Pas-Dev/quayline/releases), then drag Quayline into
Applications. The app is signed (Developer ID) and notarized by Apple. The vault is
stored in `~/Library/Application Support/dev.memepas.quayline/`.

### Usage

- **Identity provider unlock**: Settings → Identity provider (OIDC). The instructions
  adapt to the chosen provider; the master password remains valid at all times.
- **FIDO2 security keys**: Settings → Security keys → Add a key. No secret is stored:
  the key recomputes it at each unlock. Register a second key as a backup.
- **Shortcuts**: `Ctrl+Shift+L` (`Cmd+Shift+L` on macOS) locks;
  `Ctrl+Shift+C` / `Ctrl+Shift+V` copy and paste in the terminal. Right-click a session
  or a server for other actions.

### Build

```bash
# Fedora: system dependencies
sudo dnf install webkit2gtk4.1-devel openssl-devel librsvg2-devel systemd-devel nodejs
sudo dnf group install c-development
# Rust: https://rustup.rs

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

### License

Quayline is free software, distributed under the
[GNU GPL version 3 or later](LICENSE). Any modified version that is redistributed
must be released under the same license, with its source code.

---

## Français

Client SSH et SFTP pour Linux (Fedora, KDE Plasma et GNOME) et macOS, pensé pour la
sécurité. Serveurs, identifiants et clés sont rangés dans un coffre chiffré, sur votre
machine uniquement.

L'interface est en français et en anglais (Réglages → Apparence → Langue).

### Fonctionnalités

- **Terminal SSH** à onglets, **explorateur SFTP** en double panneau (glisser-déposer,
  transferts en arrière-plan), ouverture de l'un depuis l'autre.
- **Coffre chiffré** pour les serveurs, mots de passe et clés SSH, avec export et import
  chiffrés pour la sauvegarde ou le passage à un autre ordinateur.
- **Déverrouillage** par mot de passe maître, par **clé de sécurité FIDO2** (YubiKey,
  Nitrokey…) ou par un **fournisseur d'identité OpenID Connect** (Pocket ID ; Authentik,
  Authelia, Keycloak, Microsoft Entra ID et Google en bêta), y compris depuis un
  téléphone par QR code.
- **Verrouillage automatique** après inactivité, au verrouillage de la session ou de
  l'écran, avant la mise en veille.
- Vérification des **clés d'hôte** (alerte bloquante si elle change), copie des mots de
  passe avec effacement automatique du presse-papiers, thème clair ou sombre.

### Sécurité

Le modèle complet est décrit dans [docs/securite.md](docs/securite.md). En bref :

- **Coffre** : mot de passe maître → Argon2id (256 Mio, 3 passes) → clé qui protège une
  clé de données aléatoire → XChaCha20-Poly1305. Tout est chiffré, y compris la liste des
  serveurs ; toute altération du fichier est détectée.
- **Mémoire** : secrets effacés après usage, core dumps désactivés, processus non
  traçable.
- **Interface isolée** : aucun secret ne repart vers l'interface web ; politique de
  sécurité stricte, aucune ressource distante.
- **Paquets signés** : le RPM et le Flatpak sont accompagnés d'empreintes signées avec la
  clé du projet ([packaging/signing-key.asc](packaging/signing-key.asc)) ; l'application
  macOS est signée et notarisée par Apple.

Pour signaler une faille : voir [SECURITY.md](SECURITY.md).

> ⚠️ Le mot de passe maître est **irrécupérable**. Sauvegardez le fichier du coffre (il
> est chiffré) ou utilisez l'export chiffré, et gardez le mot de passe en lieu sûr.

### Installer

#### Fedora

Télécharger depuis la dernière [release](https://github.com/Meme-Pas-Dev/quayline/releases)
le RPM (`Quayline-X.Y.Z-N.x86_64.rpm`), `SHA256SUMS` et `SHA256SUMS.asc`, puis vérifier
la signature et l'empreinte avant d'installer :

```bash
curl -fsSLO https://raw.githubusercontent.com/Meme-Pas-Dev/quayline/main/packaging/signing-key.asc
gpg --dearmor < signing-key.asc > quayline.gpg
gpgv --keyring ./quayline.gpg SHA256SUMS.asc SHA256SUMS   # signature des empreintes
sha256sum -c --ignore-missing SHA256SUMS                    # empreinte du paquet
sudo dnf install ./Quayline-*.x86_64.rpm
```

Pour une mise à jour, même procédure avec la nouvelle release (`dnf` remplace
l'ancienne version et garde le coffre).

Le Flatpak se vérifie de la même façon avec `SHA256SUMS.flatpak` et
`SHA256SUMS.flatpak.asc`, puis s'installe avec `flatpak install --user ./quayline.flatpak`
(runtime GNOME de Flathub). Son coffre est rangé dans `~/.var/app/dev.memepas.quayline/`.

#### macOS (Apple Silicon)

Télécharger `Quayline-X.Y.Z-macos-arm64.dmg` dans les [releases](https://github.com/Meme-Pas-Dev/quayline/releases), puis
glisser Quayline dans Applications. L'application est signée (Developer ID) et
notarisée par Apple. Le coffre est rangé dans
`~/Library/Application Support/dev.memepas.quayline/`.

### Utilisation

- **Déverrouillage par fournisseur d'identité** : Réglages → Fournisseur d'identité
  (OIDC). La marche à suivre s'adapte au fournisseur choisi ; le mot de passe maître
  reste valable à tout moment.
- **Clés de sécurité FIDO2** : Réglages → Clés de sécurité → Ajouter une clé. Aucun
  secret n'est stocké : il est recalculé par la clé à chaque déverrouillage.
  Enregistrez une seconde clé en secours.
- **Raccourcis** : `Ctrl+Maj+L` (`Cmd+Maj+L` sur macOS) verrouille ;
  `Ctrl+Maj+C` / `Ctrl+Maj+V` copient et collent dans le terminal. Clic droit sur une
  session ou un serveur pour les autres actions.

### Compiler

```bash
# Fedora : dépendances système
sudo dnf install webkit2gtk4.1-devel openssl-devel librsvg2-devel systemd-devel nodejs
sudo dnf group install c-development
# Rust : https://rustup.rs

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

### Licence

Quayline est un logiciel libre, distribué sous licence
[GNU GPL version 3 ou ultérieure](LICENSE). Toute version modifiée et redistribuée doit
l'être sous la même licence, avec son code source.
