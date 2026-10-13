import { formatNumber, intlLocale, t } from "./i18n/index.svelte";
const UNITS = ["units.bytes", "units.kb", "units.mb", "units.gb", "units.tb"] as const;

/** Taille lisible, dans la langue de l'interface (« 1,5 Mo », « 1.5 MB »). */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${formatNumber(bytes)} ${t(UNITS[0])}`;
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${formatNumber(v, { maximumFractionDigits: v < 10 ? 1 : 0 })} ${t(UNITS[i])}`;
}

export function formatDate(secs: number | null): string {
  if (!secs) return "";
  return new Intl.DateTimeFormat(intlLocale(), { dateStyle: "short", timeStyle: "short" }).format(new Date(secs * 1000));
}

/**
 * Nom de fichier ou chemin venu d'un serveur, rendu sans piège à l'affichage : les
 * caractères de contrôle, d'inversion bidirectionnelle (U+202E…), de largeur nulle
 * (U+200B…) et les séparateurs de ligne sont remplacés par « � », pour qu'un
 * « facture\u202Efdp.exe » ne s'affiche pas « factureexe.pdf ». Même liste que
 * `text.rs` côté backend.
 */
export function displayName(name: string): string {
  return name.replace(/[\u0000-\u001f\u007f-\u009f\u061c\u200b-\u200f\u2028-\u202e\u2066-\u2069\ufeff]/g, "\ufffd");
}
