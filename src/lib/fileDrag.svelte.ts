// Glisser-déposer entre les deux panneaux de l'explorateur, géré à la souris (événements
// « pointer ») plutôt qu'avec le glisser-déposer HTML : sous Windows, WebView2 bloque
// ce dernier dès que le dépôt de fichiers du bureau (Tauri) est actif.

export const fileDrag = $state<{
  /** Panneau d'origine, ou rien hors glisser. */
  side: "local" | "remote" | null;
  paths: string[];
  x: number;
  y: number;
}>({ side: null, paths: [], x: 0, y: 0 });

/** Distance (px) à parcourir avant que l'appui devienne un glisser. */
export const DRAG_THRESHOLD = 5;
