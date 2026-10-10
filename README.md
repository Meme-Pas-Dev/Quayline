# Quayline

Client SSH et SFTP pour Linux (Fedora, KDE Plasma et GNOME) et macOS, pensé pour la
sécurité. Serveurs, identifiants et clés sont rangés dans un coffre chiffré, sur votre
machine uniquement.

*A security-focused SSH/SFTP client for Linux and macOS, with a locally encrypted
vault. The interface is in French.*

## Fonctionnalités

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

## Sécurité

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

## Installer

### Fedora

Les paquets sont publiés dans les [releases](https://github.com/Meme-Pas-Dev/quayline/releases). Le plus simple est le
script [`scripts/manage.sh`](scripts/manage.sh) : il affiche la version installée,
propose le RPM ou le Flatpak, vérifie la signature et l'empreinte du paquet, puis
installe, met à jour ou supprime. Le coffre n'est jamais effacé sans confirmation.

```bash
git clone https://github.com/Meme-Pas-Dev/quayline.git
quayline/scripts/manage.sh
```

[`scripts/install-latest.sh`](scripts/install-latest.sh) fait de même pour le RPM, sans
questions.

**À la main**, depuis une release :

```bash
gpg --dearmor < packaging/signing-key.asc > quayline.gpg
gpgv --keyring ./quayline.gpg SHA256SUMS.asc SHA256SUMS   # signature des empreintes
sha256sum -c --ignore-missing SHA256SUMS                    # empreinte du paquet
sudo dnf install ./Quayline-*.x86_64.rpm
```

Le Flatpak s'installe avec `flatpak install --user ./quayline.flatpak` (runtime GNOME
de Flathub). Son coffre est rangé dans `~/.var/app/dev.memepas.quayline/`.

### macOS (Apple Silicon)

Télécharger `Quayline-X.Y.Z-macos-arm64.dmg` dans les [releases](https://github.com/Meme-Pas-Dev/quayline/releases), puis
glisser Quayline dans Applications. L'application est signée (Developer ID) et
notarisée par Apple.

Avec Homebrew, qui compile l'application sur le Mac :

```bash
brew tap Meme-Pas-Dev/quayline
brew install quayline
```

## Utilisation

- **Déverrouillage par fournisseur d'identité** : Réglages → Fournisseur d'identité
  (OIDC). La marche à suivre s'adapte au fournisseur choisi ; le mot de passe maître
  reste valable à tout moment.
- **Clés de sécurité FIDO2** : Réglages → Clés de sécurité → Ajouter une clé. Aucun
  secret n'est stocké : il est recalculé par la clé à chaque déverrouillage.
  Enregistrez une seconde clé en secours.
- **Raccourcis** : `Ctrl+Maj+L` (`Cmd+Maj+L` sur macOS) verrouille ;
  `Ctrl+Maj+C` / `Ctrl+Maj+V` copient et collent dans le terminal. Clic droit sur une
  session ou un serveur pour les autres actions.

## Compiler

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

## Licence

Quayline est un logiciel libre, distribué sous licence
[GNU GPL version 3 ou ultérieure](LICENSE). Toute version modifiée et redistribuée doit
l'être sous la même licence, avec son code source.
