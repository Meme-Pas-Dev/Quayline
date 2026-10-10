// Mises à jour : recherche de la dernière release (backend), avis dans la barre
// latérale. Une version peut être ignorée : son avis ne revient plus, mais elle reste
// signalée dans Réglages → À propos.

import { api, errorMessage, type InstallProgress, type UpdateInfo } from "./api";

const SKIP_KEY = "quayline.update.skipped";
/** Recherche automatique au plus toutes les 12 heures (à chaque déverrouillage sinon). */
const AUTO_INTERVAL = 12 * 3600 * 1000;

function readSkipped(): string | null {
  try {
    return localStorage.getItem(SKIP_KEY);
  } catch {
    return null;
  }
}

export const update = $state<{
  available: UpdateInfo | null;
  checking: boolean;
  /** Dernière recherche réussie (ms), pour « À jour » dans À propos. */
  checkedAt: number | null;
  error: string | null;
  skipped: string | null;
  /** Installation en cours (macOS), puis « installed » en attendant le redémarrage. */
  install: InstallProgress | { step: "installed" } | null;
}>({ available: null, checking: false, checkedAt: null, error: null, skipped: readSkipped(), install: null });

let lastAuto = 0;

export async function checkForUpdate(): Promise<void> {
  if (update.checking || update.install) return;
  update.checking = true;
  update.error = null;
  try {
    update.available = await api.updateCheck();
    update.checkedAt = Date.now();
  } catch (e) {
    update.error = errorMessage(e);
  } finally {
    update.checking = false;
  }
}

/** Recherche silencieuse après le déverrouillage, si le réglage l'autorise. */
export function autoCheckForUpdate() {
  if (Date.now() - lastAuto < AUTO_INTERVAL) return;
  lastAuto = Date.now();
  void checkForUpdate();
}

export function skipUpdate() {
  if (!update.available) return;
  update.skipped = update.available.version;
  try {
    localStorage.setItem(SKIP_KEY, update.skipped);
  } catch {
    // Ignorée pour cette session seulement.
  }
}

export function openRelease() {
  api.updateOpen().catch((e) => (update.error = errorMessage(e)));
}

export async function installUpdate() {
  if (update.install) return;
  update.error = null;
  update.install = { step: "download", done: 0, total: 0 };
  try {
    await api.updateInstall((p) => (update.install = p));
    update.install = { step: "installed" };
  } catch (e) {
    update.install = null;
    update.error = errorMessage(e);
  }
}

export function restartAfterUpdate() {
  api.updateRestart().catch((e) => (update.error = errorMessage(e)));
}
