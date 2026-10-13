<script lang="ts">
  import { t, tn } from "../i18n/index.svelte";
  import type { FileEntry } from "../api";
  import { displayName, formatDate, formatSize } from "../format";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Icon, { type IconName } from "./Icon.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import { IS_WINDOWS } from "../platform";
  import { crumbsOf, parentOf as parentPath, type PathStyle } from "../paths";
  import { DRAG_THRESHOLD, fileDrag } from "../fileDrag.svelte";

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
    onproperties,
    onedit,
    onopenexternal,
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
    onproperties?: (entry: FileEntry) => void;
    /** Fichier distant : éditeur intégré, ou application par défaut du système. */
    onedit?: (entry: FileEntry) => void;
    onopenexternal?: (entry: FileEntry) => void;
  } = $props();

  // Colonne « Permissions », affichée ou non (préférence commune aux deux panneaux).
  const PERMS_KEY = "quayline.files.permissions";
  let showPerms = $state(readPref());
  function readPref() {
    try {
      return localStorage.getItem(PERMS_KEY) === "1";
    } catch {
      return false;
    }
  }
  function togglePerms() {
    showPerms = !showPerms;
    try {
      localStorage.setItem(PERMS_KEY, showPerms ? "1" : "0");
    } catch {
      // Préférence non retenue : sans conséquence.
    }
  }


  let showHidden = $state(false);
  let selected = $state<Set<string>>(new Set());
  let anchor = $state<string | null>(null);
  let pathInput = $state("");
  let editingPath = $state(false);
  let dropActive = $state(false);
  // x, y : position affichée ; cx, cy : curseur au clic droit.
  let menu = $state<{ x: number; y: number; cx: number; cy: number; entry: FileEntry } | null>(null);
  let menuEl = $state<HTMLDivElement>();
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

  // Fichiers locaux sous Windows : lecteurs et « \ » ; ailleurs, chemins Unix.
  const style: PathStyle = $derived(side === "local" && IS_WINDOWS ? "windows" : "unix");
  const parentOf = (p: string) => parentPath(p, style);
  const crumbs = $derived(crumbsOf(path, style, t("files.thisPc")));

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
    menu = { x: e.clientX, y: e.clientY, cx: e.clientX, cy: e.clientY, entry };
  }

  // Garde le menu entier dans la fenêtre : à gauche ou au-dessus du curseur s'il
  // dépasse à droite ou en bas. Calculé depuis le curseur (et non la position déjà
  // corrigée), avec une largeur fixe (libellés sur une ligne), pour ne décaler qu'une fois.
  $effect(() => {
    if (!menu || !menuEl) return;
    const r = menuEl.getBoundingClientRect();
    let x = menu.cx;
    let y = menu.cy;
    if (x + r.width > window.innerWidth - 8) x = Math.max(8, x - r.width);
    if (y + r.height > window.innerHeight - 8) y = Math.max(8, Math.min(y - r.height, window.innerHeight - r.height - 8));
    if (x !== menu.x || y !== menu.y) menu = { ...menu, x, y };
  });

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

  // ── Glisser vers l'autre panneau (voir fileDrag.svelte.ts) ──

  // Appui sur une ligne : devient un glisser au-delà de quelques pixels.
  let pressed: { entry: FileEntry; x: number; y: number } | null = null;

  function onrowpointerdown(entry: FileEntry, e: PointerEvent) {
    if (e.button === 0 && !e.shiftKey && !e.ctrlKey && !e.metaKey) pressed = { entry, x: e.clientX, y: e.clientY };
  }

  function onwindowpointermove(e: PointerEvent) {
    if (fileDrag.side === side) {
      fileDrag.x = e.clientX;
      fileDrag.y = e.clientY;
      return;
    }
    if (!pressed || Math.hypot(e.clientX - pressed.x, e.clientY - pressed.y) < DRAG_THRESHOLD) return;
    const { entry } = pressed;
    pressed = null;
    if (!selected.has(entry.path)) {
      selected = new Set([entry.path]);
      anchor = entry.path;
    }
    const paths = visible.filter((x) => selected.has(x.path)).map((x) => x.path);
    Object.assign(fileDrag, { side, paths, x: e.clientX, y: e.clientY });
  }

  // Relâché : le panneau d'arrivée (gestionnaire sur le panneau, appelé avant celui de
  // la fenêtre) lance le transfert ; le panneau d'origine termine le glisser.
  function onpanepointerup() {
    if (fileDrag.side && fileDrag.side !== side && fileDrag.paths.length) ondrop([...fileDrag.paths], fileDrag.side);
    dropActive = false;
  }

  function onwindowpointerup() {
    pressed = null;
    if (fileDrag.side === side) Object.assign(fileDrag, { side: null, paths: [] });
  }

  function onpanepointermove() {
    if (fileDrag.side && fileDrag.side !== side) dropActive = true;
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

<svelte:window
  onclick={() => (menu = null)}
  onpointerdown={(e) => menu && !menuEl?.contains(e.target as Node) && (menu = null)}
  onresize={() => (menu = null)}
  onpointermove={onwindowpointermove}
  onpointerup={onwindowpointerup}
  onblur={onwindowpointerup}
/>

{#if fileDrag.side === side}
  <div class="drag-ghost" style:left="{fileDrag.x + 14}px" style:top="{fileDrag.y + 10}px">
    <Icon name="file" size={14} />
    {tn("files.dragging", fileDrag.paths.length)}
  </div>
{/if}

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
  onpointermove={onpanepointermove}
  onpointerleave={() => (dropActive = false)}
  onpointerup={onpanepointerup}
>
  <header>
    <div class="title">
      <Icon name={icon} size={16} />
      <strong>{title}</strong>
    </div>
    <div class="tools">
      <button class="btn ghost icon sm" title={t("files.parent")} onclick={() => onnavigate(parentOf(path))}>
        <Icon name="arrowUp" size={16} />
      </button>
      <button class="btn ghost icon sm" title={t("files.home")} onclick={onhome}><Icon name="home" size={16} /></button>
      <button class="btn ghost icon sm" title={t("files.refresh")} onclick={onrefresh}><Icon name="refresh" size={16} /></button>
      <button class="btn ghost icon sm" title={t("files.newFolder")} onclick={() => (dialog = { kind: "mkdir" })}>
        <Icon name="folderPlus" size={16} />
      </button>
      <button
        class="btn ghost icon sm"
        class:on={showPerms}
        title={showPerms ? t("files.hidePermissions") : t("files.showPermissions")}
        onclick={togglePerms}
      >
        <Icon name="shield" size={16} />
      </button>
      <button
        class="btn ghost icon sm"
        class:on={showHidden}
        title={showHidden ? t("files.hideHidden") : t("files.showHidden", { count: hiddenCount })}
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
        title={t("files.enterPath")}
        ondblclick={() => {
          pathInput = path;
          editingPath = true;
        }}
      >
        {#each crumbs as c, i}
          {#if style === "windows" ? i > 0 : i > 1}<span class="sep">{style === "windows" ? (i === 1 ? "›" : "\\") : "/"}</span>{/if}
          <!-- svelte-ignore a11y_click_events_have_key_events -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <span
            class="crumb"
            onclick={(e) => {
              e.stopPropagation();
              onnavigate(c.path);
            }}>{displayName(c.label)}</span
          >
        {/each}
      </button>
      <button
        class="btn ghost icon sm edit"
        title={t("files.enterPath")}
        onclick={() => {
          pathInput = path;
          editingPath = true;
        }}><Icon name="edit" size={14} /></button
      >
    {/if}
  </div>

  <div class="table" class:perms={showPerms}>
    <div class="thead">
      <span>{t("files.name")}</span>
      {#if showPerms}<span>{t("files.permissions")}</span>{/if}
      <span class="num">{t("files.size")}</span>
      <span>{t("files.modified")}</span>
    </div>
    <div class="tbody">
      {#if error}
        <div class="state error"><Icon name="alert" size={18} /> {error}</div>
      {:else if loading && entries.length === 0}
        <div class="state">{t("files.loading")}</div>
      {:else if visible.length === 0}
        <div class="state">{t("files.empty")}{hiddenCount && !showHidden ? tn("files.hiddenCount", hiddenCount) : ""}</div>
      {/if}
      {#each visible as entry (entry.path)}
        <!-- svelte-ignore a11y_click_events_have_key_events -->
        <div
          class="row"
          class:selected={selected.has(entry.path)}
          class:hidden-file={entry.name.startsWith(".")}
          role="row"
          tabindex="-1"
          onclick={(e) => select(entry, e)}
          ondblclick={() => open(entry)}
          oncontextmenu={(e) => onrowcontext(entry, e)}
          onpointerdown={(e) => onrowpointerdown(entry, e)}
        >
          <span class="name">
            <span class="ficon" class:dir={entry.isDir}><Icon name={iconFor(entry)} size={16} /></span>
            <span class="label">{displayName(entry.name)}</span>
          </span>
          {#if showPerms}
            <span class="perm mono" title={entry.owner || entry.group ? `${entry.owner ?? "?"} : ${entry.group ?? "?"}` : undefined}
              >{entry.permissions ?? ""}</span
            >
          {/if}
          <span class="num">{entry.isDir ? "" : formatSize(entry.size)}</span>
          <span class="date">{formatDate(entry.modified)}</span>
        </div>
      {/each}
    </div>
  </div>

  <footer>
    <span class="count">
      {#if selection.length}
        {tn("files.selected", selection.length)}
      {:else}
        {tn("files.items", visible.length)}
      {/if}
    </span>
    <button class="btn sm-btn" disabled={!selection.length} onclick={() => onsend(selection)}>
      <Icon name={sendIcon} size={15} />
      {sendLabel}
    </button>
  </footer>

  {#if menu}
    <div class="menu" bind:this={menuEl} style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
      {#if selection.length === 1 && menu.entry.isDir}
        <button onclick={() => onnavigate(menu!.entry.path)}><Icon name="folder" size={15} /> {t("files.open")}</button>
      {/if}
      <button onclick={() => onsend(selection)}><Icon name={sendIcon} size={15} /> {sendLabel}</button>
      {#if selection.length === 1 && !menu.entry.isDir && onedit}
        <button onclick={() => onedit(menu!.entry)}><Icon name="edit" size={15} /> {t("files.editFile")}</button>
      {/if}
      {#if selection.length === 1 && !menu.entry.isDir && onopenexternal}
        <button onclick={() => onopenexternal(menu!.entry)}><Icon name="external" size={15} /> {t("files.openWith")}</button>
      {/if}
      {#if selection.length === 1}
        <button onclick={() => (dialog = { kind: "rename", entry: menu!.entry })}><Icon name="edit" size={15} /> {t("files.rename")}</button>
      {/if}
      {#if selection.length === 1 && onproperties}
        <button onclick={() => onproperties(menu!.entry)}><Icon name="info" size={15} /> {t("files.properties")}</button>
      {/if}
      <button class="danger" onclick={() => (dialog = { kind: "delete", entries: selection })}>
        <Icon name="trash" size={15} />
        {side === "local" ? t("files.moveToTrash") : t("common.delete")}
      </button>
    </div>
  {/if}
</div>

{#if dialog?.kind === "mkdir"}
  <PromptDialog
    title={t("files.newFolder")}
    label={t("files.folderName")}
    confirmLabel={t("files.create")}
    onsubmit={(name) => {
      dialog = null;
      onmkdir(name);
    }}
    onclose={() => (dialog = null)}
  />
{:else if dialog?.kind === "rename"}
  {@const entry = dialog.entry}
  <PromptDialog
    title={t("files.rename")}
    label={t("files.newName")}
    initial={entry.name}
    confirmLabel={t("files.rename")}
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
    title={side === "local" ? t("files.trashTitle") : t("files.deleteTitle")}
    message={tn(
      side === "local" ? "files.trashMessage" : list.some((e) => e.kind === "dir") ? "files.deleteDirMessage" : "files.deleteMessage",
      list.length,
      { name: displayName(list[0].name) },
    )}
    confirmLabel={side === "local" ? t("files.moveToTrash") : t("common.delete")}
    onconfirm={() => {
      ondelete(list);
      dialog = null;
    }}
    onclose={() => (dialog = null)}
  />
{/if}

<style>
  .drag-ghost {
    position: fixed;
    z-index: 100;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 7px;
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    font-size: 12.5px;
    pointer-events: none;
    white-space: nowrap;
  }
  .row {
    user-select: none;
  }
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
  .table.perms .thead,
  .table.perms .row {
    grid-template-columns: minmax(0, 1fr) 86px 84px 118px;
  }
  .perm {
    font-size: 12px;
    color: var(--text-muted, var(--text-faint));
    white-space: nowrap;
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
    position: fixed;
    z-index: 50;
    width: max-content;
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
    white-space: nowrap;
  }
  .menu button:hover {
    background: var(--surface-hover);
  }
  .menu button.danger {
    color: var(--danger);
  }
</style>
