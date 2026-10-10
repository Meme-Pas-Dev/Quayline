import { readFileSync } from "node:fs";
import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Identité de l'application : source unique, identity.json.
const identity = JSON.parse(readFileSync(new URL("./identity.json", import.meta.url), "utf8"));

const identityHtml: Plugin = {
  name: "identity-html",
  transformIndexHtml: (html) => html.replaceAll("%APP_NAME%", identity.name),
};

export default defineConfig({
  plugins: [svelte(), identityHtml],
  define: {
    __APP_NAME__: JSON.stringify(identity.name),
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**", "**/crates/**"] },
  },
  build: {
    target: "safari16",
    sourcemap: false,
  },
});
