// Chemins affichés par l'explorateur : style Unix (serveur, Linux, macOS) ou Windows
// (fichiers locaux sous Windows : lettres de lecteur, « \ », et une racine « Ce PC »
// représentée par le chemin vide, qui liste les lecteurs).

export type PathStyle = "unix" | "windows";

export interface Crumb {
  label: string;
  path: string;
}

const DRIVE = /^[A-Za-z]:$/;

export function parentOf(p: string, style: PathStyle): string {
  if (style === "unix") {
    if (p === "/" || p === "") return "/";
    const trimmed = p.replace(/\/+$/, "");
    const i = trimmed.lastIndexOf("/");
    return i <= 0 ? "/" : trimmed.slice(0, i);
  }
  const trimmed = p.replace(/\\+$/, "");
  if (!trimmed || DRIVE.test(trimmed)) return "";
  const i = trimmed.lastIndexOf("\\");
  if (i < 0) return "";
  const parent = trimmed.slice(0, i);
  if (DRIVE.test(parent)) return `${parent}\\`;
  // Partage réseau « \\serveur\partage » : au-dessus, la racine.
  return /^\\\\[^\\]*$/.test(parent) ? "" : parent;
}

/** Fil d'Ariane ; `root` : libellé de la racine Windows (« Ce PC »). */
export function crumbsOf(p: string, style: PathStyle, root: string): Crumb[] {
  if (style === "unix") {
    const parts = p.split("/").filter(Boolean);
    return [{ label: "/", path: "/" }, ...parts.map((part, i) => ({ label: part, path: "/" + parts.slice(0, i + 1).join("/") }))];
  }
  const unc = p.startsWith("\\\\");
  const parts = p.split("\\").filter(Boolean);
  return [
    { label: root, path: "" },
    ...parts.map((part, i) => {
      const joined = (unc ? "\\\\" : "") + parts.slice(0, i + 1).join("\\");
      return { label: part, path: i === 0 && !unc ? `${joined}\\` : joined };
    }),
  ];
}

/** Dernier élément d'un chemin (« / » et, pour Windows, « \ »). */
export function baseName(p: string, style: PathStyle): string {
  const sep = style === "windows" ? /[\\/]+/ : /\/+/;
  return p.split(sep).filter(Boolean).pop() ?? p;
}
