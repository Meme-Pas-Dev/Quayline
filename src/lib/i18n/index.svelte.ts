// Traduction de l'interface : catalogues en.json (référence) et fr.json, clés plates,
// paramètres « {nom} ». La préférence est rangée dans le stockage local de la fenêtre,
// comme le thème : elle s'applique dès l'écran de verrouillage.

import { api } from "../api";
import en from "./en.json";
import fr from "./fr.json";
import { parsePreference, resolveLanguage, type Language, type LanguagePreference } from "./language";

export type { Language, LanguagePreference } from "./language";

export type MessageKey = keyof typeof en;
/** Clés au pluriel : « base.one » et « base.other » (règles de Intl.PluralRules). */
type PluralBase = MessageKey extends infer K ? (K extends `${infer B}.one` ? B : never) : never;
type Params = Record<string, string | number>;

// Une clé absente d'une langue est une erreur de compilation ; une clé en trop ou des
// paramètres différents sont détectés par scripts/check-i18n.ts (npm run check).
const catalogs: Record<Language, Record<MessageKey, string>> = { en, fr };

const KEY = "quayline.language";

const state = $state<{ preference: LanguagePreference; language: Language }>({
  preference: "system",
  language: "en",
});

function systemTag(): string {
  return navigator.languages?.[0] ?? navigator.language ?? "";
}

function fill(text: string, params?: Params): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

/** Texte traduit (réactif : se met à jour au changement de langue). */
export function t(key: MessageKey, params?: Params): string {
  return fill(catalogs[state.language][key] ?? catalogs.en[key] ?? key, params);
}

/** Texte au pluriel selon `count`, disponible aussi comme paramètre « {count} ». */
export function tn(base: PluralBase, count: number, params?: Params): string {
  const rule = new Intl.PluralRules(state.language).select(count) === "one" ? "one" : "other";
  return t(`${base}.${rule}` as MessageKey, { count: formatNumber(count), ...params });
}

export function language(): Language {
  return state.language;
}

/**
 * Locale des dates et nombres : celle du système si elle est de la même langue
 * (« en-GB », « fr-CA »), sinon la langue seule.
 */
export function intlLocale(): string {
  const tag = systemTag();
  return tag.toLowerCase().startsWith(state.language) ? tag : state.language;
}

export function formatNumber(value: number, options?: Intl.NumberFormatOptions): string {
  return value.toLocaleString(intlLocale(), options);
}

/** Énumération dans la langue de l'interface (« A, B et C », « A, B, and C »). */
export function formatList(items: string[]): string {
  return new Intl.ListFormat(intlLocale(), { type: "conjunction" }).format(items);
}

export function languagePreference(): LanguagePreference {
  return state.preference;
}

function apply() {
  state.language = resolveLanguage(state.preference, systemTag());
  document.documentElement.lang = state.language;
  // Messages du backend (erreurs, page du navigateur, menu macOS) dans la même langue.
  api.setLanguage(state.language).catch(() => {});
}

export function setLanguagePreference(value: LanguagePreference) {
  state.preference = value;
  try {
    localStorage.setItem(KEY, value);
  } catch {
    // Non mémorisée, mais appliquée pour cette session.
  }
  apply();
}

/** À appeler avant d'afficher l'interface. */
export function initLanguage() {
  let saved: string | null = null;
  try {
    saved = localStorage.getItem(KEY);
  } catch {
    // Stockage indisponible : préférence par défaut.
  }
  state.preference = parsePreference(saved);
  apply();
  window.addEventListener("languagechange", () => {
    if (state.preference === "system") apply();
  });
}
