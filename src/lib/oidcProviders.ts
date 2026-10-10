import type { OidcProviderId } from "./api";

/** Modèle de fournisseur d'identité : réglages pré-remplis et marche à suivre. */
export interface OidcProvider {
  id: OidcProviderId;
  name: string;
  /** Adresse de l'instance (issuer) : valeur par défaut, sinon exemple. */
  issuer?: string;
  issuerPlaceholder: string;
  issuerHint?: string;
  scopes: string;
  groupsClaim: string;
  /** Exemple de groupe exigé ; absent : pas de groupes chez ce fournisseur. */
  groupPlaceholder?: string;
  groupHint?: string;
  /** Secret client exigé (Google). */
  secretRequired?: boolean;
  /** Intitulé de l'étape de l'URL de rappel, si différent. */
  redirectLabel?: string;
  /** Étapes avant et après celle de l'URL de rappel. */
  before: string[];
  after: string[];
  /** Note sur la connexion par téléphone (QR code). */
  phone: string;
  /** Pas encore essayé en conditions réelles. */
  beta?: boolean;
}

export const OIDC_PROVIDERS: OidcProvider[] = [
  {
    id: "pocket-id",
    name: "Pocket ID",
    issuerPlaceholder: "https://id.mondomaine.fr",
    scopes: "openid profile groups",
    groupsClaim: "groups",
    groupPlaceholder: "Facultatif",
    before: ["Dans Pocket ID : Administration → Clients OIDC → Ajouter, nom « Quayline »."],
    after: ["Cochez « Client public » et « PKCE », enregistrez, puis copiez le Client ID ci-dessous."],
    phone: "Proposée automatiquement avec Pocket ID 0.50 ou plus récent.",
  },
  {
    id: "authentik",
    beta: true,
    name: "Authentik",
    issuerPlaceholder: "https://authentik.mondomaine.fr/application/o/quayline/",
    issuerHint: "L'adresse de l'émetteur (« OpenID Configuration Issuer ») affichée par le fournisseur.",
    scopes: "openid profile",
    groupsClaim: "groups",
    groupPlaceholder: "Facultatif",
    before: [
      "Dans Authentik : Applications → Applications → « Créer avec l'assistant », fournisseur « OAuth2/OpenID ».",
      "Type de client : « Public ».",
    ],
    after: [
      "Copiez le Client ID, et l'adresse « OpenID Configuration Issuer » du fournisseur (elle se termine par le « slug » de l'application).",
      "Les groupes sont transmis avec le scope « profile ».",
    ],
    phone: "À activer dans Système → Marques → flux « Device code ».",
  },
  {
    id: "authelia",
    beta: true,
    name: "Authelia",
    issuerPlaceholder: "https://auth.mondomaine.fr",
    scopes: "openid profile groups",
    groupsClaim: "groups",
    groupPlaceholder: "Facultatif",
    before: [
      "Dans la configuration d'Authelia (identity_providers → oidc → clients), ajoutez un client : client_id au choix, public: true, require_pkce: true, pkce_challenge_method: S256.",
    ],
    after: ["scopes : openid, profile, email, groups. Redémarrez Authelia, puis saisissez le client_id ci-dessous."],
    phone: "Proposée automatiquement si votre version d'Authelia la prend en charge.",
  },
  {
    id: "keycloak",
    beta: true,
    name: "Keycloak",
    issuerPlaceholder: "https://keycloak.mondomaine.fr/realms/mon-realm",
    issuerHint: "Adresse du realm : https://<serveur>/realms/<realm>.",
    scopes: "openid profile",
    groupsClaim: "groups",
    groupPlaceholder: "Facultatif",
    groupHint: "Nécessite le mapper « Group Membership » (étape 4).",
    before: [
      "Dans la console Keycloak, choisissez le realm, puis Clients → Create client (OpenID Connect), Client ID au choix.",
      "« Client authentication » désactivé (client public), « Standard flow » coché ; cochez aussi « OAuth 2.0 Device Authorization Grant » pour le QR code.",
    ],
    after: [
      "Groupes : Client scopes → <client>-dedicated → Configure a new mapper → Group Membership, « Token Claim Name » : groups, « Add to ID token » coché.",
      "Dans Advanced → « Proof Key for Code Exchange Code Challenge Method » : S256.",
    ],
    phone: "Proposée si « OAuth 2.0 Device Authorization Grant » est coché sur le client.",
  },
  {
    id: "entra",
    beta: true,
    name: "Microsoft Entra ID",
    issuerPlaceholder: "https://login.microsoftonline.com/<ID du locataire>/v2.0",
    issuerHint: "Remplacez <ID du locataire> par l'ID d'annuaire (GUID) affiché sur la page de l'application.",
    scopes: "openid profile",
    groupsClaim: "groups",
    groupPlaceholder: "ID d'objet du groupe (GUID)",
    groupHint: "Entra transmet les groupes par leur identifiant, pas par leur nom.",
    before: [
      "Portail Entra : Applications → Inscriptions d'applications → Nouvelle inscription, « Comptes dans cet annuaire organisationnel uniquement ».",
      "Plateforme « Applications mobiles et de bureau ».",
    ],
    after: [
      "Authentification : « Autoriser les flux clients publics » sur Oui (nécessaire au QR code).",
      "Pour exiger un groupe : Configuration des jetons → Ajouter une revendication de groupes.",
      "Copiez l'ID d'application (client) ci-dessous, et l'ID d'annuaire (locataire) dans l'adresse.",
    ],
    phone: "Proposée ; peut être bloquée par une stratégie d'accès conditionnel.",
  },
  {
    id: "google",
    beta: true,
    name: "Google",
    issuer: "https://accounts.google.com",
    issuerPlaceholder: "https://accounts.google.com",
    scopes: "openid profile",
    groupsClaim: "groups",
    secretRequired: true,
    before: [
      "Google Cloud Console : API et services → Identifiants → Créer des identifiants → ID client OAuth, type « Application de bureau ».",
    ],
    redirectLabel: "Rien à déclarer pour l'URL de rappel, Google accepte d'office les adresses locales. Quayline utilisera :",
    after: [
      "Copiez l'ID client et le code secret du client : Google l'exige, mais il n'est pas confidentiel pour une application de bureau.",
      "Pas de groupes chez Google : seul le compte utilisé à l'activation pourra déverrouiller.",
    ],
    phone: "Non disponible : Google la réserve à un autre type de client.",
  },
  {
    id: "generic",
    beta: true,
    name: "Autre fournisseur OIDC",
    issuerPlaceholder: "https://sso.mondomaine.fr",
    issuerHint: "L'adresse dont « /.well-known/openid-configuration » donne la configuration.",
    scopes: "openid profile",
    groupsClaim: "groups",
    groupPlaceholder: "Facultatif",
    groupHint: "Ajoutez aux scopes celui qui transmet les groupes, si besoin.",
    before: ["Créez un client OpenID Connect public, avec PKCE (S256)."],
    after: ["Copiez son Client ID ci-dessous."],
    phone: "Proposée automatiquement si le fournisseur l'annonce.",
  },
];

export function oidcProvider(id: OidcProviderId | undefined): OidcProvider {
  return OIDC_PROVIDERS.find((p) => p.id === id) ?? OIDC_PROVIDERS[0];
}
