import { mount } from "svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App.svelte";
import { initAppearance } from "./lib/appearance";
import { initLanguage } from "./lib/i18n/index.svelte";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
import "@xterm/xterm/css/xterm.css";
import "./app.css";

// Protection contre la pollution de prototype. Tauri sait le faire avant tout
// chargement (freezePrototype), mais xterm.js affecte `toString` sur un objet
// lors de l'évaluation de son module, ce qui échoue sur un prototype gelé.
// Les imports ci-dessus étant évalués avant ce code, on gèle ici.
Object.freeze(Object.prototype);

// Pas de menu contextuel du navigateur (« Inspecter », « Recharger »…).
document.addEventListener("contextmenu", (e) => {
  const target = e.target as HTMLElement | null;
  if (!target?.closest("input, textarea")) e.preventDefault();
});

initAppearance();
initLanguage();

const app = mount(App, { target: document.getElementById("app")! });

// La fenêtre est créée invisible (tauri.conf.json) : on l'affiche une fois
// l'interface en place, sans passer par un fond blanc. (Pas de
// requestAnimationFrame : il n'est pas appelé tant que la fenêtre est cachée.)
void getCurrentWindow().show();

export default app;
