<script lang="ts">
  import { onMount } from "svelte";
  import { api, type AppInfo, type KnownHost, type Settings } from "../api";
  import Icon from "../components/Icon.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import PasswordInput from "../components/PasswordInput.svelte";
  import StrengthMeter from "../components/StrengthMeter.svelte";
  import { toast } from "../components/toast.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import OidcSettings from "../components/OidcSettings.svelte";
  import SecurityKeySettings from "../components/SecurityKeySettings.svelte";
  import BackupSettings from "../components/BackupSettings.svelte";
  import { data, handleError } from "../store.svelte";
  import { TERMINAL_THEMES } from "../terminalThemes";
  import { setThemePreference, themePreference, type ThemePreference } from "../appearance";
  import { providerName } from "../oidcProviders";
  import { IS_MAC } from "../platform";
  import { checkForUpdate, update } from "../updates.svelte";
  import { display, setTileDensity, type TileDensity } from "../display.svelte";
  import { APP_NAME } from "../identity";
  import UpdateActions from "../components/UpdateActions.svelte";
  import { formatList, intlLocale, languagePreference, setLanguagePreference, t, tn, type LanguagePreference, type MessageKey } from "../i18n/index.svelte";

  // Copie locale éditable ; la valeur enregistrée est publiée dans le store
  // pour que les terminaux ouverts l'appliquent immédiatement.
  let settings = $state<Settings | null>(null);
  let knownHosts = $state<KnownHost[]>([]);
  let about = $state<AppInfo | null>(null);
  let forgetting = $state<KnownHost | null>(null);

  let current = $state("");
  let next = $state("");
  let confirm = $state("");
  let changing = $state(false);

  let appearance = $state<ThemePreference>(themePreference());

  /** Largeur du contenu des réglages (px), centré dans la fenêtre. */
  const CONTENT_WIDTH = 760;

  // Onglets par thème ; le dernier ouvert est retenu (stockage local de la fenêtre).
  type Tab = "general" | "security" | "terminal" | "data" | "about";
  const TABS: [Tab, MessageKey][] = [
    ["general", "settings.tab.general"],
    ["security", "settings.tab.security"],
    ["terminal", "settings.tab.terminal"],
    ["data", "settings.tab.data"],
    ["about", "settings.tab.about"],
  ];
  const TAB_KEY = "quayline.settings.tab";
  function readTab(): Tab {
    try {
      const v = localStorage.getItem(TAB_KEY);
      if (TABS.some(([id]) => id === v)) return v as Tab;
    } catch {
      // Stockage indisponible : premier onglet.
    }
    return "general";
  }
  let tab = $state<Tab>(readTab());
  function selectTab(id: Tab) {
    tab = id;
    try {
      localStorage.setItem(TAB_KEY, id);
    } catch {
      // Non mémorisé.
    }
  }
  let languageChoice = $state<LanguagePreference>(languagePreference());

  // Noms propres (Dracula, Nord…) gardés tels quels ; « Nuit » et « Jour » traduits.
  const themeLabel = (name: string) => (name.startsWith("theme.") ? t(name as MessageKey) : name);
  const THEMES = Object.entries(TERMINAL_THEMES).map(([id, theme]) => [id, theme.name]);

  const releaseDate = (iso: string) => new Date(`${iso}T12:00:00Z`).toLocaleDateString(intlLocale(), { dateStyle: "long" });

  onMount(async () => {
    api.appInfo().then((i) => (about = i)).catch(() => {});
    try {
      [settings, knownHosts] = await Promise.all([api.settingsGet(), api.knownHostsList()]);
    } catch (e) {
      handleError(e);
    }
  });

  async function refreshAfterImport() {
    try {
      [settings, knownHosts] = await Promise.all([api.settingsGet(), api.knownHostsList()]);
    } catch (e) {
      handleError(e);
    }
  }

  async function forget() {
    if (!forgetting) return;
    try {
      await api.knownHostDelete(forgetting.hostname, forgetting.port);
      knownHosts = await api.knownHostsList();
      toast(t("settings.hostKeyForgotten"), "success");
    } catch (e) {
      handleError(e);
    }
    forgetting = null;
  }

  async function save() {
    if (!settings) return;
    try {
      settings = await api.settingsSave({
        ...settings,
        auto_lock_minutes: Number(settings.auto_lock_minutes),
        clipboard_clear_seconds: Number(settings.clipboard_clear_seconds),
        oidc_reauth_minutes: Number(settings.oidc_reauth_minutes),
        terminal_font_size: Number(settings.terminal_font_size),
      });
      data.settings = { ...settings };
      toast(t("settings.saved"), "success");
    } catch (e) {
      handleError(e);
    }
  }

  // Moyens de déverrouillage retirés par un changement de mot de passe (la clé de
  // données est renouvelée) : affichés avant de valider.
  let unlockMethods = $state<string[]>([]);
  // Incrémenté pour recharger les sections Clés de sécurité et fournisseur d'identité.
  let methodsVersion = $state(0);

  async function loadUnlockMethods() {
    const [keys, oidc] = await Promise.all([api.fido2List().catch(() => []), api.oidcStatus().catch(() => null)]);
    unlockMethods = [
      ...(oidc ? [providerName(oidc.provider)] : []),
      ...(keys.length ? [tn("settings.securityKeys", keys.length)] : []),
    ];
  }

  $effect(() => {
    void methodsVersion;
    if (current) loadUnlockMethods();
  });

  async function changePassword(e: SubmitEvent) {
    e.preventDefault();
    changing = true;
    try {
      await api.vaultChangePassword(current, next);
      current = next = confirm = "";
      methodsVersion += 1;
      toast(
        unlockMethods.length
          ? t("settings.passwordChangedReenable", { methods: formatList(unlockMethods) })
          : t("settings.passwordChanged"),
        "success",
        6000,
      );
    } catch (err) {
      handleError(err);
    } finally {
      changing = false;
    }
  }
</script>

<PageHeader title={t("settings.title")} width={CONTENT_WIDTH} />

<div class="page" style:--content-width="{CONTENT_WIDTH}px">
  <div class="tabs" role="tablist" aria-label={t("settings.title")}>
    {#each TABS as [id, label]}
      <button role="tab" class="tab" class:active={tab === id} aria-selected={tab === id} onclick={() => selectTab(id)}>{t(label)}</button>
    {/each}
  </div>

  {#if tab === "general"}
  <section class="card">
    <h2><Icon name="contrast" size={17} /> {t("settings.appearance.title")}</h2>
    <div class="setting">
      <div>
        <strong>{t("settings.appearance.language")}</strong>
        <p>{t("settings.appearance.languageHint")}</p>
      </div>
      <!-- Noms des langues dans leur propre langue, pour s'y retrouver quelle que soit
           la langue affichée. -->
      <select class="input" bind:value={languageChoice} onchange={() => setLanguagePreference(languageChoice)}>
        <option value="system">{t("settings.appearance.system")}</option>
        <option value="en">English</option>
        <option value="fr">Français</option>
      </select>
    </div>
    <div class="setting">
      <div>
        <strong>{t("settings.appearance.theme")}</strong>
        <p>{t("settings.appearance.themeHint")}</p>
      </div>
      <select class="input" bind:value={appearance} onchange={() => setThemePreference(appearance)}>
        <option value="system">{t("settings.appearance.system")}</option>
        <option value="light">{t("settings.appearance.light")}</option>
        <option value="dark">{t("settings.appearance.dark")}</option>
      </select>
    </div>
    <div class="setting">
      <div>
        <strong>{t("settings.appearance.tiles")}</strong>
        <p>{t("settings.appearance.tilesHint")}</p>
      </div>
      <select class="input" value={display.tiles} onchange={(e) => setTileDensity(e.currentTarget.value as TileDensity)}>
        <option value="standard">{t("settings.appearance.tilesStandard")}</option>
        <option value="compact">{t("settings.appearance.tilesCompact")}</option>
      </select>
    </div>
  </section>

  <section class="card">
    <h2><Icon name="refresh" size={17} /> {t("update.title")}</h2>
    {#if settings}
      <div class="setting">
        <div>
          <strong>{t("update.auto")}</strong>
          <p>{t("update.autoHint")}</p>
        </div>
        <select class="input" bind:value={settings.check_updates} onchange={save}>
          <option value={true}>{t("common.enabled")}</option>
          <option value={false}>{t("common.disabled")}</option>
        </select>
      </div>
    {/if}
    <div class="update-row">
      <button class="btn" onclick={checkForUpdate} disabled={update.checking || !!update.install}>
        <Icon name="refresh" size={15} /> {t(update.checking ? "update.checking" : "update.check")}
      </button>
      {#if update.available}
        <span>{update.available.published
            ? t("update.availableLong", { version: update.available.version, date: releaseDate(update.available.published) })
            : t("update.availableShort", { version: update.available.version })}</span>
        <UpdateActions />
      {/if}
      {#if update.error}
        <span class="error-text">{update.error}</span>
      {:else if !update.available && update.checkedAt}
        <span class="faint">{t("update.upToDate", { app: APP_NAME })}</span>
      {/if}
    </div>
  </section>
  {:else if tab === "security"}
    {#if settings}
    <section class="card">
      <h2><Icon name="shield" size={17} /> {t("settings.security.title")}</h2>
      <div class="setting">
        <div>
          <strong>{t("settings.autoLock")}</strong>
          <p>{t("settings.autoLockHint")}</p>
        </div>
        <select class="input" bind:value={settings.auto_lock_minutes} onchange={save}>
          {#each [1, 5, 10, 30] as m}<option value={m}>{tn("duration.minutes", m)}</option>{/each}
          <option value={60}>{t("duration.hour")}</option>
          <option value={0}>{t("duration.never")}</option>
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>{t("settings.systemLock")}</strong>
          <p>{t(IS_MAC ? "settings.systemLockHintMac" : "settings.systemLockHint")}</p>
        </div>
        <select class="input" bind:value={settings.lock_on_system_events} onchange={save}>
          <option value={true}>{t("common.enabled")}</option>
          <option value={false}>{t("common.disabled")}</option>
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>{t("settings.lockSessions")}</strong>
          <p>{t("settings.lockSessionsHint")}</p>
        </div>
        <select class="input" bind:value={settings.lock_closes_sessions} onchange={save}>
          <option value={true}>{t("settings.lockSessionsClose")}</option>
          <option value={false}>{t("settings.lockSessionsKeep")}</option>
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>{t("settings.clipboard")}</strong>
          <p>{t("settings.clipboardHint")}</p>
        </div>
        <select class="input" bind:value={settings.clipboard_clear_seconds} onchange={save}>
          <option value={15}>{t("duration.seconds", { count: 15 })}</option>
          <option value={30}>{t("duration.seconds", { count: 30 })}</option>
          <option value={60}>{tn("duration.minutes", 1)}</option>
          <option value={0}>{t("duration.never")}</option>
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>{t("settings.oidcReauth")}</strong>
          <p>{t("settings.oidcReauthHint")}</p>
        </div>
        <select class="input" bind:value={settings.oidc_reauth_minutes} onchange={save}>
          <option value={0}>{t("settings.oidcReauthAlways")}</option>
          {#each [5, 15, 30] as m}
            <option value={m}>{t("settings.oidcReauthAfter", { duration: tn("duration.minutes", m) })}</option>
          {/each}
          <option value={60}>{t("settings.oidcReauthAfter", { duration: t("duration.hour") })}</option>
        </select>
      </div>
    </section>
    {/if}

    <section class="card">
    <h2><Icon name="lock" size={17} /> {t("settings.master.title")}</h2>
    <form class="pw" onsubmit={changePassword}>
      <label class="field">
        <span>{t("settings.master.current")}</span>
        <PasswordInput bind:value={current} />
      </label>
      <label class="field">
        <span>{t("settings.master.new")}</span>
        <PasswordInput bind:value={next} placeholder={t("lock.minLength", { count: 12 })} />
      </label>
      <StrengthMeter password={next} />
      {#if unlockMethods.length}
        <p class="rotation-note">
          <Icon name="alert" size={14} />
          <span>{t("settings.master.rotation", { methods: formatList(unlockMethods) })}</span>
        </p>
      {/if}
      <label class="field">
        <span>{t("lock.confirmation")}</span>
        <PasswordInput bind:value={confirm} />
        {#if confirm && confirm !== next}<small class="error-text">{t("lock.mismatch")}</small>{/if}
      </label>
      <div>
        <button class="btn primary" type="submit" disabled={changing || !current || next.length < 12 || next !== confirm}>
          {t(changing ? "settings.master.changing" : "settings.master.change")}
        </button>
      </div>
    </form>
  </section>

    {#key methodsVersion}
    <SecurityKeySettings />
    <OidcSettings />
  {/key}
  {:else if tab === "terminal"}
    {#if settings}
    <section class="card">
      <h2><Icon name="terminal" size={17} /> {t("settings.terminal.title")}</h2>
      <div class="setting">
        <div>
          <strong>{t("settings.terminal.theme")}</strong>
          <p>{t("settings.terminal.themeHint")}</p>
        </div>
        <select class="input" bind:value={settings.terminal_theme} onchange={save}>
          {#each THEMES as [id, name]}<option value={id}>{themeLabel(name)}</option>{/each}
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>{t("settings.terminal.fontSize")}</strong>
          <p>{t("settings.terminal.fontSizeHint")}</p>
        </div>
        <input class="input" type="number" min="8" max="32" bind:value={settings.terminal_font_size} onchange={save} />
      </div>
    </section>
    {/if}
  {:else if tab === "data"}
  <section class="card">
    <h2><Icon name="server" size={17} /> {t("settings.knownHosts.title")}</h2>
    <p class="hint">{t("settings.knownHosts.hint")}</p>
    {#if knownHosts.length === 0}
      <p class="hint">{t("settings.knownHosts.empty")}</p>
    {/if}
    {#each knownHosts as k (k.hostname + ":" + k.port)}
      <div class="known">
        <div>
          <strong>{k.hostname}{k.port !== 22 ? `:${k.port}` : ""}</strong>
          <span class="mono">{k.algorithm} · {k.fingerprint}</span>
        </div>
        <button class="btn ghost icon" onclick={() => (forgetting = k)} title={t("settings.knownHosts.forget")}><Icon name="trash" size={16} /></button>
      </div>
    {/each}
  </section>

  <BackupSettings knownHostsCount={knownHosts.length} onimported={refreshAfterImport} />
  {:else}
  {#if about}
    <section class="card about">
      <h2><Icon name="sparkles" size={17} /> {t("settings.about.title")}</h2>
      <p>
        <strong>{about.name} {about.version}</strong>
        <span class="badge">{about.build === "dev" ? t("settings.about.dev") : t("settings.about.build", { build: about.build })}</span>
        {#if about.flatpak}<span class="badge">Flatpak</span>{/if}
      </p>
      {#if about.commit}<p class="faint">{t("settings.about.commit")} <code>{about.commit}</code></p>{/if}
    </section>
  {/if}
  {/if}
</div>

{#if forgetting}
  <ConfirmDialog
    title={t("settings.forgetTitle")}
    message={t("settings.forgetMessage", { host: forgetting.hostname })}
    confirmLabel={t("settings.knownHosts.forget")}
    onconfirm={forget}
    onclose={() => (forgetting = null)}
  />
{/if}

<style>
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    border-bottom: 1px solid var(--border);
  }
  .tab {
    padding: 9px 14px;
    margin-bottom: -1px;
    border: 0;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--text-muted);
    font-size: 13.5px;
    white-space: nowrap;
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.active {
    color: var(--text);
    border-color: var(--accent);
    font-weight: 600;
  }
  .tab:focus-visible {
    outline: none;
    box-shadow: var(--ring);
    border-radius: 6px 6px 0 0;
  }
  .hint {
    color: var(--text-muted);
    font-size: 12.5px;
    margin-bottom: 8px;
  }
  .known {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }
  .known div {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .known span {
    font-size: 11.5px;
    color: var(--text-faint);
    word-break: break-all;
  }
  .known .btn {
    color: var(--text-faint);
    flex-shrink: 0;
  }
  .about > p {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 4px;
    font-size: 13px;
  }
  .about .faint {
    color: var(--text-faint);
    font-size: 12px;
  }
  .update-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 12px;
  }
  .about code {
    font-family: var(--font-mono);
    user-select: text;
  }
  /* Contenu de largeur limitée (lisibilité), centré : marges égales des deux côtés. */
  .page {
    padding: 4px max(32px, calc((100% - var(--content-width)) / 2)) 32px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .card {
    padding: 20px 22px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 15px;
    margin-bottom: 8px;
  }
  h2 :global(svg) {
    color: var(--accent);
  }
  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 12px 0;
    border-top: 1px solid var(--border);
  }
  .setting:first-of-type {
    border-top: 0;
  }
  .setting p {
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .setting .input {
    width: 180px;
    flex-shrink: 0;
  }
  .rotation-note {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 0;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .rotation-note :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
    color: var(--warning);
  }
  .pw {
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 420px;
    padding-top: 6px;
  }
</style>
