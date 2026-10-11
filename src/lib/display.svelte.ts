// Affichage des tuiles d'hôtes (« standard » ou « compact »). Rangé dans le stockage
// local de la fenêtre, comme le thème : préférence d'affichage propre à l'ordinateur.

export type TileDensity = "standard" | "compact";

const KEY = "quayline.tiles";

function read(): TileDensity {
  try {
    return localStorage.getItem(KEY) === "compact" ? "compact" : "standard";
  } catch {
    return "standard";
  }
}

export const display = $state<{ tiles: TileDensity }>({ tiles: read() });

export function setTileDensity(value: TileDensity) {
  display.tiles = value;
  try {
    if (value === "standard") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, value);
  } catch {
    // Non mémorisé, mais appliqué pour cette session.
  }
}
