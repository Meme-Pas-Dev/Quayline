<script lang="ts">
  import { t, type MessageKey } from "../i18n/index.svelte";
  import { onMount } from "svelte";
  import Icon, { type IconName } from "../components/Icon.svelte";
  import Logo from "../components/Logo.svelte";
  import { APP_NAME } from "../identity";
  import { KEYS, shortcut } from "../platform";
  import SftpBrowser from "../components/SftpBrowser.svelte";
  import Terminal from "../components/Terminal.svelte";
  import {
    closeSession,
    data,
    disconnectSession,
    openFromSession,
    reconnect,
    reload,
    sessionHost,
    resetSessionState,
    setLockedHandler,
    showView,
    subscribeTransfers,
    ui,
    type View,
  } from "../store.svelte";
  import Hosts from "../views/Hosts.svelte";
  import Keychain from "../views/Keychain.svelte";
  import SettingsView from "../views/SettingsView.svelte";

  let { onlock, onlocked }: { onlock: () => void; onlocked: () => void } = $props();

  const nav: { id: View; label: MessageKey; icon: IconName; count?: () => number }[] = [
    { id: "hosts", label: "nav.hosts", icon: "server", count: () => data.hosts.length },
    { id: "keychain", label: "nav.keychain", icon: "key", count: () => data.identities.length },
    { id: "settings", label: "nav.settings", icon: "settings" },
  ];

  const statusLabel = {
    connecting: "session.status.connecting",
    authenticating: "session.status.authenticating",
    connected: "session.status.connected",
    closed: "session.status.closed",
  } as const;

  // Menu du clic droit sur une session.
  let menu = $state<{ key: string; x: number; y: number } | null>(null);
  const menuSession = $derived(menu ? ui.sessions.find((s) => s.key === menu!.key) ?? null : null);
  const menuHost = $derived(menu ? sessionHost(menu.key) : null);
  let menuEl = $state<HTMLDivElement>();

  function openMenu(e: MouseEvent, key: string) {
    e.preventDefault();
    menu = { key, x: e.clientX, y: e.clientY };
  }

  // Garde le menu dans la fenêtre.
  $effect(() => {
    if (!menu || !menuEl) return;
    const r = menuEl.getBoundingClientRect();
    const x = Math.min(menu.x, window.innerWidth - r.width - 8);
    const y = Math.min(menu.y, window.innerHeight - r.height - 8);
    if (x !== menu.x || y !== menu.y) menu = { ...menu, x, y };
  });

  function act(fn: (key: string) => void) {
    const key = menu?.key;
    menu = null;
    if (key) fn(key);
  }

  function onkeydown(e: KeyboardEvent) {
    if (menu && e.key === "Escape") {
      menu = null;
      return;
    }
    // Ctrl+L reste au terminal (effacer l'écran) : le verrouillage est sur Ctrl+Maj+L
    // (Cmd+Maj+L sur macOS).
    if (shortcut(e) === "lock") {
      e.preventDefault();
      onlock();
    }
  }

  onMount(() => {
    setLockedHandler(onlocked);
    reload();
    subscribeTransfers();
    // Plus aucune donnée ni session côté interface une fois verrouillé.
    return resetSessionState;
  });
</script>

<svelte:window {onkeydown} onpointerdown={(e) => menu && !menuEl?.contains(e.target as Node) && (menu = null)} onblur={() => (menu = null)} />

<div class="shell">
  <aside>
    <div class="brand">
      <Logo size={30} />
      <span>{APP_NAME}</span>
    </div>

    <nav>
      {#each nav as item}
        <button class="nav-item" class:active={!ui.active && ui.view === item.id} onclick={() => showView(item.id)}>
          <Icon name={item.icon} size={17} />
          <span>{t(item.label)}</span>
          {#if item.count && data.loaded}<em>{item.count()}</em>{/if}
        </button>
      {/each}
    </nav>

    {#if ui.sessions.length}
      <h3>{t("shell.sessions")}</h3>
      <div class="sessions">
        {#each ui.sessions as s (s.key)}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div class="session-item" class:active={ui.active === s.key} oncontextmenu={(e) => openMenu(e, s.key)}>
            <button class="session-main" onclick={() => (ui.active = s.key)} title={t(statusLabel[s.status])}>
              <span class="dot {s.status}" class:pending={s.pending}></span>
              <span class="label">{s.label}</span>
              <span class="kind" title={t(s.kind === "sftp" ? "shell.sftpBrowser" : "shell.sshTerminal")}>
                <Icon name={s.kind === "sftp" ? "folder" : "terminal"} size={13} />
              </span>
            </button>
            <button class="close" onclick={() => closeSession(s.key)} aria-label={t("shell.closeSession")}>
              <Icon name="x" size={14} />
            </button>
          </div>
        {/each}
      </div>
    {/if}

    <div class="spacer"></div>

    <button class="nav-item lock" onclick={onlock} title={t("shell.lockTitle", { keys: KEYS.lock })}>
      <Icon name="lock" size={17} />
      <span>{t("shell.lock")}</span>
      <kbd>{KEYS.lockShort}</kbd>
    </button>
  </aside>

  <section class="content">
    <div class="page" class:hidden={!!ui.active}>
      {#if ui.view === "hosts"}
        <Hosts />
      {:else if ui.view === "keychain"}
        <Keychain />
      {:else}
        <SettingsView />
      {/if}
    </div>

    {#each ui.sessions as s (s.key)}
      {#key s.attempt}
        {#if s.kind === "sftp"}
          <SftpBrowser tab={s} visible={ui.active === s.key} />
        {:else}
          <Terminal tab={s} visible={ui.active === s.key} />
        {/if}
      {/key}
    {/each}
  </section>
</div>

{#if menu && menuSession}
  <div class="context-menu" bind:this={menuEl} style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
    <div class="menu-head">{menuSession.label} · {menuSession.kind === "sftp" ? "SFTP" : "SSH"}</div>
    {#if menuSession.kind === "ssh"}
      <button role="menuitem" disabled={!menuHost} onclick={() => act((k) => openFromSession(k, "other"))}>
        <Icon name="folder" size={15} /> {t("menu.openSftp")}
      </button>
    {:else}
      <button role="menuitem" disabled={!menuHost} onclick={() => act((k) => openFromSession(k, "other"))}>
        <Icon name="terminal" size={15} /> {t("menu.openSsh")}
      </button>
    {/if}
    <button role="menuitem" disabled={!menuHost} onclick={() => act((k) => openFromSession(k, "same"))}>
      <Icon name="copy" size={15} /> {t("menu.duplicateSession")}
    </button>
    <div class="sep"></div>
    <button role="menuitem" disabled={menuSession.status !== "closed" || !menuHost} onclick={() => act(reconnect)}>
      <Icon name="refresh" size={15} /> {t("menu.reconnect")}
    </button>
    <button role="menuitem" disabled={menuSession.status !== "connected"} onclick={() => act(disconnectSession)}>
      <Icon name="power" size={15} /> {t("menu.disconnect")}
    </button>
    <button role="menuitem" class="danger" onclick={() => act(closeSession)}>
      <Icon name="x" size={15} /> {t("menu.close")}
    </button>
  </div>
{/if}

<style>
  .shell {
    display: grid;
    grid-template-columns: 232px 1fr;
    height: 100%;
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-height: 0;
    padding: 18px 12px 14px;
    background: var(--bg-sidebar);
    border-right: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 2px 8px 18px;
    font-weight: 700;
    font-size: 16px;
    letter-spacing: -0.01em;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 11px;
    height: 38px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--text-muted);
    font-weight: 550;
    text-align: left;
    transition: background 0.15s, color 0.15s;
  }
  .nav-item span {
    flex: 1;
  }
  .nav-item:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .nav-item.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .nav-item em {
    font-style: normal;
    font-size: 11.5px;
    font-weight: 600;
    min-width: 22px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--text-faint);
    text-align: center;
  }
  .nav-item.active em {
    background: transparent;
    color: var(--accent);
  }
  h3 {
    margin: 18px 10px 6px;
    font-size: 11px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--text-faint);
  }
  .sessions {
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
    min-height: 0;
  }
  .session-item {
    display: flex;
    align-items: center;
    border-radius: var(--radius-sm);
    transition: background 0.15s;
  }
  .session-item:hover {
    background: var(--surface-hover);
  }
  .session-item.active {
    background: var(--accent-soft);
  }
  .session-main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 4px 0 12px;
    border: 0;
    background: transparent;
    color: var(--text-muted);
    font-weight: 550;
    text-align: left;
  }
  .session-item.active .session-main {
    color: var(--text);
  }
  .kind {
    display: grid;
    place-items: center;
    margin-left: auto;
    color: var(--text-faint);
  }
  .label {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dot {
    width: 8px;
    height: 8px;
    flex-shrink: 0;
    border-radius: 50%;
    background: var(--text-faint);
  }
  .dot.connected {
    background: var(--success);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--success) 20%, transparent);
  }
  .dot.connecting,
  .dot.authenticating {
    background: var(--warning);
    animation: pulse 1.2s ease-in-out infinite;
  }
  .dot.pending {
    background: var(--accent);
    animation: pulse 0.9s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }
  .close {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    margin-right: 4px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-faint);
    opacity: 0;
  }
  .session-item:hover .close,
  .session-item.active .close {
    opacity: 1;
  }
  .close:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  kbd {
    font-family: var(--font-ui);
    font-size: 10.5px;
    color: var(--text-faint);
    border: 1px solid var(--border);
    border-radius: 5px;
    padding: 0 5px;
  }
  .spacer {
    flex: 1;
  }
  .content {
    position: relative;
    min-width: 0;
    overflow: hidden;
  }
  .page {
    position: absolute;
    inset: 0;
    overflow: auto;
  }
  .page.hidden {
    visibility: hidden;
  }
</style>
