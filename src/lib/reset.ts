// Après une réinitialisation (voir reset.rs) : préférences de l'interface effacées et
// remises à leur valeur par défaut, comme après une installation neuve.
import { setThemePreference } from "./appearance";
import { setTileDensity } from "./display.svelte";
import { setLanguagePreference } from "./i18n/index.svelte";
import { update } from "./updates.svelte";

export function resetLocalPreferences() {
  setLanguagePreference("system");
  setThemePreference("system");
  setTileDensity("standard");
  update.skipped = null;
  try {
    const keys = Array.from({ length: localStorage.length }, (_, i) => localStorage.key(i)).filter(
      (k): k is string => !!k && k.startsWith("quayline."),
    );
    for (const k of keys) localStorage.removeItem(k);
  } catch {
    // Stockage indisponible : rien n'y était rangé.
  }
}
