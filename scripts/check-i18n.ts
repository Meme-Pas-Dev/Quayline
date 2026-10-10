// Contrôle des traductions (lancé par « npm run check ») : mêmes clés et mêmes
// paramètres dans chaque langue, pluriels complets, aucun texte vide ; règles de
// choix de la langue. Exécuté par Node avec --experimental-strip-types.

import { readFileSync } from "node:fs";
import { LANGUAGES, parsePreference, resolveLanguage, supportedLanguage } from "../src/lib/i18n/language.ts";

const dir = new URL("../src/lib/i18n/", import.meta.url);
const load = (lang: string): Record<string, string> => JSON.parse(readFileSync(new URL(`${lang}.json`, dir), "utf8"));
const params = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");

const errors: string[] = [];
const reference = load("en");
for (const lang of LANGUAGES) {
  const catalog = load(lang);
  for (const key of Object.keys(reference)) {
    if (!(key in catalog)) errors.push(`${lang} : clé absente « ${key} »`);
    else if (params(catalog[key]) !== params(reference[key]))
      errors.push(`${lang} : paramètres différents pour « ${key} » (${params(catalog[key]) || "aucun"} au lieu de ${params(reference[key]) || "aucun"})`);
  }
  for (const [key, text] of Object.entries(catalog)) {
    if (!(key in reference)) errors.push(`${lang} : clé inconnue « ${key} »`);
    if (typeof text !== "string" || !text.trim()) errors.push(`${lang} : texte vide pour « ${key} »`);
    if (key.endsWith(".one") && !(key.replace(/\.one$/, ".other") in catalog)) errors.push(`${lang} : « ${key} » sans « .other »`);
    if (key.endsWith(".other") && !(key.replace(/\.other$/, ".one") in catalog)) errors.push(`${lang} : « ${key} » sans « .one »`);
  }
}

const cases: [string, ReturnType<typeof supportedLanguage>][] = [
  ["fr", "fr"], ["fr-FR", "fr"], ["fr-CA", "fr"], ["fr_CH", "fr"], ["fr-BE", "fr"], ["FR-ca", "fr"],
  ["en", "en"], ["en-US", "en"], ["en-GB", "en"], ["en_AU", "en"],
  ["de-DE", "en"], ["es", "en"], ["", "en"], ["freaky", "en"],
];
for (const [tag, expected] of cases) {
  if (supportedLanguage(tag) !== expected) errors.push(`langue du système « ${tag} » : ${supportedLanguage(tag)} au lieu de ${expected}`);
}
if (parsePreference(null) !== "system") errors.push("préférence par défaut : « system » attendu");
if (parsePreference("de") !== "system") errors.push("préférence inconnue : « system » attendu");
if (resolveLanguage(parsePreference(null), "de-DE") !== "en") errors.push("premier lancement, système non pris en charge : anglais attendu");
if (resolveLanguage(parsePreference(null), "fr-CH") !== "fr") errors.push("premier lancement, système en français : français attendu");
if (resolveLanguage("fr", "en-US") !== "fr") errors.push("préférence « fr » ignorée");
if (resolveLanguage("system", "fr-CA") !== "fr") errors.push("préférence « system » ignorée");

if (errors.length) {
  console.error(errors.map((e) => `✗ ${e}`).join("\n"));
  process.exit(1);
}
console.log(`✓ Traductions : ${Object.keys(reference).length} textes, ${LANGUAGES.length} langues.`);
