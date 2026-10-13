# Changelog · Journal des modifications

Each version is described in English, then in French. The section of a version is
used as the text of its GitHub release.

Chaque version est décrite en anglais, puis en français. La section d'une version sert
de texte à sa release GitHub.

## [1.8.3] - 2026-10-13

### English

**Security** (code audit, continued)
- Unlocking with an identity provider in the browser now requires signing in again
  once the grace period has passed (new setting “Sign in again to the identity
  provider”: 15 minutes by default, or at every unlock); within it, one click is
  enough, as before. If the provider does not report when you signed in, enabling it
  shows a warning.
- Importing a file: the host keys it contains are listed (server, type, fingerprint)
  and approved only if you tick the box; otherwise each fingerprint is checked at the
  first connection. Security settings the file would change are shown.
- SSH: host keys signed with SHA-1 (“ssh-rsa”) are no longer accepted, as in OpenSSH;
  a clear message explains it. RSA keys signed with SHA-2 still work.
- Names, paths and messages sent by a server are cleaned of more invisible characters
  that could disguise them, including in the file browser's path bar and in error
  messages.

**Changed**
- The window no longer freezes while the master password is being checked, and saving
  no longer blocks the interface.
- Terminal menu and some messages translated; on macOS, system menu items follow the
  system language.

### Français

**Sécurité** (audit du code, suite)
- Le déverrouillage par fournisseur d'identité dans le navigateur demande désormais de
  se reconnecter une fois le délai de grâce écoulé (nouveau réglage « Nouvelle
  connexion au fournisseur d'identité » : 15 minutes par défaut, ou à chaque
  déverrouillage) ; dans ce délai, un clic suffit, comme avant. Si le fournisseur
  n'indique pas l'heure de connexion, son activation affiche un avertissement.
- Import d'un fichier : les clés d'hôte qu'il contient sont listées (serveur, type,
  empreinte) et approuvées seulement si vous cochez la case ; sinon chaque empreinte est
  vérifiée à la première connexion. Les réglages de sécurité que le fichier modifierait
  sont affichés.
- SSH : les clés d'hôte signées en SHA-1 (« ssh-rsa ») ne sont plus acceptées, comme
  dans OpenSSH ; un message clair l'explique. Les clés RSA signées en SHA-2 restent
  utilisables.
- Les noms, chemins et messages envoyés par un serveur sont débarrassés de davantage de
  caractères invisibles qui pourraient les maquiller, y compris dans la barre de chemin
  de l'explorateur et dans les messages d'erreur.

**Modifié**
- La fenêtre ne se fige plus pendant la vérification du mot de passe maître, et les
  enregistrements ne bloquent plus l'interface.
- Menu du terminal et quelques messages traduits ; sous macOS, les éléments de menu du
  système suivent la langue du système.

## [1.8.2] - 2026-10-12

### English

**Security** (fixes following a code audit)
- Only one copy of the application runs at a time: launching it again brings the
  existing window to the front. The vault file is also reserved for the copy that
  unlocked it, and a change is no longer saved over a vault modified elsewhere in the
  meantime: two copies could undo each other's changes, including a password change
  made after losing a security key.
- Pasting into a terminal: control characters are removed from the pasted text. A
  crafted text could end “bracketed paste” mode early and run a command without
  confirmation.
- Windows: SFTP downloads refuse file names that Windows would interpret differently
  (drive letter such as `C:name`, `:` streams, device names such as `CON` or `NUL`,
  trailing dot or space). A malicious server could otherwise write outside the chosen
  folder.
- Enabling the identity provider with 15 security keys registered no longer writes a
  vault that cannot be opened again; a vault is never saved in a state the
  application would refuse to read.
- macOS: copied passwords are excluded from clipboard history managers (Maccy,
  Raycast, Paste, Alfred…), as on Linux and Windows.
- While the vault is locked, the application refuses terminal input, transfers and
  local file operations, even for sessions kept open behind the lock screen.
- Encrypted export: the file is written atomically and always with owner-only
  permissions, even when replacing an existing file.
- Hardening against hostile servers: size limits on identity provider and update
  responses, depth limit on recursive deletion, limit on the decryption cost of an
  imported private key, more robust reading of the browser's return after signing in.

**Fixed**
- Very long file names could not be downloaded (temporary name too long).
- Minimum password length now counted the same way in the window and in the vault.
- Fedora package: icon installed in the standard 256×256 folder.

### Français

**Sécurité** (corrections à la suite d'un audit du code)
- Une seule copie de l'application à la fois : la relancer ramène la fenêtre existante
  au premier plan. Le fichier du coffre est en plus réservé à la copie qui l'a
  déverrouillé, et une modification n'est plus enregistrée par-dessus un coffre modifié
  ailleurs entre-temps : deux copies pouvaient annuler les changements l'une de
  l'autre, y compris un changement de mot de passe fait après la perte d'une clé de
  sécurité.
- Collage dans un terminal : les caractères de contrôle sont retirés du texte collé.
  Un texte piégé pouvait terminer le mode « collage encadré » avant l'heure et faire
  exécuter une commande sans validation.
- Windows : les téléchargements SFTP refusent les noms de fichiers que Windows
  interpréterait autrement (lecteur comme `C:nom`, flux `:`, noms de périphériques comme
  `CON` ou `NUL`, point ou espace final). Un serveur malveillant pouvait sinon écrire
  hors du dossier choisi.
- Activer le fournisseur d'identité avec 15 clés de sécurité enregistrées n'écrit plus
  un coffre impossible à rouvrir ; le coffre n'est jamais enregistré dans un état que
  l'application refuserait de relire.
- macOS : les mots de passe copiés sont exclus des gestionnaires d'historique du
  presse-papiers (Maccy, Raycast, Paste, Alfred…), comme sous Linux et Windows.
- Coffre verrouillé, l'application refuse la saisie dans les terminaux, les transferts
  et les opérations sur les fichiers locaux, même pour les sessions gardées ouvertes
  derrière l'écran de verrouillage.
- Export chiffré : fichier écrit de façon atomique et toujours lisible par son seul
  propriétaire, même quand il remplace un fichier existant.
- Durcissement face aux serveurs hostiles : taille limitée des réponses du fournisseur
  d'identité et de la recherche de mise à jour, profondeur limitée pour la suppression
  récursive, coût de déchiffrement limité pour une clé privée importée, lecture plus
  robuste du retour du navigateur après la connexion.

**Corrigé**
- Les fichiers aux noms très longs ne pouvaient pas être téléchargés (nom temporaire
  trop long).
- Longueur minimale du mot de passe comptée de la même façon dans la fenêtre et dans le
  coffre.
- Paquet Fedora : icône installée dans le dossier standard 256×256.

## [1.8.1] - 2026-10-11

### English

**Changed**
- Settings split into tabs: General, Security, Terminal, Data, About. The last tab
  opened is remembered.
- Host tiles: new “Compact” display (Settings → General), without the address (shown
  on hover) and with the badges under the name. “Standard” stays the default.
- Update notice: better contrast for the “View” button.
- Settings page centered in the window, with equal margins on both sides.

### Français

**Modifié**
- Réglages répartis en onglets : Général, Sécurité, Terminal, Données, À propos. Le
  dernier onglet ouvert est retenu.
- Tuiles des hôtes : nouvel affichage « Compact » (Réglages → Général), sans l'adresse
  (affichée au survol) et avec les badges sous le nom. « Standard » reste l'affichage
  par défaut.
- Encart de mise à jour : bouton « Voir » mieux contrasté.
- Page des réglages centrée dans la fenêtre, avec des marges égales des deux côtés.

## [1.8.0] - 2026-10-11

### English

**Added**
- Windows: security keys (FIDO2), through Windows' own window for the PIN and the
  touch. A key registered on Linux or macOS also opens the vault on Windows, and the
  other way round.

### Français

**Ajouté**
- Windows : clés de sécurité (FIDO2), par la fenêtre de Windows pour le code PIN et le
  toucher. Une clé enregistrée sous Linux ou macOS ouvre aussi le coffre sous Windows,
  et inversement.

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
