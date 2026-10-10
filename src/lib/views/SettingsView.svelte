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

  const THEMES = Object.entries(TERMINAL_THEMES).map(([id, t]) => [id, t.name]);

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
      toast("Clé d'hôte oubliée", "success");
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
        terminal_font_size: Number(settings.terminal_font_size),
      });
      data.settings = { ...settings };
      toast("Réglages enregistrés", "success");
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
      ...(oidc ? [oidc.providerName] : []),
      ...(keys.length ? [keys.length > 1 ? `${keys.length} clés de sécurité` : "1 clé de sécurité"] : []),
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
          ? `Mot de passe maître modifié. À réactiver : ${unlockMethods.join(" et ")}.`
          : "Mot de passe maître modifié",
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

<PageHeader title="Réglages" />

<div class="page">
  <section class="card">
    <h2><Icon name="contrast" size={17} /> Apparence</h2>
    <div class="setting">
      <div>
        <strong>Thème de l'interface</strong>
        <p>« Système » suit le réglage clair ou sombre de l'ordinateur.</p>
      </div>
      <select class="input" bind:value={appearance} onchange={() => setThemePreference(appearance)}>
        <option value="system">Système</option>
        <option value="light">Clair</option>
        <option value="dark">Sombre</option>
      </select>
    </div>
  </section>

  {#if settings}
    <section class="card">
      <h2><Icon name="shield" size={17} /> Sécurité</h2>
      <div class="setting">
        <div>
          <strong>Verrouillage automatique</strong>
          <p>Le coffre se verrouille après une période d'inactivité.</p>
        </div>
        <select class="input" bind:value={settings.auto_lock_minutes} onchange={save}>
          <option value={1}>1 minute</option>
          <option value={5}>5 minutes</option>
          <option value={10}>10 minutes</option>
          <option value={30}>30 minutes</option>
          <option value={60}>1 heure</option>
          <option value={0}>Jamais</option>
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>Verrouillage avec le système</strong>
          <p>Quand la session KDE ou GNOME est verrouillée, et avant la mise en veille.</p>
        </div>
        <select class="input" bind:value={settings.lock_on_system_events} onchange={save}>
          <option value={true}>Activé</option>
          <option value={false}>Désactivé</option>
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>Sessions au verrouillage</strong>
          <p>Fermer les connexions est plus sûr ; les garder évite de se reconnecter.</p>
        </div>
        <select class="input" bind:value={settings.lock_closes_sessions} onchange={save}>
          <option value={true}>Les fermer</option>
          <option value={false}>Les garder ouvertes</option>
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>Effacement du presse-papiers</strong>
          <p>Après la copie d'un mot de passe depuis le trousseau.</p>
        </div>
        <select class="input" bind:value={settings.clipboard_clear_seconds} onchange={save}>
          <option value={15}>15 secondes</option>
          <option value={30}>30 secondes</option>
          <option value={60}>1 minute</option>
          <option value={0}>Jamais</option>
        </select>
      </div>
    </section>

    <section class="card">
      <h2><Icon name="terminal" size={17} /> Terminal</h2>
      <div class="setting">
        <div>
          <strong>Thème</strong>
          <p>Palette de couleurs du terminal.</p>
        </div>
        <select class="input" bind:value={settings.terminal_theme} onchange={save}>
          {#each THEMES as [id, name]}<option value={id}>{name}</option>{/each}
        </select>
      </div>
      <div class="setting">
        <div>
          <strong>Taille de police</strong>
          <p>En points.</p>
        </div>
        <input class="input" type="number" min="8" max="32" bind:value={settings.terminal_font_size} onchange={save} />
      </div>
    </section>
  {/if}

  {#key methodsVersion}
    <SecurityKeySettings />
    <OidcSettings />
  {/key}

  <section class="card">
    <h2><Icon name="server" size={17} /> Clés d'hôte connues</h2>
    <p class="hint">
      Empreintes des serveurs approuvés. Oublier une clé fera redemander la confirmation à la prochaine connexion.
    </p>
    {#if knownHosts.length === 0}
      <p class="hint">Aucun serveur approuvé pour l'instant.</p>
    {/if}
    {#each knownHosts as k (k.hostname + ":" + k.port)}
      <div class="known">
        <div>
          <strong>{k.hostname}{k.port !== 22 ? `:${k.port}` : ""}</strong>
          <span class="mono">{k.algorithm} · {k.fingerprint}</span>
        </div>
        <button class="btn ghost icon" onclick={() => (forgetting = k)} title="Oublier"><Icon name="trash" size={16} /></button>
      </div>
    {/each}
  </section>

  <BackupSettings knownHostsCount={knownHosts.length} onimported={refreshAfterImport} />

  <section class="card">
    <h2><Icon name="lock" size={17} /> Mot de passe maître</h2>
    <form class="pw" onsubmit={changePassword}>
      <label class="field">
        <span>Mot de passe actuel</span>
        <PasswordInput bind:value={current} />
      </label>
      <label class="field">
        <span>Nouveau mot de passe</span>
        <PasswordInput bind:value={next} placeholder="12 caractères minimum" />
      </label>
      <StrengthMeter password={next} />
      {#if unlockMethods.length}
        <p class="rotation-note">
          <Icon name="alert" size={14} />
          <span>
            Le changement renouvelle la clé de chiffrement du coffre. À réactiver ensuite :
            {unlockMethods.join(", ")}.
          </span>
        </p>
      {/if}
      <label class="field">
        <span>Confirmation</span>
        <PasswordInput bind:value={confirm} />
        {#if confirm && confirm !== next}<small class="error-text">Les mots de passe ne correspondent pas.</small>{/if}
      </label>
      <div>
        <button class="btn primary" type="submit" disabled={changing || !current || next.length < 12 || next !== confirm}>
          {changing ? "Modification…" : "Changer le mot de passe"}
        </button>
      </div>
    </form>
  </section>

  {#if about}
    <section class="card about">
      <h2><Icon name="sparkles" size={17} /> À propos</h2>
      <p>
        <strong>{about.name} {about.version}</strong>
        <span class="badge">{about.build === "dev" ? "développement" : `build ${about.build}`}</span>
        {#if about.flatpak}<span class="badge">Flatpak</span>{/if}
      </p>
      {#if about.commit}<p class="faint">Commit <code>{about.commit}</code></p>{/if}
    </section>
  {/if}
</div>

{#if forgetting}
  <ConfirmDialog
    title="Oublier cette clé ?"
    message="La clé de « {forgetting.hostname} » sera supprimée. Vous devrez vérifier et approuver à nouveau l'empreinte à la prochaine connexion."
    confirmLabel="Oublier"
    onconfirm={forget}
    onclose={() => (forgetting = null)}
  />
{/if}

<style>
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
  .about p {
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
  .about code {
    font-family: var(--font-mono);
    user-select: text;
  }
  .page {
    padding: 4px 32px 32px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 760px;
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
