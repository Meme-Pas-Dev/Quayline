<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "../i18n/index.svelte";
  import { api, errorCode, errorMessage, type RemoteVersion, type TextEncoding } from "../api";
  import { languageOf, loadLanguage } from "../editorLanguages";
  import { displayName } from "../format";
  import { IS_MAC } from "../platform";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Modal from "./Modal.svelte";
  import { toast } from "./toast.svelte";
  import type { EditorView } from "@codemirror/view";

  let { sftpId, path, name, onclose }: { sftpId: string; path: string; name: string; onclose: () => void } = $props();

  let host: HTMLDivElement;
  let view: EditorView | null = null;
  let ready = $state(false);
  let version: RemoteVersion | null = null;
  let encoding = $state<TextEncoding | null>(null);
  // L'éditeur est recréé pour chaque fichier : le chemin de départ suffit.
  const language = untrack(() => languageOf(path));
  let saved = "";
  let dirty = $state(false);
  let loading = $state(true);
  let saving = $state(false);
  let error = $state("");
  let ask = $state<"conflict" | "discard" | null>(null);

  const width = Math.min(1100, window.innerWidth - 64);

  onMount(() => {
    let disposed = false;
    (async () => {
      try {
        // Chargé à la demande : l'éditeur n'alourdit pas le démarrage.
        const [{ basicSetup }, { EditorView: View, keymap }, { EditorState }, { HighlightStyle, syntaxHighlighting }, { tags }, lang, text] =
          await Promise.all([
            import("codemirror"),
            import("@codemirror/view"),
            import("@codemirror/state"),
            import("@codemirror/language"),
            import("@lezer/highlight"),
            loadLanguage(language),
            api.sftpReadText(sftpId, path),
          ]);
        if (disposed) return;
        version = text.version;
        encoding = text.encoding;
        // Couleurs des thèmes clair et sombre (variables --syn-* de app.css).
        const highlight = HighlightStyle.define([
          { tag: [tags.keyword, tags.operatorKeyword, tags.controlKeyword, tags.modifier], color: "var(--syn-keyword)" },
          { tag: [tags.string, tags.special(tags.string), tags.regexp], color: "var(--syn-string)" },
          { tag: [tags.number, tags.bool, tags.null, tags.atom], color: "var(--syn-number)" },
          { tag: [tags.comment, tags.lineComment, tags.blockComment], color: "var(--syn-comment)", fontStyle: "italic" },
          { tag: [tags.variableName, tags.propertyName, tags.definition(tags.variableName)], color: "var(--syn-name)" },
          { tag: [tags.typeName, tags.className, tags.namespace], color: "var(--syn-type)" },
          { tag: [tags.tagName, tags.heading], color: "var(--syn-tag)", fontWeight: "600" },
          { tag: [tags.meta, tags.processingInstruction, tags.attributeName, tags.labelName], color: "var(--syn-meta)" },
          { tag: tags.link, textDecoration: "underline" },
          { tag: tags.emphasis, fontStyle: "italic" },
          { tag: tags.strong, fontWeight: "700" },
          { tag: [tags.inserted], color: "var(--syn-string)" },
          { tag: [tags.deleted], color: "var(--syn-tag)" },
        ]);
        // Fins de ligne Windows conservées à l'enregistrement (sliceDoc les reprend).
        const crlf = text.content.includes("\r\n");
        saved = text.content;
        view = new View({
          parent: host,
          state: EditorState.create({
            doc: text.content,
            extensions: [
              basicSetup,
              // Si la CSP porte un nonce (style en ligne ajouté à la page), CodeMirror
              // doit le reprendre pour que ses styles ne soient pas bloqués.
              ...(styleNonce() ? [View.cspNonce.of(styleNonce()!)] : []),
              syntaxHighlighting(highlight),
              ...(lang ? [lang] : []),
              EditorState.lineSeparator.of(crlf ? "\r\n" : "\n"),
              keymap.of([{ key: "Mod-s", preventDefault: true, run: () => (save(false), true) }]),
              View.updateListener.of((u) => {
                if (u.docChanged) dirty = u.state.sliceDoc() !== saved;
              }),
              View.theme({
                "&": { height: "100%", fontSize: "13px", backgroundColor: "var(--surface)", color: "var(--text)" },
                ".cm-scroller": { fontFamily: "var(--font-mono)" },
                ".cm-gutters": { backgroundColor: "var(--surface-2)", color: "var(--text-faint)", border: "none" },
                ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--accent-soft)" },
                "&.cm-focused .cm-cursor": { borderLeftColor: "var(--text)" },
                ".cm-selectionBackground, &.cm-focused .cm-selectionBackground": { backgroundColor: "var(--accent-soft) !important" },
                ".cm-panels": { backgroundColor: "var(--surface-2)", color: "var(--text)" },
              }),
            ],
          }),
        });
        view.focus();
        ready = true;
      } catch (e) {
        error = errorMessage(e);
      } finally {
        loading = false;
      }
    })();
    return () => {
      disposed = true;
      view?.destroy();
    };
  });

  async function save(force: boolean) {
    if (!view || saving) return;
    saving = true;
    ask = null;
    const content = view.state.sliceDoc();
    try {
      version = await api.sftpWriteText(sftpId, path, content, encoding!, force ? null : version);
      saved = content;
      dirty = false;
      toast(t("editor.saved", { name: displayName(name) }), "success");
    } catch (e) {
      if (errorCode(e) === "conflict") ask = "conflict";
      else toast(errorMessage(e), "error", 6000);
    } finally {
      saving = false;
    }
  }

  function styleNonce(): string | null {
    return document.querySelector<HTMLStyleElement>("style[nonce]")?.nonce || null;
  }

  function encodingLabel(e: TextEncoding) {
    const names = { "utf-8": "UTF-8", "utf-16le": "UTF-16 LE", "utf-16be": "UTF-16 BE", "windows-1252": "Windows-1252" } as const;
    return names[e.charset] + (e.bom ? " (BOM)" : "");
  }

  function requestClose() {
    if (dirty) ask = "discard";
    else onclose();
  }
</script>

<Modal title={t("editor.title", { name: displayName(name) })} onclose={requestClose} {width}>
  <p class="where mono faint">{displayName(path)}</p>
  <div class="editor" bind:this={host}>
    {#if loading}<p class="state">{t("files.loading")}</p>{/if}
    {#if error}<p class="state error-text">{error}</p>{/if}
  </div>
  {#snippet footer()}
    <span class="hint faint">
      {#if encoding}{encodingLabel(encoding)}{language ? ` · ${language}` : ""}{/if}{dirty ? ` · ${t("editor.unsaved")}` : ""}
    </span>
    <button class="btn" onclick={requestClose}>{t("common.close")}</button>
    <button class="btn primary" onclick={() => save(false)} disabled={!ready || !dirty || saving} title={IS_MAC ? "⌘S" : "Ctrl+S"}>
      {t(saving ? "editor.saving" : "editor.save")}
    </button>
  {/snippet}
</Modal>

{#if ask === "conflict"}
  <ConfirmDialog
    title={t("editor.conflictTitle")}
    message={t("editor.conflictMessage", { name: displayName(name) })}
    confirmLabel={t("editor.overwrite")}
    onconfirm={() => save(true)}
    onclose={() => (ask = null)}
  />
{:else if ask === "discard"}
  <ConfirmDialog
    title={t("editor.discardTitle")}
    message={t("editor.discardMessage", { name: displayName(name) })}
    confirmLabel={t("editor.discard")}
    onconfirm={onclose}
    onclose={() => (ask = null)}
  />
{/if}

<style>
  .where {
    margin: -6px 0 8px;
    font-size: 12px;
    word-break: break-all;
  }
  .editor {
    height: min(65vh, 720px);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .state {
    padding: 14px;
  }
  .hint {
    margin-right: auto;
    font-size: 12.5px;
  }
</style>
