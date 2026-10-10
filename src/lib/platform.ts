// Plateforme, détectée dans la WebView (WKWebView sur macOS) : synchrone, donc
// utilisable directement dans les gestionnaires de touches.
export const IS_MAC = /Macintosh|Mac OS X/.test(navigator.userAgent);

// Raccourcis de l'application : Cmd sur macOS, Ctrl+Maj ailleurs, pour que Ctrl+C,
// Ctrl+L, etc. restent transmis au serveur.
export const KEYS = IS_MAC
  ? { copy: "⌘C", paste: "⌘V", lock: "⌘⇧L", lockShort: "⌘⇧L" }
  : { copy: "Ctrl+Maj+C", paste: "Ctrl+Maj+V", lock: "Ctrl+Maj+L", lockShort: "Ctrl⇧L" };

export type Shortcut = "copy" | "paste" | "lock";

export function shortcut(e: KeyboardEvent): Shortcut | null {
  const key = e.key.toLowerCase();
  if (IS_MAC) {
    if (!e.metaKey || e.ctrlKey || e.altKey) return null;
    if (e.shiftKey) return key === "l" ? "lock" : null;
    return key === "c" ? "copy" : key === "v" ? "paste" : null;
  }
  if (!e.ctrlKey || !e.shiftKey) return null;
  return key === "c" ? "copy" : key === "v" ? "paste" : key === "l" ? "lock" : null;
}
