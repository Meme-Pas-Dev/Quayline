<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import { tick } from "svelte";
  import { api, type Host } from "../api";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import HostForm from "../components/HostForm.svelte";
  import Icon from "../components/Icon.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import { toast } from "../components/toast.svelte";
  import { data, handleError, identityName, openSession, reload } from "../store.svelte";
  import { display } from "../display.svelte";

  // Affichage des tuiles (Réglages → Général).
  const compact = $derived(display.tiles === "compact");

  let query = $state("");
  let editing = $state<Host | null | undefined>(undefined); // undefined = fermé, null = création
  let duplicating = $state<Host | null>(null);
  let deleting = $state<Host | null>(null);

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return data.hosts;
    return data.hosts.filter((h) =>
      [h.label, h.hostname, h.username, h.group, ...h.tags].some((v) => v?.toLowerCase().includes(q)),
    );
  });

  const groups = $derived.by(() => {
    const map = new Map<string, Host[]>();
    for (const h of [...filtered].sort((a, b) => a.label.localeCompare(b.label))) {
      const key = h.group ?? "";
      if (!map.has(key)) map.set(key, []);
      map.get(key)!.push(h);
    }
    return [...map.entries()].sort(([a], [b]) => (a === "" ? 1 : b === "" ? -1 : a.localeCompare(b)));
  });

  // Un clic sur la tuile ouvre le type principal de l'hôte ; le bouton secondaire, l'autre.
  function connect(h: Host) {
    openSession(h, h.kind);
  }

  const other = (h: Host) => (h.kind === "sftp" ? "ssh" : "sftp");

  // Menu d'actions d'une tuile : bouton ☰ ou clic droit, comme pour les sessions.
  let menu = $state<{ host: Host; x: number; y: number } | null>(null);
  let menuEl = $state<HTMLDivElement>();
  let menuButton: HTMLElement | null = null;

  async function openMenu(host: Host, x: number, y: number, button: HTMLElement | null = null) {
    menuButton = button;
    menu = { host, x, y };
    await tick();
    menuEl?.querySelector<HTMLButtonElement>("button")?.focus();
  }

  function onMenuButton(e: MouseEvent, host: Host) {
    if (menu?.host.id === host.id) return closeMenu();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    // Sous le bouton, aligné sur son bord droit (recalé dans la fenêtre ci-dessous).
    openMenu(host, r.right - (menuEl?.offsetWidth ?? 220), r.bottom + 4, e.currentTarget as HTMLElement);
  }

  function onContextMenu(e: MouseEvent, host: Host) {
    e.preventDefault();
    openMenu(host, e.clientX, e.clientY);
  }

  function closeMenu(refocus = false) {
    menu = null;
    if (refocus) menuButton?.focus();
    menuButton = null;
  }

  // Garde le menu dans la fenêtre (le bouton ☰ l'aligne à droite une fois sa largeur connue).
  $effect(() => {
    if (!menu || !menuEl) return;
    const r = menuEl.getBoundingClientRect();
    let x = menu.x;
    if (menuButton) x = menuButton.getBoundingClientRect().right - r.width;
    x = Math.max(8, Math.min(x, window.innerWidth - r.width - 8));
    let y = menu.y;
    // Pas la place en dessous du bouton : au-dessus.
    if (y + r.height > window.innerHeight - 8) {
      y = menuButton ? menuButton.getBoundingClientRect().top - r.height - 4 : window.innerHeight - r.height - 8;
    }
    y = Math.max(8, y);
    if (x !== menu.x || y !== menu.y) menu = { ...menu, x, y };
  });

  function act(fn: (h: Host) => void) {
    const host = menu?.host;
    closeMenu();
    if (host) fn(host);
  }

  // Échap ferme ; flèches, Début et Fin parcourent les entrées.
  function onMenuKeydown(e: KeyboardEvent) {
    const items = [...(menuEl?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
    const i = items.indexOf(document.activeElement as HTMLButtonElement);
    const move: Record<string, number> = { ArrowDown: i + 1, ArrowUp: i - 1, Home: 0, End: items.length - 1 };
    if (e.key === "Escape") {
      e.preventDefault();
      closeMenu(true);
    } else if (e.key in move && items.length) {
      e.preventDefault();
      items[(move[e.key] + items.length) % items.length].focus();
    } else if (e.key === "Tab") {
      closeMenu();
    }
  }

  async function confirmDelete() {
    if (!deleting) return;
    try {
      await api.hostDelete(deleting.id);
      await reload();
      toast(t("hosts.deleted"), "success");
    } catch (e) {
      handleError(e);
    }
    deleting = null;
  }

  function userAt(h: Host) {
    const user = h.username ?? data.identities.find((i) => i.id === h.identityId)?.username;
    return `${user ? user + "@" : ""}${h.hostname}${h.port !== 22 ? ":" + h.port : ""}`;
  }
</script>

<svelte:window
  onpointerdown={(e) => menu && !menuEl?.contains(e.target as Node) && !menuButton?.contains(e.target as Node) && closeMenu()}
  onblur={() => closeMenu()}
  onresize={() => closeMenu()}
/>

<PageHeader title={t("hosts.title")} subtitle={t("hosts.subtitle")}>
  {#snippet actions()}
    <div class="search">
      <Icon name="search" size={16} />
      <input class="input" placeholder={t("hosts.search")} bind:value={query} spellcheck="false" />
    </div>
    <button class="btn primary" onclick={() => (editing = null)}><Icon name="plus" size={16} /> {t("hosts.new")}</button>
  {/snippet}
</PageHeader>

<div class="page">
  {#if data.loaded && data.hosts.length === 0}
    <div class="empty">
      <div class="empty-icon"><Icon name="server" size={28} /></div>
      <h3>{t("hosts.empty.title")}</h3>
      <p>{t("hosts.empty.text")}</p>
      <button class="btn primary" onclick={() => (editing = null)}><Icon name="plus" size={16} /> {t("hosts.empty.add")}</button>
    </div>
  {:else if data.loaded && filtered.length === 0}
    <div class="empty"><p>{t("hosts.noResults", { query })}</p></div>
  {/if}

  {#each groups as [group, hosts] (group)}
    <section>
      <h2>
        <Icon name="folder" size={14} />
        {group || t("hosts.noGroup")}
        <em>{hosts.length}</em>
      </h2>
      <div class="grid">
        {#each hosts as h (h.id)}
          <div
            class="card"
            role="group"
            aria-label={h.label}
            class:menu-open={menu?.host.id === h.id}
            class:compact
            style:--host-color={h.color ?? "var(--accent)"}
            oncontextmenu={(e) => onContextMenu(e, h)}
          >
            <div class="body">
              <!-- Compact : l'adresse passe dans l'info-bulle, les badges sous le nom. -->
              <button
                class="main"
                onclick={() => connect(h)}
                title={(compact ? `${userAt(h)} · ` : "") + t(h.kind === "sftp" ? "hosts.openSftpTitle" : "hosts.openSshTitle")}
              >
                <div class="avatar"><Icon name={h.kind === "sftp" ? "folder" : "terminal"} size={18} /></div>
                <div class="info">
                  <strong>{h.label}</strong>
                  {#if compact}
                    <div class="meta inline">{@render badges(h)}</div>
                  {:else}
                    <span class="mono" title={userAt(h)}>{userAt(h)}</span>
                  {/if}
                </div>
              </button>
              {#if !compact}<div class="meta">{@render badges(h)}</div>{/if}
            </div>
            <button
              class="btn ghost icon menu-btn"
              onclick={(e) => onMenuButton(e, h)}
              aria-label={t("hosts.actionsFor", { name: h.label })}
              aria-haspopup="menu"
              aria-expanded={menu?.host.id === h.id}
              title={t("hosts.actions")}
            >
              <Icon name="menu" size={16} />
            </button>
          </div>
        {/each}
      </div>
    </section>
  {/each}
</div>

{#snippet badges(h: Host)}
  <span class="badge kind">{h.kind === "sftp" ? "SFTP" : "SSH"}</span>
  {#if identityName(h.identityId)}
    <span class="badge" title={identityName(h.identityId)}>
      <Icon name="key" size={11} /><span class="text">{identityName(h.identityId)}</span>
    </span>
  {/if}
  {#each h.tags.slice(0, 3) as tag}<span class="badge" title={tag}><span class="text">{tag}</span></span>{/each}
{/snippet}

{#if menu}
  {@const h = menu.host}
  <!-- svelte-ignore a11y_interactive_supports_focus -->
  <div
    class="context-menu"
    bind:this={menuEl}
    style:left="{menu.x}px"
    style:top="{menu.y}px"
    role="menu"
    aria-label={t("hosts.actionsFor", { name: h.label })}
    onkeydown={onMenuKeydown}
  >
    <div class="menu-head">{h.label} · {h.kind === "sftp" ? "SFTP" : "SSH"}</div>
    <button role="menuitem" onclick={() => act(connect)}>
      {#if h.kind === "sftp"}<Icon name="folder" size={15} /> {t("hosts.menu.openBrowser")}{:else}<Icon name="terminal" size={15} /> {t("hosts.menu.openTerminal")}{/if}
    </button>
    <button role="menuitem" onclick={() => act((x) => openSession(x, other(x)))}>
      {#if other(h) === "sftp"}<Icon name="folder" size={15} /> {t("menu.openSftp")}{:else}<Icon name="terminal" size={15} /> {t("menu.openSsh")}{/if}
    </button>
    <div class="sep"></div>
    <button role="menuitem" onclick={() => act((x) => (editing = x))}><Icon name="edit" size={15} /> {t("hosts.menu.edit")}</button>
    <button role="menuitem" onclick={() => act((x) => (duplicating = x))}><Icon name="copy" size={15} /> {t("hosts.menu.duplicate")}</button>
    <div class="sep"></div>
    <button role="menuitem" class="danger" onclick={() => act((x) => (deleting = x))}><Icon name="trash" size={15} /> {t("common.delete")}</button>
  </div>
{/if}

{#if editing !== undefined}
  <HostForm host={editing} onclose={() => (editing = undefined)} />
{/if}

{#if duplicating}
  <HostForm host={duplicating} copy onclose={() => (duplicating = null)} />
{/if}

{#if deleting}
  <ConfirmDialog
    title={t("hosts.deleteTitle")}
    message={t("hosts.deleteMessage", { name: deleting.label })}
    onconfirm={confirmDelete}
    onclose={() => (deleting = null)}
  />
{/if}

<style>
  /* Badge du type de connexion : aux couleurs de l'hôte, comme sa bordure et son icône. */
  .badge.kind {
    color: var(--host-color);
    background: color-mix(in srgb, var(--host-color) 16%, transparent);
    border-color: transparent;
  }
  .page {
    padding: 4px 32px 32px;
    display: flex;
    flex-direction: column;
    gap: 26px;
  }
  .search {
    position: relative;
    width: 240px;
  }
  .search :global(svg) {
    position: absolute;
    left: 11px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--text-faint);
  }
  .search .input {
    padding-left: 34px;
    height: 36px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
    font-size: 12px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-faint);
  }
  h2 em {
    font-style: normal;
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 12px;
  }
  .card {
    position: relative;
    display: flex;
    align-items: center;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    overflow: hidden;
    transition: border-color 0.15s, box-shadow 0.15s;
  }
  .card::before {
    content: "";
    position: absolute;
    inset: 0 auto 0 0;
    width: 3px;
    background: var(--host-color);
  }
  .card:hover {
    border-color: var(--border-strong);
    box-shadow: var(--shadow);
  }
  .body {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-width: 0;
  }
  .main {
    display: flex;
    align-items: center;
    gap: 12px;
    /* Place pour le bouton ☰ en haut à droite. */
    padding: 14px 44px 10px 16px;
    border: 0;
    background: transparent;
    text-align: left;
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 38px;
    height: 38px;
    flex-shrink: 0;
    border-radius: 10px;
    color: var(--host-color);
    background: color-mix(in srgb, var(--host-color) 16%, transparent);
  }
  .info {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .info strong {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .info span {
    color: var(--text-muted);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px 10px 16px;
    min-height: 40px;
    min-width: 0;
  }
  /* Badges sur une seule ligne : un nom trop long est tronqué (nom complet en info-bulle). */
  .meta .badge {
    flex-shrink: 1;
    min-width: 0;
    white-space: nowrap;
  }
  .meta .badge.kind {
    flex-shrink: 0;
  }
  .meta .badge :global(svg) {
    flex-shrink: 0;
  }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Compact : badges sur la ligne de l'adresse, tuile d'environ 73 px au lieu de 104. */
  .compact .main {
    flex: 1;
    padding: 12px 44px 12px 16px;
  }
  .compact .info {
    gap: 5px;
  }
  .meta.inline {
    padding: 0;
    min-height: 0;
    overflow: hidden;
  }
  .menu-btn {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 30px;
    height: 30px;
    color: var(--text-faint);
  }
  .menu-btn:hover,
  .menu-open .menu-btn {
    color: var(--text);
    background: var(--surface-hover);
  }
  .menu-open {
    border-color: var(--border-strong);
  }
  .empty-icon {
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    border-radius: 18px;
    background: var(--accent-soft);
    color: var(--accent);
    margin-bottom: 6px;
  }
</style>
