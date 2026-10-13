<script lang="ts">
  import { IS_WINDOWS } from "../platform";
  import { baseName } from "../paths";
  import { t, tn } from "../i18n/index.svelte";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { onMount, untrack } from "svelte";
  import { api, errorMessage, type FileEntry, type SessionEvent } from "../api";
  import { displayName, formatSize } from "../format";
  import {
    clearFinishedTransfers,
    closeSession,
    data,
    handleError,
    onTransferFinished,
    reconnect,
    transfers,
    type SessionTab,
  } from "../store.svelte";
  import FilePane, { type Side } from "./FilePane.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import SessionPrompts from "./SessionPrompts.svelte";
  import PropertiesDialog from "./PropertiesDialog.svelte";
  import TextEditor from "./TextEditor.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import { toast } from "./toast.svelte";
  import { externalEdits, openExternal, overwriteExternal, stopExternal } from "../edits.svelte";

  let { tab, visible }: { tab: SessionTab; visible: boolean } = $props();

  let prompts: SessionPrompts;
  let promptActive = $state(false);
  let sftpId = $state<string | null>(null);
  let localHome = "";
  let remoteHome = "";

  type PaneState = { path: string; entries: FileEntry[]; loading: boolean; error: string | null };
  let local = $state<PaneState>({ path: "", entries: [], loading: true, error: null });
  let remote = $state<PaneState>({ path: "", entries: [], loading: true, error: null });

  let localPane = $state<FilePane>();
  let remotePane = $state<FilePane>();

  let conflict = $state<{ direction: "upload" | "download"; paths: string[]; names: string[]; destination: string } | null>(null);
  let properties = $state<{ side: Side; entry: FileEntry } | null>(null);
  let editing = $state<FileEntry | null>(null);

  const host = $derived(data.hosts.find((h) => h.id === tab.hostId));
  const ready = $derived(tab.status === "connected" && !!sftpId);
  const myTransfers = $derived(transfers.filter((tr) => tr.sftpId === sftpId));
  const editConflict = $derived(externalEdits.find((x) => x.sftpId === sftpId && x.status === "conflict"));

  // ── Chargement des listes ──

  async function loadLocal(path = local.path) {
    local.loading = true;
    try {
      const real = await api.localRealpath(path);
      local.entries = await api.localList(real);
      local.path = real;
      local.error = null;
    } catch (e) {
      if (local.entries.length) toast(errorMessage(e), "error");
      else local.error = errorMessage(e);
    } finally {
      local.loading = false;
    }
  }

  async function loadRemote(path = remote.path) {
    if (!sftpId) return;
    remote.loading = true;
    try {
      const real = await api.sftpRealpath(sftpId, path);
      remote.entries = await api.sftpList(sftpId, real);
      remote.path = real;
      remote.error = null;
    } catch (e) {
      if (remote.entries.length) handleError(e);
      else remote.error = errorMessage(e);
    } finally {
      remote.loading = false;
    }
  }

  function onEvent(e: SessionEvent) {
    if (prompts.handle(e)) return;
    switch (e.type) {
      case "status":
        tab.status = e.state;
        break;
      case "sftpReady":
        tab.status = "connected";
        remoteHome = e.home;
        loadRemote(e.home);
        break;
      case "closed":
        tab.status = "closed";
        tab.error = e.error;
        prompts.reset();
        break;
    }
  }

  // ── Opérations ──

  async function run(action: () => Promise<unknown>, refresh: () => Promise<void>, success?: string) {
    try {
      await action();
      if (success) toast(success, "success");
    } catch (e) {
      handleError(e);
    }
    await refresh();
  }

  function transfer(direction: "upload" | "download", paths: string[]) {
    if (!ready || !paths.length) return;
    const dest = direction === "upload" ? remote : local;
    const existing = new Set(dest.entries.map((e) => e.name));
    // Envoi : chemins locaux (style Windows sous Windows) ; réception : chemins du serveur.
    const style = direction === "upload" && IS_WINDOWS ? "windows" : "unix";
    const names = paths.map((p) => baseName(p, style)).filter((n) => existing.has(n));
    if (names.length) conflict = { direction, paths, names, destination: dest.path };
    else start(direction, paths, dest.path, false);
  }

  async function start(direction: "upload" | "download", paths: string[], destination: string, overwrite: boolean) {
    conflict = null;
    try {
      if (direction === "upload") await api.sftpUpload(sftpId!, paths, destination, overwrite);
      else await api.sftpDownload(sftpId!, paths, destination, overwrite);
    } catch (e) {
      handleError(e);
    }
  }

  // ── Glisser-déposer depuis le bureau (Dolphin, Nautilus…) ──

  function paneAt(x: number, y: number): Side | null {
    const el = document.elementFromPoint(x, y);
    if (el && remotePane?.element()?.contains(el)) return "remote";
    if (el && localPane?.element()?.contains(el)) return "local";
    return null;
  }

  onMount(() => {
    let disposed = false;
    api
      .sftpOpen(tab.hostId, onEvent)
      .then((id) => {
        if (disposed) api.sftpClose(id);
        else sftpId = id;
      })
      .catch((e) => onEvent({ type: "closed", error: errorMessage(e) }));
    api.localHome().then((home) => {
      localHome = home;
      loadLocal(home);
    });

    const offFinished = onTransferFinished((tr) => {
      if (tr.sftpId !== sftpId) return;
      if (tr.status === "error") toast(`${displayName(tr.name)} : ${tr.error}`, "error", 6000);
      else if (tr.status === "done" && tr.skipped) toast(tn("sftp.skipped", tr.skipped, { name: displayName(tr.name) }), "info");
      if (tr.direction === "upload" && tr.destination === remote.path) loadRemote();
      if (tr.direction === "download" && tr.destination === local.path) loadLocal();
    });

    const unlisten = getCurrentWebview().onDragDropEvent((event) => {
      if (!visible || !ready) return;
      const p = event.payload;
      if (p.type === "over" || p.type === "enter") {
        const ratio = window.devicePixelRatio || 1;
        remotePane?.setNativeDropActive(paneAt(p.position.x / ratio, p.position.y / ratio) === "remote");
      } else if (p.type === "drop") {
        const ratio = window.devicePixelRatio || 1;
        remotePane?.setNativeDropActive(false);
        if (paneAt(p.position.x / ratio, p.position.y / ratio) === "remote") transfer("upload", p.paths);
      } else {
        remotePane?.setNativeDropActive(false);
      }
    });

    return () => {
      disposed = true;
      offFinished();
      unlisten.then((f) => f());
      prompts.cancelPending();
      if (sftpId) api.sftpClose(sftpId).catch(() => {});
    };
  });

  // Déconnexion demandée depuis la liste des sessions : transferts de cette connexion
  // annulés, connexion fermée, onglet conservé avec le bandeau « Reconnecter ».
  let disconnectSeen = untrack(() => tab.disconnect);
  $effect(() => {
    if (tab.disconnect === disconnectSeen) return;
    disconnectSeen = tab.disconnect;
    const id = untrack(() => sftpId);
    if (!id) return;
    for (const tr of untrack(() => transfers)) {
      if (tr.sftpId === id && tr.status === "running") api.transferCancel(tr.id).catch(() => {});
    }
    api.sftpClose(id).catch(() => {});
    sftpId = null;
    onEvent({ type: "closed", error: null });
  });

  function percent(done: number, total: number) {
    return total ? Math.min(100, Math.round((done / total) * 100)) : 0;
  }
</script>

<div class="sftp" class:hidden={!visible}>
  <div class="bar">
    <div class="who">
      <span class="dot {tab.status}"></span>
      <strong>SFTP</strong>
      <span class="mono">{host ? `${host.username ?? ""}${host.username ? "@" : ""}${host.hostname}` : ""}</span>
    </div>
    {#if tab.status === "closed"}
      <span class="err">{tab.error ?? t("sftp.connectionClosed")}</span>
      <button class="btn sm-btn" onclick={() => reconnect(tab.key)}>{t("menu.reconnect")}</button>
      <button class="btn ghost sm-btn" onclick={() => closeSession(tab.key)}>{t("terminal.closeTab")}</button>
    {:else if !ready}
      <span class="muted">{t(tab.status === "authenticating" ? "session.status.authenticating" : "session.status.connecting")}</span>
    {/if}
  </div>

  <div class="panes">
    <FilePane
      bind:this={localPane}
      side="local"
      title={t("sftp.thisComputer")}
      icon="laptop"
      path={local.path}
      entries={local.entries}
      loading={local.loading}
      error={local.error}
      sendLabel={t("sftp.upload")}
      sendIcon="upload"
      onnavigate={(p) => loadLocal(p)}
      onrefresh={() => loadLocal()}
      onhome={() => loadLocal(localHome)}
      onmkdir={(name) => run(() => api.localMkdir(local.path, name), () => loadLocal())}
      onrename={(entry, name) => run(() => api.localRename(entry.path, name), () => loadLocal())}
      ondelete={(entries) =>
        run(() => api.localTrash(entries.map((e) => e.path)), () => loadLocal(), t("sftp.movedToTrash"))}
      onsend={(entries) => transfer("upload", entries.map((e) => e.path))}
      ondrop={(paths) => transfer("download", paths)}
      onproperties={(entry) => (properties = { side: "local", entry })}
    />

    {#if ready || remote.entries.length}
      <FilePane
        bind:this={remotePane}
        side="remote"
        title={host?.label ?? t("sftp.server")}
        icon="server"
        path={remote.path}
        entries={remote.entries}
        loading={remote.loading}
        error={remote.error}
        sendLabel={t("sftp.download")}
        sendIcon="download"
        onnavigate={(p) => loadRemote(p)}
        onrefresh={() => loadRemote()}
        onhome={() => loadRemote(remoteHome)}
        onmkdir={(name) => run(() => api.sftpMkdir(sftpId!, remote.path, name), () => loadRemote())}
        onrename={(entry, name) => run(() => api.sftpRename(sftpId!, entry.path, name), () => loadRemote())}
        ondelete={(entries) =>
          run(() => api.sftpDelete(sftpId!, entries.map((e) => e.path)), () => loadRemote(), t("sftp.deleted"))}
        onsend={(entries) => transfer("download", entries.map((e) => e.path))}
        ondrop={(paths) => transfer("upload", paths)}
        onproperties={(entry) => (properties = { side: "remote", entry })}
        onedit={(entry) => (editing = entry)}
        onopenexternal={(entry) => sftpId && openExternal(sftpId, entry.path)}
      />
    {:else}
      <div class="placeholder">
        {#if tab.status === "closed"}
          <Icon name="alert" size={22} />
          <span>{tab.error ?? t("sftp.connectionClosed")}</span>
        {:else}
          <span class="spinner"></span>
          <span>{tab.status === "authenticating" ? t("session.status.authenticating") : t("sftp.connectingTo", { host: host?.hostname ?? "" })}</span>
        {/if}
      </div>
    {/if}
  </div>

  {#if myTransfers.length}
    <div class="transfers">
      <div class="thead">
        <strong>{t("sftp.transfers")}</strong>
        <button class="btn ghost sm-btn" onclick={() => sftpId && clearFinishedTransfers(sftpId)}>{t("sftp.clearFinished")}</button>
      </div>
      <div class="tlist">
        {#each myTransfers as tr (tr.id)}
          <div class="transfer {tr.status}">
            <Icon name={tr.direction === "upload" ? "upload" : "download"} size={15} />
            <div class="tinfo">
              <div class="tline">
                <span class="tname">{displayName(tr.name)}</span>
                <span class="tmeta">
                  {#if tr.status === "running"}
                    {formatSize(tr.done)} / {formatSize(tr.total)} · {percent(tr.done, tr.total)} %
                  {:else if tr.status === "done"}
                    {t("sftp.done", { size: formatSize(tr.total) })}{tr.files > 1 ? t("sftp.files", { count: tr.files }) : ""}{tr.skipped
                      ? tn("sftp.skippedShort", tr.skipped)
                      : ""}
                  {:else if tr.status === "cancelled"}
                    {t("sftp.cancelled")}
                  {:else}
                    {tr.error}
                  {/if}
                </span>
              </div>
              <div class="progress"><span style:width="{tr.status === 'done' ? 100 : percent(tr.done, tr.total)}%"></span></div>
            </div>
            {#if tr.status === "running"}
              <button class="btn ghost icon tbtn" title={t("common.cancel")} onclick={() => api.transferCancel(tr.id)}><Icon name="x" size={14} /></button>
            {/if}
          </div>
        {/each}
      </div>
    </div>
  {/if}
</div>

<SessionPrompts bind:this={prompts} bind:active={promptActive} {tab} {visible} />

{#if visible && properties}
  {@const p = properties}
  <PropertiesDialog
    entry={p.entry}
    load={() => (p.side === "remote" ? api.sftpStat(sftpId!, p.entry.path) : api.localStat(p.entry.path))}
    canChmod={p.side === "remote" ? ready : !IS_WINDOWS}
    onchmod={async (mode) => {
      if (p.side === "remote") await api.sftpChmod(sftpId!, p.entry.path, mode);
      else await api.localChmod(p.entry.path, mode);
      toast(t("props.applied"), "success");
      if (p.side === "remote") loadRemote();
      else loadLocal();
    }}
    onclose={() => (properties = null)}
  />
{/if}

{#if visible && editing && sftpId}
  <TextEditor {sftpId} path={editing.path} name={editing.name} onclose={() => (editing = null)} />
{/if}

{#if visible && editConflict}
  {@const ed = editConflict}
  <ConfirmDialog
    title={t("editor.conflictTitle")}
    message={t("editor.externalConflictMessage", { name: displayName(ed.name) })}
    confirmLabel={t("editor.overwrite")}
    onconfirm={() => overwriteExternal(ed.id)}
    onclose={() => stopExternal(ed.id)}
  />
{/if}

{#if visible && conflict}
  {@const c = conflict}
  <Modal title={t("sftp.conflictTitle")} onclose={() => (conflict = null)} width={460}>
    <p class="muted">
      {c.names.length === 1 ? t("sftp.conflictOne", { name: c.names[0] }) : t("sftp.conflictMany", { count: c.names.length })}{t(
        "sftp.conflictIn",
      )}<code>{c.destination}</code>{t("sftp.conflictQuestion")}
    </p>
    {#snippet footer()}
      <button class="btn" onclick={() => (conflict = null)}>{t("common.cancel")}</button>
      <button class="btn" onclick={() => start(c.direction, c.paths, c.destination, false)}>{t("sftp.skipExisting")}</button>
      <button class="btn primary" onclick={() => start(c.direction, c.paths, c.destination, true)}>{t("sftp.replace")}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .sftp {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px 14px;
    background: var(--bg);
  }
  .sftp.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 32px;
    padding: 0 4px;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 9px;
    flex: 1;
    min-width: 0;
  }
  .who .mono {
    color: var(--text-muted);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-faint);
    flex-shrink: 0;
  }
  .dot.connected {
    background: var(--success);
  }
  .dot.connecting,
  .dot.authenticating {
    background: var(--warning);
  }
  .err {
    color: var(--danger);
    font-size: 13px;
  }
  .muted {
    color: var(--text-muted);
    font-size: 13px;
  }
  .sm-btn {
    height: 30px;
    font-size: 13px;
  }
  .panes {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius);
    color: var(--text-muted);
    text-align: center;
    padding: 20px;
  }
  .placeholder :global(svg) {
    color: var(--danger);
  }
  .spinner {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .transfers {
    display: flex;
    flex-direction: column;
    max-height: 190px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .transfers .thead {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 4px 14px;
    font-size: 13px;
  }
  .tlist {
    overflow-y: auto;
    padding: 0 8px 8px;
  }
  .transfer {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 7px 6px;
    border-radius: 8px;
  }
  .transfer > :global(svg) {
    color: var(--accent);
    flex-shrink: 0;
  }
  .transfer.error > :global(svg) {
    color: var(--danger);
  }
  .transfer.done > :global(svg) {
    color: var(--success);
  }
  .tinfo {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .tline {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 12.5px;
  }
  .tname {
    font-weight: 550;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tmeta {
    color: var(--text-muted);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .transfer.error .tmeta {
    color: var(--danger);
    white-space: normal;
    text-align: right;
  }
  .progress {
    height: 4px;
    border-radius: 4px;
    background: var(--border);
    overflow: hidden;
  }
  .progress span {
    display: block;
    height: 100%;
    background: var(--gradient);
    transition: width 0.2s;
  }
  .transfer.done .progress span {
    background: var(--success);
  }
  .transfer.error .progress span,
  .transfer.cancelled .progress span {
    background: var(--text-faint);
  }
  .tbtn {
    width: 28px;
    height: 28px;
    color: var(--text-faint);
  }
</style>
