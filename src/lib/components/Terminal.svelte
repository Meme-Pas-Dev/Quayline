<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { FitAddon } from "@xterm/addon-fit";
  import { WebglAddon } from "@xterm/addon-webgl";
  import { Terminal } from "@xterm/xterm";
  import { onMount, untrack } from "svelte";
  import { api, errorMessage, type SessionEvent } from "../api";
  import { IS_MAC, KEYS, shortcut } from "../platform";
  import { closeSession, data, handleError, reconnect, type SessionTab } from "../store.svelte";
  import { TERMINAL_THEMES } from "../terminalThemes";
  import Icon from "./Icon.svelte";
  import SessionPrompts from "./SessionPrompts.svelte";

  let { tab, visible }: { tab: SessionTab; visible: boolean } = $props();

  const FONT = '"JetBrains Mono Variable", "JetBrains Mono", "Noto Sans Mono", monospace';

  let container: HTMLDivElement;
  let term: Terminal | null = null;
  let fit: FitAddon | null = null;
  let sessionId: string | null = null;

  let prompts: SessionPrompts;
  let promptActive = $state(false);
  let sessionEl: HTMLDivElement;
  let menu = $state<{ x: number; y: number; hasSelection: boolean } | null>(null);

  function oncontextmenu(e: MouseEvent) {
    if (!term) return;
    e.preventDefault();
    const rect = sessionEl.getBoundingClientRect();
    // Garder le menu dans la fenêtre.
    const x = Math.min(e.clientX - rect.left, rect.width - 230);
    const y = Math.min(e.clientY - rect.top, rect.height - 180);
    menu = { x, y, hasSelection: term.hasSelection() };
  }

  function menuAction(action: () => void) {
    menu = null;
    action();
    term?.focus();
  }

  const host = $derived(data.hosts.find((h) => h.id === tab.hostId));
  // Mêmes opérations pour une session SSH et pour un terminal local.
  const local = untrack(() => tab.kind === "local");
  const backend = local
    ? {
        open: (cols: number, rows: number, onEvent: (e: SessionEvent) => void, onData: (b: Uint8Array) => void) =>
          api.localTermOpen(cols, rows, onEvent, onData),
        write: api.localTermWrite,
        paste: api.localTermPaste,
        resize: api.localTermResize,
        close: api.localTermClose,
      }
    : {
        open: (cols: number, rows: number, onEvent: (e: SessionEvent) => void, onData: (b: Uint8Array) => void) =>
          api.sshOpen(untrack(() => tab.hostId), cols, rows, onEvent, onData),
        write: api.sshWrite,
        paste: api.sshPaste,
        resize: api.sshResize,
        close: api.sshClose,
      };

  function themeFor(id: string | undefined) {
    return (TERMINAL_THEMES[id ?? ""] ?? TERMINAL_THEMES.night).theme;
  }

  function refit() {
    if (!term || !fit || !visible || !container.offsetParent) return;
    try {
      fit.fit();
    } catch {
      /* conteneur pas encore dimensionné */
    }
  }

  function onEvent(e: SessionEvent) {
    if (prompts.handle(e)) return;
    switch (e.type) {
      case "status":
        tab.status = e.state;
        break;
      case "connected":
        tab.status = "connected";
        refit();
        if (visible) term?.focus();
        break;
      case "closed":
        tab.status = "closed";
        tab.error = e.error;
        prompts.reset();
        sessionId = null;
        term?.write(`\r\n\x1b[2m── ${t(e.error ? "terminal.interrupted" : "terminal.ended")} ──\x1b[0m\r\n`);
        break;
    }
  }

  // Déconnexion demandée depuis la liste des sessions : la connexion se ferme, le
  // contenu reste affiché avec le bandeau « Reconnecter ». La valeur au montage est
  // ignorée (le terminal est recréé à chaque reconnexion).
  let disconnectSeen = untrack(() => tab.disconnect);
  $effect(() => {
    if (tab.disconnect === disconnectSeen) return;
    disconnectSeen = tab.disconnect;
    if (sessionId) backend.close(sessionId).catch(() => {});
  });

  async function copySelection() {
    const text = term?.getSelection();
    if (text) await writeText(text);
  }

  // Le presse-papiers est lu par le backend : l'interface n'y a pas accès.
  async function paste() {
    if (!sessionId || tab.status !== "connected" || !term) return;
    await backend.paste(sessionId, term.modes.bracketedPasteMode).catch(handleError);
    term.scrollToBottom();
  }

  // macOS : « Coller » du menu Édition déclenche un collage natif dans xterm ; il est
  // redirigé vers le collage par le backend, comme le raccourci.
  function onpaste(e: ClipboardEvent) {
    if (!IS_MAC) return;
    e.preventDefault();
    e.stopPropagation();
    paste();
  }

  onMount(() => {
    let disposed = false;
    const settings = data.settings;
    const terminal = new Terminal({
      fontFamily: FONT,
      fontSize: settings?.terminal_font_size ?? 14,
      lineHeight: 1.15,
      theme: themeFor(settings?.terminal_theme),
      cursorBlink: true,
      scrollback: 10000,
      allowProposedApi: false,
      // Liens OSC 8 envoyés par le serveur : désactivés (le texte affiché peut
      // masquer une tout autre adresse).
      linkHandler: { activate: () => {} },
      macOptionIsMeta: true,
    });
    term = terminal;
    fit = new FitAddon();
    terminal.loadAddon(fit);

    // Copier/coller (Ctrl+Maj+C/V, ou Cmd+C/V sur macOS) sans interférer avec Ctrl+C (SIGINT).
    terminal.attachCustomKeyEventHandler((e) => {
      if (e.type !== "keydown") return true;
      const action = shortcut(e);
      if (action === "copy" || action === "paste") {
        // Empêche aussi l'action native de WebKit, sinon le texte est collé deux fois.
        e.preventDefault();
        if (action === "copy") copySelection();
        else paste();
        return false;
      }
      return action !== "lock" && action !== "localTerm"; // Gérés par la fenêtre.
    });

    terminal.onData((input) => {
      if (sessionId && tab.status === "connected") backend.write(sessionId, input).catch(() => {});
    });
    terminal.onResize(({ cols, rows }) => {
      if (sessionId) backend.resize(sessionId, cols, rows).catch(() => {});
    });

    const observer = new ResizeObserver(() => refit());

    (async () => {
      // La police doit être chargée avant de mesurer les cellules.
      await document.fonts.load(`${settings?.terminal_font_size ?? 14}px "JetBrains Mono Variable"`).catch(() => {});
      if (disposed) return;
      terminal.open(container);
      try {
        const webgl = new WebglAddon();
        webgl.onContextLoss(() => webgl.dispose());
        terminal.loadAddon(webgl);
      } catch {
        /* rendu DOM par défaut si WebGL est indisponible */
      }
      observer.observe(container);
      refit();
      try {
        const id = await backend.open(terminal.cols, terminal.rows, onEvent, (bytes) => terminal.write(bytes));
        if (disposed) backend.close(id);
        else sessionId = id;
      } catch (e) {
        onEvent({ type: "closed", error: errorMessage(e) });
      }
    })();

    return () => {
      disposed = true;
      observer.disconnect();
      if (sessionId) backend.close(sessionId).catch(() => {});
      prompts.cancelPending();
      terminal.dispose();
      term = null;
    };
  });

  // Onglet affiché : ajuster la taille et donner le focus.
  $effect(() => {
    if (visible) {
      requestAnimationFrame(() => {
        refit();
        if (!promptActive) term?.focus();
      });
    }
  });

  // Réglages modifiés : appliquer thème et taille de police à chaud.
  $effect(() => {
    const s = data.settings;
    if (!term || !s) return;
    term.options.theme = themeFor(s.terminal_theme);
    term.options.fontSize = s.terminal_font_size;
    refit();
  });

  const background = $derived(themeFor(data.settings?.terminal_theme).background);
</script>

<svelte:window onclick={() => (menu = null)} onkeydown={(e) => e.key === "Escape" && (menu = null)} />

<div class="session" class:hidden={!visible} style:--term-bg={background} bind:this={sessionEl}>
  <!-- Le padding est sur le parent : FitAddon mesure la hauteur du conteneur
       direct, qui inclurait le padding en box-sizing: border-box. -->
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="terminal-wrap" {oncontextmenu} onpastecapture={onpaste}><div class="terminal" bind:this={container}></div></div>

  {#if menu}
    <div class="menu" style:left="{menu.x}px" style:top="{menu.y}px" role="menu">
      <button disabled={!menu.hasSelection} onclick={() => menuAction(copySelection)}>
        <Icon name="copy" size={15} /> {t("common.copy")} <kbd>{KEYS.copy}</kbd>
      </button>
      <button onclick={() => menuAction(paste)}>
        <Icon name="clipboard" size={15} /> {t("terminal.paste")} <kbd>{KEYS.paste}</kbd>
      </button>
      <button onclick={() => menuAction(() => term?.selectAll())}>
        <Icon name="check" size={15} /> {t("terminal.selectAll")}
      </button>
      <div class="sep"></div>
      <button onclick={() => menuAction(() => {
          term?.clearSelection();
          term?.clear();
        })}>
        <Icon name="trash" size={15} /> {t("terminal.clearScreen")}
      </button>
    </div>
  {/if}

<SessionPrompts bind:this={prompts} bind:active={promptActive} {tab} {visible} onanswered={() => term?.focus()} />

  {#if tab.status === "connecting" || tab.status === "authenticating"}
    {#if !promptActive}
      <div class="overlay">
        <div class="card">
          <span class="spinner"></span>
          <div>
            <strong>{t(tab.status === "connecting" ? "session.status.connecting" : "session.status.authenticating")}</strong>
            <span class="mono">{local ? t("localTerm.label") : host ? `${host.hostname}:${host.port}` : ""}</span>
          </div>
        </div>
      </div>
    {/if}
  {:else if tab.status === "closed"}
    <div class="closed-bar" class:error={!!tab.error}>
      <Icon name={tab.error ? "alert" : "check"} size={16} />
      <span>{tab.error ?? t("terminal.endedDot")}</span>
      <div class="spacer"></div>
      <button class="btn" onclick={() => reconnect(tab.key)}>{t(local ? "menu.restart" : "menu.reconnect")}</button>
      <button class="btn ghost" onclick={() => closeSession(tab.key)}>{t("terminal.closeTab")}</button>
    </div>
  {/if}
</div>

<style>
  .session {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: var(--term-bg);
  }
  .session.hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .terminal-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    padding: 10px 6px 6px 14px;
  }
  .terminal {
    flex: 1;
    min-width: 0;
    min-height: 0;
  }
  .terminal :global(.xterm) {
    height: 100%;
  }
  .terminal :global(.xterm-viewport) {
    background: transparent !important;
  }
  .menu {
    position: absolute;
    z-index: 20;
    min-width: 220px;
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
    color: var(--text);
    font-size: 13px;
    text-align: left;
  }
  .menu button:hover:not(:disabled) {
    background: var(--surface-hover);
  }
  .menu button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .menu kbd {
    margin-left: auto;
    font-family: var(--font-ui);
    font-size: 11px;
    color: var(--text-faint);
  }
  .menu .sep {
    height: 1px;
    margin: 4px 6px;
    background: var(--border);
  }
  .overlay {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    pointer-events: none;
  }
  .overlay .card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px 22px;
    border-radius: var(--radius);
    background: var(--surface);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
  }
  .overlay .card div {
    display: flex;
    flex-direction: column;
  }
  .overlay .card span.mono {
    color: var(--text-muted);
    font-size: 12px;
  }
  .spinner {
    width: 20px;
    height: 20px;
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
  .closed-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: var(--surface);
    border-top: 1px solid var(--border);
    color: var(--text-muted);
  }
  .closed-bar.error :global(svg) {
    color: var(--danger);
  }
  .closed-bar.error span {
    color: var(--text);
  }
  .spacer {
    flex: 1;
  }
</style>
