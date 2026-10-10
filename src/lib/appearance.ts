import { getCurrentWindow } from "@tauri-apps/api/window";

// Thème de l'interface. Rangé dans le stockage local de la fenêtre et non dans le
// coffre : il doit s'appliquer dès l'écran de verrouillage, et il n'a rien de secret.

export type ThemePreference = "system" | "light" | "dark";

const KEY = "quayline.theme";
const media = window.matchMedia("(prefers-color-scheme: light)");
// Barre de titre native : seul macOS la dessine lui-même. Sous Linux, elle vient du
// gestionnaire de fenêtres (KDE) et forcer le thème GTK fausserait la détection du
// thème système par le moteur web.
const NATIVE_TITLE_BAR = navigator.userAgent.includes("Mac");

let preference: ThemePreference = "system";

function read(): ThemePreference {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark") return v;
  } catch {
    // Stockage indisponible : on suit le système.
  }
  return "system";
}

function apply() {
  const light = preference === "system" ? media.matches : preference === "light";
  document.documentElement.dataset.theme = light ? "light" : "dark";
}

export function themePreference(): ThemePreference {
  return preference;
}

export function setThemePreference(value: ThemePreference) {
  preference = value;
  try {
    if (value === "system") localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, value);
  } catch {
    // Non mémorisé, mais appliqué pour cette session.
  }
  apply();
  if (NATIVE_TITLE_BAR) {
    void getCurrentWindow()
      .setTheme(value === "system" ? null : value)
      .catch(() => {});
  }
}

/** À appeler avant d'afficher l'interface. */
export function initAppearance() {
  media.addEventListener("change", () => {
    if (preference === "system") apply();
  });
  setThemePreference(read());
}
