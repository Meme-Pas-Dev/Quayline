# Modèle de sécurité de Quayline

## Ce que Quayline protège

| Menace | Protection |
|---|---|
| Vol du fichier coffre (sauvegarde, disque volé) | Chiffrement XChaCha20-Poly1305 ; clé dérivée par Argon2id coûteux |
| Attaque par force brute du mot de passe | Argon2id 256 Mio / 3 passes / 4 voies ; mot de passe ≥ 12 caractères |
| Modification malveillante du coffre | En-tête et contenu authentifiés (AEAD + AAD) ; paramètres Argon2 bornés (1 Gio, 10 passes), un seul slot « mot de passe », 16 slots au plus |
| Lecture de la mémoire par un autre processus de l'utilisateur | `PR_SET_DUMPABLE=0` (ptrace et `/proc/<pid>/mem` interdits) |
| Fuite via core dump | `RLIMIT_CORE=0` |
| Poste laissé ouvert | Verrouillage automatique après inactivité, au verrouillage de la session KDE/GNOME et avant la mise en veille ; `Ctrl+Maj+L` |
| Mot de passe dérobé par un faux dialogue | Les questions d'un serveur (keyboard-interactive) sont affichées comme telles, sous le nom de l'hôte, nettoyées des caractères de contrôle et d'inversion de texte |
| Clé d'hôte changée acceptée par erreur | Remplacement exigeant une confirmation explicite, vérifiée par le backend |
| Nom de fichier trompeur (`facture\u202Efdp.exe`) | Caractères de contrôle et d'inversion bidirectionnelle remplacés à l'affichage |
| Code malveillant injecté dans l'interface | CSP stricte, aucune ressource distante, commandes allow-listées, aucun secret renvoyé à la WebView ; l'interface ne peut pas lire le presse-papiers (le collage dans le terminal passe par le backend) ; vérifications du mot de passe maître temporisées |
| Corruption lors d'une coupure | Écriture atomique (fichier temporaire, `fsync`, `rename`) |
| Serveur SFTP malveillant renvoyant `../../.bashrc` | Noms contenant `/`, `\`, `\0`, `.` ou `..` rejetés à la lecture et au téléchargement |
| Écriture à travers un lien symbolique local | Un fichier de destination qui est un lien n'est jamais écrasé ; liens jamais suivis |
| Fichier tronqué après une coupure | Téléchargement dans `.nom.part-…`, renommé seulement une fois complet et synchronisé |
| Suppression locale par erreur | Les fichiers locaux vont à la corbeille (norme freedesktop, visible dans Dolphin) |
| Mot de passe laissé dans le presse-papiers | Copié par le backend (jamais par l'interface), exclu des historiques du presse-papiers (KDE : `x-kde-passwordManagerHint` ; Windows : historique `Win+V` et synchronisation ; macOS : `org.nspasteboard.ConcealedType`), effacé après le délai réglé, au verrouillage et à la fermeture — seulement s'il est encore présent |
| Session oubliée ouverte | Par défaut, le verrouillage ferme les terminaux, les explorateurs SFTP et annule les transferts. Si l'utilisateur choisit de garder les sessions, elles restent masquées et inertes (ni clavier ni souris) derrière l'écran de verrouillage, et le backend refuse toute action tant que le coffre est verrouillé (saisie, collage, transferts, opérations sur les fichiers) ; le contenu affiché des terminaux reste alors en mémoire |

## Ce que Quayline ne protège pas

- Un poste déjà compromis avec les droits root, ou un enregistreur de frappe : il
  capturera le mot de passe maître à la saisie.
- Les copies de secrets laissées par la WebView ou le noyau lors de la saisie
  (tampons IPC) : elles sont hors de notre contrôle, mais ne quittent pas la RAM
  et disparaissent avec le processus.

## Limites connues

Points relevés lors de la revue de sécurité de la v1, acceptés en connaissance de cause :

- **Retrait d'un seul moyen de déverrouillage.** Changer le mot de passe maître
  renouvelle la clé de données et rechiffre tout le coffre : une ancienne copie, même
  ouverte avec un ancien moyen de déverrouillage, ne déchiffre plus le nouveau. Ce
  changement retire aussi le fournisseur d'identité et les clés FIDO2 (leur secret n'est
  pas disponible pour réemballer la nouvelle clé) : ils sont à réactiver. En revanche,
  retirer une seule clé FIDO2 ou le fournisseur d'identité ne renouvelle pas la clé de
  données. **Après la perte
  d'une clé FIDO2, changez donc le mot de passe maître**, puis réenregistrez les clés
  restantes.
- **Signature des paquets.** Le RPM et les empreintes (RPM et Flatpak) sont signés par
  une clé GPG propre au projet (clé publique : `packaging/signing-key.asc`) ; la
  vérification avant installation est décrite dans le README. L'application macOS est signée (Developer ID) et notarisée par
  Apple. La confiance repose sur ces clés, rangées dans des secrets GitHub : quelqu'un
  qui contrôlerait à la fois le dépôt et ses secrets pourrait signer. Les paquets
  publiés sont construits à partir des étiquettes de version, jamais d'une pull request.
- **Processus de la WebView.** Le durcissement (`PR_SET_DUMPABLE`) protège le processus
  principal, pas les processus de WebKitGTK où transitent les saisies (mot de passe
  maître, PIN). Un programme malveillant tournant sous le même compte peut les observer,
  comme il pourrait le faire avec un enregistreur de frappe.
- **Isolation du Flatpak.** Le paquet a besoin du dossier personnel (SFTP), du réseau,
  des périphériques (clés FIDO2) et de X11 (presse-papiers) : il ne faut pas le
  considérer comme un bac à sable.
- **Fichiers locaux.** L'explorateur SFTP peut lire et déplacer n'importe quel fichier de
  l'utilisateur ; c'est sa fonction. Un script injecté dans l'interface aurait le même
  pouvoir, d'où la CSP stricte et l'absence de tout contenu distant.
- **Serveur malveillant.** La négociation et l'authentification SSH disposent de
  45 secondes de temps serveur (le temps passé par l'utilisateur à répondre n'est pas
  compté) ; un téléchargement s'interrompt après 60 secondes sans données, au-delà du
  double de la taille annoncée (+ 64 Mio) ou de 64 niveaux de dossiers. Une session
  établie peut rester inactive aussi longtemps que le serveur le permet ; sa sortie est
  bornée par le terminal (historique de 10 000 lignes).
- **Temporisation des essais.** Le compteur d'échecs est remis à zéro au redémarrage ;
  chaque essai coûte de toute façon un calcul Argon2id (~1 s).

## Hiérarchie de clés

```
mot de passe maître ──Argon2id(sel 256 bits)──▶ KEK
                                                 │ XChaCha20-Poly1305
                                                 ▼
                         DEK aléatoire (256 bits) emballée dans un « slot »
                                                 │ XChaCha20-Poly1305 (AAD = en-tête)
                                                 ▼
                         contenu : hôtes, identités, clés d'hôte, réglages
```

La DEK n'est jamais dérivée du mot de passe : changer de mot de passe ne réécrit que le
slot. D'autres slots emballent la même DEK (fournisseur d'identité, clés FIDO2) sans
affaiblir le slot du mot de passe.

## Format du fichier

```
"QLVAULT\0" (8 o) | longueur de l'en-tête (u32 LE) | en-tête JSON | contenu chiffré
```

L'en-tête contient la version, les slots (paramètres KDF, sel, nonce, DEK emballée)
et le nonce du contenu, renouvelé à chaque enregistrement. Les octets
`magic | longueur | en-tête` servent de données associées au chiffrement du contenu.

**L'en-tête est authentifié, pas confidentiel** : il doit être lisible avant le
déverrouillage pour proposer les moyens disponibles. Un fichier de coffre dérobé révèle
donc, sans mot de passe, pour le fournisseur d'identité : l'adresse de l'instance, le
`client_id`, l'identifiant (`sub`) et le nom affiché du compte, le groupe exigé, le port
de retour et, s'il y en a un, le secret client ; pour chaque clé FIDO2 : le nom choisi,
le domaine (`rp_id`) et l'identifiant de la clé. Rien de cela ne permet d'ouvrir le
coffre ; la liste des serveurs, les identifiants et les clés restent chiffrés. Piste
envisagée : chiffrer le nom du compte et des clés avec la clé de données et n'afficher
qu'un libellé générique avant le déverrouillage.

## Export et import

Réglages → Données → Sauvegarde et transfert. Le fichier (`.qlexport`) est chiffré par une phrase
de passe propre à l'export (12 caractères minimum), indépendante du mot de passe maître :

```
"QLEXPORT" (8 o) | longueur de l'en-tête (u32 LE) | en-tête JSON | contenu chiffré
phrase de passe ──Argon2id (256 Mio, 3 passes)──▶ clé ──XChaCha20-Poly1305──▶ contenu
```

- L'en-tête ne contient que les paramètres Argon2, le sel et le nonce ; il sert de
  données associées. Rien d'autre n'est lisible, pas même le nombre d'hôtes.
- Exporter demande le mot de passe maître (avec la même temporisation qu'au
  déverrouillage) : le fichier contient les secrets du trousseau.
- Les fichiers sont choisis dans une boîte de dialogue ouverte par le backend :
  l'interface ne fournit jamais de chemin. Le fichier écrit est en `0600`.
- À l'import, le contenu déchiffré est revérifié avec les règles de saisie (un fichier
  peut avoir été fabriqué par un tiers), puis présenté en aperçu ; rien n'est modifié
  avant confirmation. Il reste en mémoire au plus le temps de l'import et est effacé au
  verrouillage.
- Une clé d'hôte différente de celle déjà approuvée n'est remplacée que sur demande
  explicite (« Remplacer ») : un serveur n'a qu'une clé approuvée.
- Le fournisseur d'identité et les clés FIDO2 ne sont jamais exportés : ils sont liés à l'installation.

## Fournisseur d'identité (OIDC)

Pocket ID, Authentik, Authelia, Keycloak, Microsoft Entra ID, Google, ou tout fournisseur
OpenID Connect conforme. Les modèles ne font que pré-remplir les réglages ; le flux et
les contrôles sont les mêmes pour tous.

OIDC prouve une identité mais ne fournit aucun secret capable de déchiffrer le coffre.
Le déverrouillage fonctionne donc ainsi :

1. **Activation** (coffre déverrouillé, mot de passe maître ressaisi) : authentification
   auprès du fournisseur, puis une clé aléatoire de 256 bits emballe la DEK dans un
   nouveau slot. Cette clé est rangée dans le trousseau système (Secret Service sous
   Linux, Trousseau sous macOS). Le compte autorisé et les réglages (modèle, instance,
   Client ID, `sub`, groupe exigé, port, scopes, donnée des groupes, secret client) sont
   inclus dans les données authentifiées du slot : les modifier dans le fichier rend le
   slot inutilisable.
2. **Déverrouillage** : Authorization Code + PKCE S256 avec `state` et `nonce`, retour sur
   `http://127.0.0.1:<port>/callback`. Le jeton d'identité est vérifié : signature
   asymétrique avec les clés publiques de l'instance (jamais `none` ni HMAC), `iss` (tel
   qu'annoncé par la découverte, « / » final compris), `aud`, expiration, `nonce`. Le
   `sub` doit être celui enregistré, et le groupe exigé doit figurer dans le jeton, sous
   la donnée configurée (`groups` par défaut, ou un chemin comme `realm_access.roles`).
   La clé est alors lue dans le trousseau et ouvre le slot.
3. **Depuis un téléphone** (Device Authorization Grant, RFC 8628), si l'instance annonce
   `device_authorization_endpoint` : l'écran de verrouillage affiche un QR code et un
   code court ; on valide sur le téléphone avec sa passkey, Quayline interroge l'instance
   jusqu'à la validation, sans port local. L'adresse du QR code doit appartenir à
   l'instance configurée (même origine), ou à un domaine connu du fournisseur choisi
   (microsoft.com et login.microsoft(online).com pour Entra) ; le code court, à comparer
   sur le téléphone, protège contre l'hameçonnage par code. Le jeton reçoit les mêmes contrôles, à ceci
   près que le `nonce` n'est vérifié que s'il est renvoyé : le jeton vient directement de
   l'instance, en échange du code de l'appareil, et ne transite pas par le navigateur.

Après une connexion dans le navigateur ou sur le téléphone, la fenêtre de Quayline est
ramenée au premier plan (au mieux : certains bureaux refusent le vol de focus et font
seulement clignoter l'application). Au retour dans le navigateur, l'onglet reste en
attente jusqu'à la décision finale (jeton, compte, groupe, ouverture du coffre) : il
affiche « Connexion réussie » seulement si le coffre est réellement déverrouillé, sinon
la raison du refus. Cette page n'a droit à aucune ressource externe (CSP
`default-src 'none'`).

Données demandées au fournisseur : le minimum, soit `openid` (identifiant du compte),
`profile` (un nom à afficher) et, si besoin, les groupes. L'adresse e-mail n'est ni
demandée par défaut ni affichée.

Contrôles complémentaires : HTTPS obligatoire (HTTP accepté uniquement en local, pour
les tests), l'issuer annoncé par la découverte doit être exactement celui configuré,
certificats vérifiés par le magasin du système (une autorité privée ajoutée à Fedora
est donc reconnue), client public de préférence. Le secret client, quand le fournisseur
l'exige (Google), est conservé dans le fichier du coffre en clair mais authentifié : pour
une application de bureau il n'est pas confidentiel (RFC 8252, §8.5), Google le dit
lui-même. Seul le `sub` enregistré peut déverrouiller ; chez Google, sans groupes, c'est
la seule restriction.

**Niveau de sécurité.** Ce mode équivaut à « session ouverte + compte chez le
fournisseur » : quelqu'un qui contrôle déjà votre session (et donc votre trousseau)
pourrait lire la clé sans passer par le fournisseur. Le mot de passe maître reste la
racine, et le moyen de secours. Pour un facteur matériel qui ne dépend pas du trousseau, voir ci-dessous.

**Authentification récente.** Hors délai de grâce, une session déjà ouverte chez le
fournisseur ne suffit pas : la connexion par navigateur demande une nouvelle
authentification (`prompt=login`, sauf Google qui ne l'accepte pas) datant de moins de
5 minutes (`max_age=300`), et Quayline vérifie l'heure `auth_time` du jeton (marge d'une
minute). **Délai de grâce** (réglage « Nouvelle connexion au fournisseur d'identité »,
15 minutes par défaut, 0 à 60) : dans ce délai après la dernière connexion par le
navigateur, la session du fournisseur suffit (un clic), avec `max_age` égal au délai.
Le délai est compté depuis la dernière connexion réussie, en mémoire seulement : après
un redémarrage de l'application, une nouvelle connexion est exigée. Compromis assumé :
pendant ce délai, une personne devant le poste (session du bureau ouverte, Quayline
verrouillé) peut déverrouiller en un clic ; « À chaque déverrouillage » le supprime.
Un fournisseur qui n'envoie pas `auth_time` est accepté, mais l'activation le signale :
la réauthentification n'est alors pas vérifiable. La connexion depuis un téléphone
demande de toute façon une validation explicite sur le téléphone.

## Clés de sécurité FIDO2

Une clé FIDO2 (YubiKey, Nitrokey, SoloKey…) peut ouvrir le coffre grâce à l'extension
CTAP2 `hmac-secret` : la clé calcule un HMAC à partir d'un secret qu'elle seule détient
(propre à l'identifiant créé pour Quayline) et d'un sel aléatoire de 256 bits stocké
dans le slot. Ce HMAC est la clé qui emballe la DEK.

- **Rien n'est stocké hors de la clé** : ni dans le trousseau système, ni dans le coffre.
- **Code PIN obligatoire** : le secret est demandé avec vérification de l'utilisateur.
  Les authentificateurs renvoient un secret différent avec et sans vérification ;
  le slot ne s'ouvre donc qu'avec le PIN, et la clé se bloque après plusieurs erreurs.
- **Toucher obligatoire** (présence de l'utilisateur) à chaque déverrouillage.
- L'identifiant de la clé, le sel et le nom donné font partie des données authentifiées
  du slot. Plusieurs clés peuvent être enregistrées (une clé de secours est conseillée) ;
  une même clé ne peut pas l'être deux fois.
- Ajouter une clé demande le mot de passe maître ; le retirer supprime simplement le slot.
- La clé est jointe directement en USB (`hidraw`), sans navigateur. Les passkeys
  stockées dans un téléphone ou un gestionnaire de mots de passe ne sont pas prises en
  charge.

Ce mode est plus fort que le fournisseur d'identité : il faut posséder la clé physique et connaître son
PIN, et une session déjà compromise ne suffit pas à lire le secret.

## Verrouillage avec le système

Sous Linux, le coffre se verrouille quand la session de bureau est verrouillée (propriété
`LockedHint` et signal `Lock` de logind, signal `ActiveChanged` de
`org.freedesktop.ScreenSaver`) et juste avant une mise en veille ou une hibernation
(`PrepareForSleep` de logind). Le réglage « Sessions au verrouillage » s'applique : les
connexions sont fermées par défaut. Sous macOS, il se verrouille au verrouillage de
l'écran et avant la mise en veille. Ce comportement peut être désactivé dans les
réglages.
