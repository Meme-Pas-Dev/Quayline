<script lang="ts">
  import type { FileEntry } from "../api";
  import { displayName, formatDate, formatSize } from "../format";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import PromptDialog from "./PromptDialog.svelte";

  export type Side = "local" | "remote";

  let {
    side,
    title,
    icon,
    path,
    entries,
    loading = false,
    error = null,
    sendLabel,
    sendIcon,
    onnavigate,
    onrefresh,
    onhome,
    onmkdir,
    onrename,
    ondelete,
    onsend,
    ondrop,
  }: {
    side: Side;
    title: string;
    icon: IconName;
    path: string;
    entries: FileEntry[];
    loading?: boolean;
    error?: string | null;
    sendLabel: string;
    sendIcon: IconName;
    onnavigate: (path: string) => void;
    onrefresh: () => void;
    onhome: () => void;
    onmkdir: (name: string) => void;
    onrename: (entry: FileEntry, newName: string) => void;
    ondelete: (entries: FileEntry[]) => void;
    onsend: (entries: FileEntry[]) => void;
    /** Fichiers lâchés depuis l'autre panneau. */
    ondrop: (paths: string[], from: Side) => void;
  } = $props();

  const DRAG_TYPE = "application/x-app-files";

  let showHidden = $state(false);
  let selected = $state<Set<string>>(new Set());
  let anchor = $state<string | null>(null);
  let pathInput = $state("");
  let editingPath = $state(false);
  let dropActive = $state(false);
  let menu = $state<{ x: number; y: number; entry: FileEntry } | null>(null);
  let dialog = $state<
    { kind: "mkdir" } | { kind: "rename"; entry: FileEntry } | { kind: "delete"; entries: FileEntry[] } | null
  >(null);
  let paneEl: HTMLDivElement;

  const visible = $derived(showHidden ? entries : entries.filter((e) => !e.name.startsWith(".")));
  const selection = $derived(visible.filter((e) => selected.has(e.path)));
  const hiddenCount = $derived(entries.length - entries.filter((e) => !e.name.startsWith(".")).length);

  // Nouveau dossier affiché : sélection remise à zéro.
  $effect(() => {
    path;
    selected = new Set();
    anchor = null;
  });

  export function element() {
    return paneEl;
  }

  function parentOf(p: string) {
    if (p === "/" || p === "") return "/";
    const trimmed = p.replace(/\/+$/, "");
    const i = trimmed.lastIndexOf("/");
    return i <= 0 ? "/" : trimmed.slice(0, i);
  }

  const crumbs = $derived.by(() => {
    const parts = path.split("/").filter(Boolean);
    return [{ label: "/", path: "/" }, ...parts.map((p, i) => ({ label: p, path: "/" + parts.slice(0, i + 1).join("/") }))];
  });

  function select(entry: FileEntry, e: MouseEvent) {
    const next = new Set(e.ctrlKey || e.metaKey ? selected : []);
    if (e.shiftKey && anchor) {
      const a = visible.findIndex((x) => x.path === anchor);
      const b = visible.findIndex((x) => x.path === entry.path);
      const [from, to] = a < b ? [a, b] : [b, a];
      for (const x of visible.slice(from, to + 1)) next.add(x.path);
    } else if ((e.ctrlKey || e.metaKey) && selected.has(entry.path)) {
      next.delete(entry.path);
    } else {
      next.add(entry.path);
      anchor = entry.path;
    }
    selected = next;
  }

  function open(entry: FileEntry) {
    if (entry.isDir) onnavigate(entry.path);
    else onsend([entry]);
  }

  function onrowcontext(entry: FileEntry, e: MouseEvent) {
    e.preventDefault();
    if (!selected.has(entry.path)) {
      selected = new Set([entry.path]);
      anchor = entry.path;
    }
    const rect = paneEl.getBoundingClientRect();
    menu = { x: e.clientX - rect.left, y: e.clientY - rect.top, entry };
  }

  function onkeydown(e: KeyboardEvent) {
    if (dialog || editingPath || (e.target as HTMLElement).tagName === "INPUT") return;
    if (e.key === "Delete" && selection.length) {
      e.preventDefault();
      dialog = { kind: "delete", entries: selection };
    } else if (e.key === "F2" && selection.length === 1) {
      e.preventDefault();
      dialog = { kind: "rename", entry: selection[0] };
    } else if (e.key === "Enter" && selection.length === 1) {
      e.preventDefault();
      open(selection[0]);
    } else if (e.key === "Backspace") {
      e.preventDefault();
      onnavigate(parentOf(path));
    } else if (e.key === "F5") {
      e.preventDefault();
      onrefresh();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      selected = new Set(visible.map((x) => x.path));
    } else if (e.key === "Escape") {
      selected = new Set();
      menu = null;
    }
  }

  function ondragstart(entry: FileEntry, e: DragEvent) {
    if (!selected.has(entry.path)) {
      selected = new Set([entry.path]);
      anchor = entry.path;
    }
    const paths = visible.filter((x) => selected.has(x.path)).map((x) => x.path);
    e.dataTransfer?.setData(DRAG_TYPE, JSON.stringify({ side, paths }));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "copy";
  }

  function acceptsDrag(e: DragEvent) {
    return !!e.dataTransfer?.types.includes(DRAG_TYPE);
  }

  function ondragover(e: DragEvent) {
    if (!acceptsDrag(e)) return;
    e.preventDefault();
    dropActive = true;
  }

  function ondropEvent(e: DragEvent) {
    dropActive = false;
    const raw = e.dataTransfer?.getData(DRAG_TYPE);
    if (!raw) return;
    e.preventDefault();
    try {
      const { side: from, paths } = JSON.parse(raw) as { side: Side; paths: string[] };
      if (from !== side && paths.length) ondrop(paths, from);
    } catch {
      /* contenu inattendu : ignoré */
    }
  }

  /** Surbrillance pendant un glisser-déposer depuis le gestionnaire de fichiers du bureau. */
  export function setNativeDropActive(active: boolean) {
    dropActive = active;
  }

  function submitPath(e: SubmitEvent) {
    e.preventDefault();
    editingPath = false;
    if (pathInput.trim()) onnavigate(pathInput.trim());
  }

  function iconFor(entry: FileEntry): IconName {
    if (entry.isDir) return "folder";
    if (entry.kind === "link") return "link";
    return "file";
  }
</script>

<svelte:window onclick={() => (menu = null)} />

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="pane"
  class:drop={dropActive}
  bind:this={paneEl}
  tabindex="0"
  role="region"
  aria-label={title}
  {onkeydown}
  {ondragover}
  ondragleave={() => (dropActive = false)}
  ondrop={ondropEvent}
>
  <header>
    <div class="title">
      <Icon name={icon} size={16} />
      <strong>{title}</strong>
    </div>
    <div class="tools">
      <button class="btn ghost icon sm" title="Dossier parent (Retour arrière)" onclick={() => onnavigate(parentOf(path))}>
        <Icon name="arrowUp" size={16} />
      </button>
      <button class="btn ghost icon sm" title="Dossier personnel" onclick={onhome}><Icon name="home" size={16} /></button>
      <button class="btn ghost icon sm" title="Actualiser (F5)" onclick={onrefresh}><Icon name="refresh" size={16} /></button>
      <button class="btn ghost icon sm" title="Nouveau dossier" onclick={() => (dialog = { kind: "mkdir" })}>
        <Icon name="folderPlus" size={16} />
      </button>
      <button
        class="btn ghost icon sm"
        class:on={showHidden}
        title={showHidden ? "Masquer les fichiers cachés" : `Afficher les fichiers cachés (${hiddenCount})`}
        onclick={() => (showHidden = !showHidden)}
      >
        <Icon name={showHidden ? "eye" : "eyeOff"} size={16} />
      </button>
    </div>
  </header>

  <div class="path">
    {#if editingPath}
      <form onsubmit={submitPath}>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          class="input mono"
          bind:value={pathInput}
          autofocus
          spellcheck="false"
          onblur={() => (editingPath = false)}
          onkeydown={(e) => e.key === "Escape" && (editingPath = false)}
        />
      </form>
    {:else}
      <button
        class="crumbs"
        title="Saisir un chemin"
        ondblclick={() => {
          pathInput = path;
          editingPath = true;
        }}
      >
        {#each crumbs as c, i}
          {#if i > 1}<span class="sep">/</span>{/if}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span
            class="crumb"
            onclick={(e) => {
              e.stopPropagation();
              onnavigate(c.path);
            }}>{c.label}</span
          >
        {/each}
      </button>
      <button
        class="btn ghost icon sm edit"
        title="Saisir un chemin"
        onclick={() => {
          pathInput = path;
          editingPath = true;
        }}><Icon name="edit" size={14} /></button
      >
    {/if}
  </div>

  <div class="table">
    <div class="thead">
      <span>Nom</span>
      <span class="num">Taille</span>
      <span>Modifié</span>
    </div>
    <div class="tbody">
      {#if error}
        <div class="state error"><Icon name="alert" size={18} /> {error}</div>
      {:else if loading && entries.length === 0}
        <div class="state">Chargement…</div>
      {:else if visible.length === 0}
        <div class="state">Dossier vide{hiddenCount && !showHidden ? ` (${hiddenCount} fichier(s) caché(s))` : ""}</div>
      {/if}
      {#each visible as entry (entry.path)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row"
          class:selected={selected.has(entry.path)}
          class:hidden-file={entry.name.startsWith(".")}
          role="row"
          tabindex="-1"
          draggable="true"
          onclick={(e) => select(entry, e)}
          ondblclick={() => open(entry)}
          oncontextmenu={(e) => onrowcontext(entry, e)}
          ondragstart={(e) => ondragstart(entry, e)}
        >
          <span class="name">
            <span class="ficon" class:dir={entry.isDir}><Icon name={iconFor(entry)} size={16} /></span>
            <span class="label">{displayName(entry.name)}</span>
          </span>
          <span class="num">{entry.isDir ? "" : formatSize(entry.size)}</span>
          <span class="date">{formatDate(entry.modified)}</span>
        </div>
      {/each}
    </div>
  </div>

  <footer>
    <span class="count">
      {#if selection.length}
        {selection.length} sélectionné{selection.length > 1 ? "s" : ""}
      {:else}
        {visible.length} élément{visible.length > 1 ? "s" : ""}
      {/if}
    </span>
    <button class="btn sm-btn" disabled={!selection.length} onclick={() => onsend(selection)}>
      <Icon name={sendIcon} size={15} />
      {sendLabel}
    </button>
  </footer>

  {#if menu}
    <div class="menu" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
      {#if selection.length === 1 && menu.entry.isDir}
        <button onclick={() => onnavigate(menu!.entry.path)}><Icon name="folder" size={15} /> Ouvrir</button>
      {/if}
      <button onclick={() => onsend(selection)}><Icon name={sendIcon} size={15} /> {sendLabel}</button>
      {#if selection.length === 1}
        <button onclick={() => (dialog = { kind: "rename", entry: menu!.entry })}><Icon name="edit" size={15} /> Renommer</button>
      {/if}
      <button class="danger" onclick={() => (dialog = { kind: "delete", entries: selection })}>
        <Icon name="trash" size={15} />
        {side === "local" ? "Mettre à la corbeille" : "Supprimer"}
      </button>
    </div>
  {/if}
</div>

{#if dialog?.kind === "mkdir"}
  <PromptDialog
    title="Nouveau dossier"
    label="Nom du dossier"
    confirmLabel="Créer"
    onsubmit={(name) => {
      dialog = null;
      onmkdir(name);
    }}
    onclose={() => (dialog = null)}
  />
{:else if dialog?.kind === "rename"}
  {@const entry = dialog.entry}
  <PromptDialog
    title="Renommer"
    label="Nouveau nom"
    initial={entry.name}
    confirmLabel="Renommer"
    onsubmit={(name) => {
      // Agir avant de fermer : `entry` est dérivé de `dialog`.
      if (name !== entry.name) onrename(entry, name);
      dialog = null;
    }}
    onclose={() => (dialog = null)}
  />
{:else if dialog?.kind === "delete"}
  {@const list = dialog.entries}
  <ConfirmDialog
    title={side === "local" ? "Mettre à la corbeille ?" : "Supprimer définitivement ?"}
    message={(list.length === 1 ? `« ${displayName(list[0].name)} »` : `${list.length} éléments`) +
      (side === "local"
        ? " sera déplacé vers la corbeille."
        : (list.some((e) => e.kind === "dir") ? " et tout son contenu" : "") + " sera supprimé du serveur. Cette action est irréversible.")}
    confirmLabel={side === "local" ? "Mettre à la corbeille" : "Supprimer"}
    onconfirm={() => {
      ondelete(list);
      dialog = null;
    }}
    onclose={() => (dialog = null)}
  />
{/if}

<style>
  .pane {
    position: relative;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    outline: none;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .pane:focus-within {
    border-color: var(--border-strong);
  }
  .pane.drop {
    border-color: var(--accent);
    box-shadow: var(--ring);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 8px 4px 14px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .title :global(svg) {
    color: var(--accent);
    flex-shrink: 0;
  }
  .title strong {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tools {
    display: flex;
    gap: 2px;
  }
  .sm {
    width: 30px;
    height: 30px;
    color: var(--text-muted);
  }
  .sm.on {
    color: var(--accent);
  }
  .path {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 2px 8px 8px 10px;
    border-bottom: 1px solid var(--border);
  }
  .path form {
    flex: 1;
  }
  .path .input {
    height: 30px;
  }
  .crumbs {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 2px;
    height: 30px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: var(--bg);
    font-family: var(--font-mono);
    font-size: 12px;
    overflow: hidden;
    white-space: nowrap;
    text-align: left;
    cursor: text;
  }
  .crumb {
    padding: 2px 4px;
    border-radius: 4px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .crumb:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .crumb:last-child {
    color: var(--text);
  }
  .sep {
    color: var(--text-faint);
  }
  .edit {
    width: 28px;
    height: 28px;
  }
  .table {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .thead,
  .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 84px 118px;
    gap: 10px;
    align-items: center;
    padding: 0 12px;
  }
  .thead {
    height: 30px;
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text-faint);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    border-bottom: 1px solid var(--border);
  }
  .tbody {
    flex: 1;
    overflow-y: auto;
    padding: 4px;
  }
  .row {
    height: 32px;
    border-radius: 7px;
    cursor: default;
    font-size: 13px;
  }
  .row:hover {
    background: var(--surface-hover);
  }
  .row.selected {
    background: var(--accent-soft);
  }
  .row.hidden-file .label {
    opacity: 0.6;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
  }
  .ficon {
    display: grid;
    place-items: center;
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .ficon.dir {
    color: var(--accent);
  }
  .label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--text-muted);
  }
  .date {
    color: var(--text-muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .state {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 40px 16px;
    color: var(--text-faint);
    text-align: center;
  }
  .state.error {
    color: var(--danger);
  }
  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 10px 8px 14px;
    border-top: 1px solid var(--border);
  }
  .count {
    font-size: 12px;
    color: var(--text-faint);
  }
  .sm-btn {
    height: 30px;
    font-size: 13px;
  }
  .menu {
    position: absolute;
    z-index: 20;
    min-width: 200px;
    padding: 5px;
    background: var(--surface-2);
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    box-shadow: var(--shadow-lg);
  }
  .menu button {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    font-size: 13px;
    text-align: left;
  }
  .menu button:hover {
    background: var(--surface-hover);
  }
  .menu button.danger {
    color: var(--danger);
  }
</style>
