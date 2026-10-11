# Changelog · Journal des modifications

Each version is described in English, then in French. The section of a version is
used as the text of its GitHub release.

Chaque version est décrite en anglais, puis en français. La section d'une version sert
de texte à sa release GitHub.

## [1.7.1] - 2026-10-11

### English

**Fixed**
- macOS: installing an update from the application failed, because the `.dmg` asks to
  accept the license when opened.
- Update notice: centered buttons, shorter error messages.

### Français

**Corrigé**
- macOS : l'installation d'une mise à jour depuis l'application échouait, le `.dmg`
  demandant d'accepter la licence à l'ouverture.
- Encart de mise à jour : boutons centrés, messages d'erreur raccourcis.

## [1.7.0] - 2026-10-11

### English

**Added**
- Windows 11 (preview): installer for the current user (no administrator account) or
  for all users. Identity provider key in the Windows Credential Manager, vault locked
  with the session (Win+L) and before sleep, copied passwords kept out of the Win+V
  history. Security keys (FIDO2) are not available on Windows yet.
- File browser: drag and drop between the local and remote panes now also works on
  Windows.

**Changed**
- Linux: desktop entry and AppStream metadata in English with a French translation,
  declaring both supported languages. The RPM packages (GitHub release and COPR) now
  include the AppStream metadata, so software centers such as Discover can list the
  application.

### Français

**Ajouté**
- Windows 11 (préversion) : installateur pour l'utilisateur actif (sans compte
  administrateur) ou pour tous. Clé du fournisseur d'identité dans le Gestionnaire
  d'identifiants de Windows, coffre verrouillé avec la session (Win+L) et avant la mise
  en veille, mots de passe copiés exclus de l'historique Win+V. Les clés de sécurité
  (FIDO2) ne sont pas encore disponibles sous Windows.
- Explorateur de fichiers : le glisser-déposer entre les panneaux local et distant
  fonctionne aussi sous Windows.

**Modifié**
- Linux : entrée du menu et métadonnées AppStream en anglais avec la traduction
  française, langues prises en charge déclarées. Les paquets RPM (release GitHub et
  COPR) incluent maintenant les métadonnées AppStream, pour que les logithèques comme
  Discover listent l'application.

## [1.6.1] - 2026-10-10

### English

**Fixed**
- Button labels no longer wrap onto two lines (update question, dialogs).

**Changed**
- Simpler wording for the update question shown after unlocking.

### Français

**Corrigé**
- Les libellés des boutons ne passent plus sur deux lignes (question des mises à jour,
  fenêtres de dialogue).

**Modifié**
- Question des mises à jour, posée après le déverrouillage, formulée plus simplement.

## [1.6.0] - 2026-10-10

### English

**Added**
- Update check: after unlocking (at most every 12 hours) or on demand in
  Settings → About. A new version is announced in the sidebar, with “View” and “Skip
  this version”. The choice to check automatically is asked once, after unlocking.
- macOS: install updates from the application. After your consent, the `.dmg` is
  downloaded and checked (checksum, Apple signature from the same team, Gatekeeper,
  version) before replacing the application, then Quayline restarts.
- Fedora: COPR repository `matt-mpd/quayline`, with updates through `dnf upgrade` or
  Discover.

### Français

**Ajouté**
- Recherche des mises à jour : après le déverrouillage (au plus toutes les 12 heures)
  ou à la demande dans Réglages → À propos. Une nouvelle version est signalée dans la
  barre latérale, avec « Voir » et « Ignorer cette version ». Le choix de la recherche
  automatique est demandé une fois, après le déverrouillage.
- macOS : installation des mises à jour depuis l'application. Après accord, le `.dmg`
  est téléchargé et vérifié (empreinte, signature Apple de la même équipe, Gatekeeper,
  version) avant de remplacer l'application, puis Quayline redémarre.
- Fedora : dépôt COPR `matt-mpd/quayline`, mises à jour par `dnf upgrade` ou Discover.

## [1.5.0] - 2026-10-10

### English

**Added**
- English interface, alongside French. The language follows the system by default
  (English if the system language is not supported) and can be changed in
  Settings → Appearance, without restarting. Messages from the backend (errors, browser
  page after an identity provider sign-in, macOS menu) follow the same language.
- Bilingual README and security policy.

**Changed**
- Phone sign-in: the verification address is shown on its own line.

### Français

**Ajouté**
- Interface en anglais, en plus du français. La langue suit celle du système par
  défaut (anglais si elle n'est pas prise en charge) et se change dans
  Réglages → Apparence, sans redémarrer. Les messages du backend (erreurs, page du
  navigateur après une connexion par fournisseur d'identité, menu macOS) suivent la
  même langue.
- README et politique de sécurité bilingues.

**Modifié**
- Connexion par téléphone : l'adresse de validation est affichée sur sa propre ligne.

## [1.4.1] - 2026-10-10

### English

- First public version: GPL-3.0-or-later license, public repository, signed packages
  (RPM, Flatpak) and signed, notarized macOS application.

### Français

- Première version publique : licence GPL-3.0 ou ultérieure, dépôt public, paquets
  signés (RPM, Flatpak) et application macOS signée et notarisée.
