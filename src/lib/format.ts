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
 * Nom de fichier venu d'un serveur, rendu sans piège à l'affichage : les caractères
 * de contrôle et d'inversion bidirectionnelle (U+202E…) sont remplacés par « � »,
 * pour qu'un « facture\u202Efdp.exe » ne s'affiche pas « factureexe.pdf ».
 */
export function displayName(name: string): string {
  return name.replace(/[\u0000-\u001f\u007f-\u009f\u200e\u200f\u202a-\u202e\u2066-\u2069]/g, "\ufffd");
}
