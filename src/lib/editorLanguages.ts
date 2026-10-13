// Coloration syntaxique de l'éditeur intégré, selon l'extension ou le nom du fichier.
// Chaque langage est chargé à la demande (aucun coût tant qu'il ne sert pas).
import type { Extension } from "@codemirror/state";

type Loader = () => Promise<Extension>;

const legacy = (load: () => Promise<Parameters<typeof import("@codemirror/language").StreamLanguage.define>[0]>): Loader => async () => {
  const [{ StreamLanguage }, mode] = await Promise.all([import("@codemirror/language"), load()]);
  return StreamLanguage.define(mode);
};

const LANGUAGES: Record<string, Loader> = {
  json: async () => (await import("@codemirror/lang-json")).json(),
  yaml: async () => (await import("@codemirror/lang-yaml")).yaml(),
  python: async () => (await import("@codemirror/lang-python")).python(),
  javascript: async () => (await import("@codemirror/lang-javascript")).javascript(),
  typescript: async () => (await import("@codemirror/lang-javascript")).javascript({ typescript: true }),
  jsx: async () => (await import("@codemirror/lang-javascript")).javascript({ jsx: true, typescript: true }),
  xml: async () => (await import("@codemirror/lang-xml")).xml(),
  html: async () => (await import("@codemirror/lang-html")).html(),
  css: async () => (await import("@codemirror/lang-css")).css(),
  sql: async () => (await import("@codemirror/lang-sql")).sql(),
  markdown: async () => (await import("@codemirror/lang-markdown")).markdown(),
  shell: legacy(async () => (await import("@codemirror/legacy-modes/mode/shell")).shell),
  properties: legacy(async () => (await import("@codemirror/legacy-modes/mode/properties")).properties),
  toml: legacy(async () => (await import("@codemirror/legacy-modes/mode/toml")).toml),
  nginx: legacy(async () => (await import("@codemirror/legacy-modes/mode/nginx")).nginx),
  dockerfile: legacy(async () => (await import("@codemirror/legacy-modes/mode/dockerfile")).dockerFile),
  powershell: legacy(async () => (await import("@codemirror/legacy-modes/mode/powershell")).powerShell),
  diff: legacy(async () => (await import("@codemirror/legacy-modes/mode/diff")).diff),
  lua: legacy(async () => (await import("@codemirror/legacy-modes/mode/lua")).lua),
  perl: legacy(async () => (await import("@codemirror/legacy-modes/mode/perl")).perl),
  ruby: legacy(async () => (await import("@codemirror/legacy-modes/mode/ruby")).ruby),
  go: legacy(async () => (await import("@codemirror/legacy-modes/mode/go")).go),
  rust: legacy(async () => (await import("@codemirror/legacy-modes/mode/rust")).rust),
  c: legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).c),
  cpp: legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).cpp),
  java: legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).java),
  csharp: legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).csharp),
  kotlin: legacy(async () => (await import("@codemirror/legacy-modes/mode/clike")).kotlin),
  cmake: legacy(async () => (await import("@codemirror/legacy-modes/mode/cmake")).cmake),
};

const BY_EXTENSION: Record<string, string> = {
  json: "json", jsonc: "json", json5: "json",
  yaml: "yaml", yml: "yaml",
  py: "python", pyw: "python",
  js: "javascript", mjs: "javascript", cjs: "javascript",
  ts: "typescript", mts: "typescript", cts: "typescript", jsx: "jsx", tsx: "jsx",
  xml: "xml", svg: "xml", plist: "xml", xsd: "xml", xsl: "xml", csproj: "xml",
  html: "html", htm: "html", xhtml: "html",
  css: "css",
  sql: "sql",
  md: "markdown", markdown: "markdown",
  sh: "shell", bash: "shell", zsh: "shell", ksh: "shell", profile: "shell", bashrc: "shell", zshrc: "shell",
  ini: "properties", cfg: "properties", conf: "properties", properties: "properties", env: "properties",
  service: "properties", timer: "properties", socket: "properties", mount: "properties", desktop: "properties",
  toml: "toml",
  dockerfile: "dockerfile", containerfile: "dockerfile",
  ps1: "powershell", psm1: "powershell", psd1: "powershell",
  diff: "diff", patch: "diff",
  lua: "lua",
  pl: "perl", pm: "perl",
  rb: "ruby",
  go: "go",
  rs: "rust",
  c: "c", h: "c",
  cpp: "cpp", cc: "cpp", cxx: "cpp", hpp: "cpp", hh: "cpp",
  java: "java",
  cs: "csharp",
  kt: "kotlin", kts: "kotlin",
  cmake: "cmake",
};

const BY_NAME: Record<string, string> = {
  dockerfile: "dockerfile",
  containerfile: "dockerfile",
  "cmakelists.txt": "cmake",
  ".bashrc": "shell",
  ".bash_profile": "shell",
  ".profile": "shell",
  ".zshrc": "shell",
  ".env": "properties",
  "nginx.conf": "nginx",
  makefile: "shell",
};

/** Langage reconnu pour ce fichier, ou null (texte brut). */
export function languageOf(path: string): string | null {
  const name = path.split("/").pop()!.toLowerCase();
  if (BY_NAME[name]) return BY_NAME[name];
  // Configuration nginx : dossier /nginx/ (sites-available, conf.d…).
  if (name.endsWith(".conf") && path.toLowerCase().includes("/nginx/")) return "nginx";
  const dot = name.lastIndexOf(".");
  return dot > 0 || (dot === 0 && name.length > 1) ? BY_EXTENSION[name.slice(dot + 1)] ?? null : null;
}

export async function loadLanguage(language: string | null): Promise<Extension | null> {
  const load = language ? LANGUAGES[language] : undefined;
  if (!load) return null;
  try {
    return await load();
  } catch {
    return null; // Coloration absente : le texte reste modifiable.
  }
}
