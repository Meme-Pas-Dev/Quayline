// Choix de la langue de l'interface. Module sans dépendance : il est aussi vérifié
// par scripts/check-i18n.ts.

export const LANGUAGES = ["en", "fr"] as const;
export type Language = (typeof LANGUAGES)[number];
export type LanguagePreference = "system" | Language;

/** Au premier lancement : la langue du système (anglais si elle n'est pas prise en charge). */
export const DEFAULT_PREFERENCE: LanguagePreference = "system";

/**
 * Langue de l'interface pour une langue du système (« fr-CA », « en_GB »…) : seule la
 * langue de base compte, toutes ses variantes régionales donnent la même interface.
 * Une langue non prise en charge donne l'anglais.
 */
export function supportedLanguage(tag: string | null | undefined): Language {
  const base = (tag ?? "").trim().toLowerCase().split(/[-_.@]/)[0];
  return (LANGUAGES as readonly string[]).includes(base) ? (base as Language) : "en";
}

export function resolveLanguage(preference: LanguagePreference, systemTag: string | null | undefined): Language {
  return preference === "system" ? supportedLanguage(systemTag) : preference;
}

/** Préférence enregistrée, ou celle par défaut si elle est absente ou inconnue. */
export function parsePreference(value: string | null | undefined): LanguagePreference {
  return value === "system" || (LANGUAGES as readonly string[]).includes(value ?? "")
    ? (value as LanguagePreference)
    : DEFAULT_PREFERENCE;
}
