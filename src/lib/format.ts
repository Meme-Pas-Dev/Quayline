const UNITS = ["o", "Ko", "Mo", "Go", "To"];

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} o`;
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toLocaleString("fr-FR", { maximumFractionDigits: v < 10 ? 1 : 0 })} ${UNITS[i]}`;
}

const dateFmt = new Intl.DateTimeFormat("fr-FR", { dateStyle: "short", timeStyle: "short" });

export function formatDate(secs: number | null): string {
  return secs ? dateFmt.format(new Date(secs * 1000)) : "";
}

/**
 * Nom de fichier venu d'un serveur, rendu sans piège à l'affichage : les caractères
 * de contrôle et d'inversion bidirectionnelle (U+202E…) sont remplacés par « � »,
 * pour qu'un « facture\u202Efdp.exe » ne s'affiche pas « factureexe.pdf ».
 */
export function displayName(name: string): string {
  return name.replace(/[\u0000-\u001f\u007f-\u009f\u200e\u200f\u202a-\u202e\u2066-\u2069]/g, "\ufffd");
}
