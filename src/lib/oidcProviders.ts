import type { OidcProviderId } from "./api";
import { t, type MessageKey } from "./i18n/index.svelte";

/**
 * Modèle de fournisseur d'identité : réglages pré-remplis et marche à suivre. Les
 * textes sont des clés de traduction (catalogues de src/lib/i18n).
 */
export interface OidcProvider {
  id: OidcProviderId;
  /** Nom de marque, non traduit ; absent : « Autre fournisseur OIDC ». */
  name?: string;
  /** Adresse de l'instance (issuer) : valeur par défaut, sinon exemple. */
  issuer?: string;
  issuerPlaceholder: MessageKey;
  issuerHint?: MessageKey;
  scopes: string;
  groupsClaim: string;
  /** Exemple de groupe exigé ; absent : pas de groupes chez ce fournisseur. */
  groupPlaceholder?: MessageKey;
  groupHint?: MessageKey;
  /** Secret client exigé (Google). */
  secretRequired?: boolean;
  /** Intitulé de l'étape de l'URL de rappel, si différent. */
  redirectLabel?: MessageKey;
  /** Étapes avant et après celle de l'URL de rappel. */
  before: MessageKey[];
  after: MessageKey[];
  /** Note sur la connexion par téléphone (QR code). */
  phone: MessageKey;
  /** Pas encore essayé en conditions réelles. */
  beta?: boolean;
}

export const OIDC_PROVIDERS: OidcProvider[] = [
  {
    id: "pocket-id",
    name: "Pocket ID",
    issuerPlaceholder: "oidc.pocket.issuer",
    scopes: "openid profile groups",
    groupsClaim: "groups",
    groupPlaceholder: "oidc.optional",
    before: ["oidc.pocket.before1"],
    after: ["oidc.pocket.after1"],
    phone: "oidc.pocket.phone",
  },
  {
    id: "authentik",
    beta: true,
    name: "Authentik",
    issuerPlaceholder: "oidc.authentik.issuer",
    issuerHint: "oidc.authentik.issuerHint",
    scopes: "openid profile",
    groupsClaim: "groups",
    groupPlaceholder: "oidc.optional",
    before: ["oidc.authentik.before1", "oidc.authentik.before2"],
    after: ["oidc.authentik.after1", "oidc.authentik.after2"],
    phone: "oidc.authentik.phone",
  },
  {
    id: "authelia",
    beta: true,
    name: "Authelia",
    issuerPlaceholder: "oidc.authelia.issuer",
    scopes: "openid profile groups",
    groupsClaim: "groups",
    groupPlaceholder: "oidc.optional",
    before: ["oidc.authelia.before1"],
    after: ["oidc.authelia.after1"],
    phone: "oidc.authelia.phone",
  },
  {
    id: "keycloak",
    beta: true,
    name: "Keycloak",
    issuerPlaceholder: "oidc.keycloak.issuer",
    issuerHint: "oidc.keycloak.issuerHint",
    scopes: "openid profile",
    groupsClaim: "groups",
    groupPlaceholder: "oidc.optional",
    groupHint: "oidc.keycloak.groupHint",
    before: ["oidc.keycloak.before1", "oidc.keycloak.before2"],
    after: ["oidc.keycloak.after1", "oidc.keycloak.after2"],
    phone: "oidc.keycloak.phone",
  },
  {
    id: "entra",
    beta: true,
    name: "Microsoft Entra ID",
    issuerPlaceholder: "oidc.entra.issuer",
    issuerHint: "oidc.entra.issuerHint",
    scopes: "openid profile",
    groupsClaim: "groups",
    groupPlaceholder: "oidc.entra.groupPlaceholder",
    groupHint: "oidc.entra.groupHint",
    before: ["oidc.entra.before1", "oidc.entra.before2"],
    after: ["oidc.entra.after1", "oidc.entra.after2", "oidc.entra.after3"],
    phone: "oidc.entra.phone",
  },
  {
    id: "google",
    beta: true,
    name: "Google",
    issuer: "https://accounts.google.com",
    issuerPlaceholder: "oidc.google.issuer",
    scopes: "openid profile",
    groupsClaim: "groups",
    secretRequired: true,
    before: ["oidc.google.before1"],
    redirectLabel: "oidc.google.redirect",
    after: ["oidc.google.after1", "oidc.google.after2"],
    phone: "oidc.google.phone",
  },
  {
    id: "generic",
    beta: true,
    issuerPlaceholder: "oidc.generic.issuer",
    issuerHint: "oidc.generic.issuerHint",
    scopes: "openid profile",
    groupsClaim: "groups",
    groupPlaceholder: "oidc.optional",
    groupHint: "oidc.generic.groupHint",
    before: ["oidc.generic.before1"],
    after: ["oidc.generic.after1"],
    phone: "oidc.generic.phone",
  },
];

export function oidcProvider(id: OidcProviderId | undefined): OidcProvider {
  return OIDC_PROVIDERS.find((p) => p.id === id) ?? OIDC_PROVIDERS[0];
}

/** Nom affiché d'un fournisseur (« Keycloak », « Autre fournisseur OIDC »…). */
export function providerName(id: OidcProviderId | undefined): string {
  return oidcProvider(id).name ?? t("oidc.otherProvider");
}
