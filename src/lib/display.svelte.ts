// Affichage des tuiles d'hôtes (« standard » ou « compact ») et terminal local activé.
// Rangé dans le stockage local de la fenêtre, comme le thème : préférence propre à
// l'ordinateur.

import { IS_WINDOWS } from "./platform";

export type TileDensity = "standard" | "compact";

const KEY = "quayline.tiles";

function read(): TileDensity {
  try {
    return localStorage.getItem(KEY) === "compact" ? "compact" : "standard";
  } catch {
    return "standard";
  }
}

const LOCAL_TERM_KEY = "quayline.localTerm";

// Terminal local désactivé par défaut sous Windows : tant que l'exécutable n'est pas
// signé, des EDR (SentinelOne) bloquent l'application quand elle crée une console.
const LOCAL_TERM_DEFAULT = !IS_WINDOWS;

function readLocalTerm(): boolean {
  try {
    const v = localStorage.getItem(LOCAL_TERM_KEY);
    return v === null ? LOCAL_TERM_DEFAULT : v === "on";
  } catch {
    return LOCAL_TERM_DEFAULT;
  }
}

const OSC52_KEY = "quayline.osc52";

/** Copie demandée par le serveur (OSC 52 : tmux, vim…) : activée par défaut. */
function readOsc52(): boolean {
  try {
    return localStorage.getItem(OSC52_KEY) !== "off";
  } catch {
    return true;
  }
}

export const display = $state<{ tiles: TileDensity; localTerm: boolean; osc52: boolean }>({
  tiles: read(),
  localTerm: readLocalTerm(),
  osc52: readOsc52(),
});

export function setTileDensity(value: TileDensity) {
  display.tiles = value;
  try {
    if (value === "standard") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, value);
  } catch {
    // Non mémorisé, mais appliqué pour cette session.
  }
}

export function setLocalTermEnabled(value: boolean) {
  display.localTerm = value;
  try {
    if (value === LOCAL_TERM_DEFAULT) localStorage.removeItem(LOCAL_TERM_KEY);
    else localStorage.setItem(LOCAL_TERM_KEY, value ? "on" : "off");
  } catch {
    // Non mémorisé, mais appliqué pour cette session.
  }
}

export function setOsc52Enabled(value: boolean) {
  display.osc52 = value;
  try {
    if (value) localStorage.removeItem(OSC52_KEY);
    else localStorage.setItem(OSC52_KEY, "off");
  } catch {
    // Non mémorisé, mais appliqué pour cette session.
  }
}
