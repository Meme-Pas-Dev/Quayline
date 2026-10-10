<script lang="ts">
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { onMount } from "svelte";
  import { api, errorCode, errorMessage, type OidcProviderId, type OidcStatus } from "../api";
  import { APP_NAME } from "../identity";
  import { OIDC_PROVIDERS, oidcProvider } from "../oidcProviders";
  import { handleError } from "../store.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import PasswordInput from "./PasswordInput.svelte";
  import { toast } from "./toast.svelte";

  const DEFAULT_PORT = 47615;

  let status = $state<OidcStatus | null>(null);
  let loaded = $state(false);
  let editing = $state(false);

  let providerId = $state<OidcProviderId>("pocket-id");
  let issuer = $state("");
  let clientId = $state("");
  let port = $state(DEFAULT_PORT);
  let group = $state("");
  let scopes = $state("");
  let groupsClaim = $state("");
  let clientSecret = $state("");
  let advanced = $state(false);

  const provider = $derived(oidcProvider(providerId));
  const secretMissing = $derived(!!provider.secretRequired && !clientSecret.trim());

  let askPassword = $state(false);
  let password = $state("");
  let waiting = $state(false);
  let error = $state("");
  let confirmDisable = $state(false);

  const redirectUri = $derived(`http://127.0.0.1:${Number(port) || DEFAULT_PORT}/callback`);
  const formValid = $derived(
    /^https?:\/\/[^\s<>]+$/.test(issuer.trim()) &&
      clientId.trim().length > 0 &&
      Number(port) >= 1024 &&
      /(^|\s)openid(\s|$)/.test(scopes) &&
      !secretMissing,
  );

  async function load() {
    try {
      status = await api.oidcStatus();
    } catch (e) {
      handleError(e);
    }
    loaded = true;
  }

  onMount(load);

  function startEdit() {
    providerId = status?.provider ?? "pocket-id";
    const p = oidcProvider(providerId);
    issuer = status?.issuer ?? p.issuer ?? "";
    clientId = status?.clientId ?? "";
    port = status?.redirectPort ?? DEFAULT_PORT;
    group = status?.requiredGroup ?? "";
    scopes = status?.scopes ?? p.scopes;
    groupsClaim = status?.groupsClaim ?? p.groupsClaim;
    // Le secret enregistré n'est jamais renvoyé à l'interface : à ressaisir.
    clientSecret = "";
    advanced = false;
    error = "";
    editing = true;
  }

  /** Changement de modèle : réglages par défaut du fournisseur choisi. */
  function selectProvider(id: OidcProviderId) {
    const previous = provider;
    providerId = id;
    const p = oidcProvider(id);
    if (!issuer.trim() || issuer === previous.issuer) issuer = p.issuer ?? "";
    scopes = p.scopes;
    groupsClaim = p.groupsClaim;
    if (!p.groupPlaceholder) group = "";
    error = "";
  }

  async function copyRedirect() {
    await writeText(redirectUri);
    toast("Adresse de retour copiée", "success");
  }

  async function enroll(e: SubmitEvent) {
    e.preventDefault();
    askPassword = false;
    waiting = true;
    error = "";
    try {
      const identity = await api.oidcEnroll(
        {
          provider: providerId,
          issuer: issuer.trim(),
          clientId: clientId.trim(),
          redirectPort: Number(port),
          requiredGroup: group.trim() || null,
          scopes: scopes.trim(),
          groupsClaim: groupsClaim.trim(),
          clientSecret: clientSecret.trim() || null,
        },
        password,
      );
      toast(`${provider.name} activé pour ${identity.name ?? identity.email ?? identity.subject}`, "success", 5000);
      editing = false;
      await load();
    } catch (err) {
      if (errorCode(err) === "locked") handleError(err);
      else if (errorCode(err) !== "cancelled") error = errorMessage(err);
    } finally {
      password = "";
      clientSecret = "";
      waiting = false;
    }
  }

  async function disable() {
    confirmDisable = false;
    try {
      await api.oidcDisable();
      toast("Fournisseur d'identité désactivé", "success");
      await load();
    } catch (e) {
      handleError(e);
    }
  }
</script>

<section class="card">
  <h2><Icon name="user" size={17} /> Fournisseur d'identité (OIDC)</h2>

  {#if !loaded}
    <p class="hint">Chargement…</p>
  {:else if status && !editing}
    <p class="hint">
      Le coffre peut être déverrouillé avec votre compte {status.providerName}. Le mot de passe maître reste valable.
    </p>
    <dl>
      <dt>Compte</dt>
      <dd>{status.account}</dd>
      <dt>Fournisseur</dt>
      <dd>
        {oidcProvider(status.provider).name}
        {#if oidcProvider(status.provider).beta}<span class="beta">Bêta</span>{/if}
      </dd>
      <dt>Instance</dt>
      <dd class="mono">{status.issuer}</dd>
      <dt>Client ID</dt>
      <dd class="mono">{status.clientId}</dd>
      <dt>Adresse de retour</dt>
      <dd class="mono">{status.redirectUri}</dd>
      {#if status.requiredGroup}
        <dt>Groupe exigé</dt>
        <dd>{status.requiredGroup}</dd>
      {/if}
      <dt>Scopes</dt>
      <dd class="mono">{status.scopes}</dd>
      {#if status.hasClientSecret}
        <dt>Secret client</dt>
        <dd>Enregistré</dd>
      {/if}
    </dl>
    <div class="actions">
      <button class="btn" onclick={startEdit}>Reconfigurer</button>
      <button class="btn danger" onclick={() => (confirmDisable = true)}>Désactiver</button>
    </div>
  {:else if !editing}
    <p class="hint">
      Déverrouillez {APP_NAME} avec votre compte <strong>Pocket ID</strong>, <strong>Authentik</strong>,
      <strong>Authelia</strong>, <strong>Keycloak</strong>, <strong>Microsoft Entra ID</strong>, <strong>Google</strong>
      ou tout autre fournisseur OpenID Connect, au lieu de saisir le mot de passe maître.
    </p>
    <div class="actions">
      <button class="btn primary" onclick={startEdit}>Configurer</button>
    </div>
  {:else}
    <div class="providers" role="radiogroup" aria-label="Fournisseur d'identité">
      {#each OIDC_PROVIDERS as p (p.id)}
        <button
          type="button"
          class="provider"
          class:active={p.id === providerId}
          role="radio"
          aria-checked={p.id === providerId}
          onclick={() => selectProvider(p.id)}
          disabled={waiting}
        >
          {p.name}
          {#if p.beta}<span class="beta">Bêta</span>{/if}
        </button>
      {/each}
    </div>

    <ol class="steps">
      {#each provider.before as step, i (i)}<li>{step}</li>{/each}
      <li>
        {provider.redirectLabel ?? "URL de rappel (redirect URI) :"}
        <span class="uri">
          <code>{redirectUri}</code>
          <button class="btn ghost icon tiny" title="Copier" onclick={copyRedirect}><Icon name="copy" size={14} /></button>
        </span>
      </li>
      {#each provider.after as step, i (i)}<li>{step}</li>{/each}
    </ol>
    <p class="note"><Icon name="phone" size={13} /> Connexion par QR code : {provider.phone}</p>
    {#if provider.beta}
      <p class="note beta-note">
        <Icon name="alert" size={13} /> Bêta : ce modèle n'a pas encore été essayé en conditions réelles. Le mot de passe
        maître reste valable ; signalez tout problème.
      </p>
    {/if}

    <form id="oidc-form" class="grid" onsubmit={(e) => { e.preventDefault(); if (formValid) askPassword = true; }}>
      <label class="field span2">
        <span>Adresse de l'instance (issuer)</span>
        <input class="input" bind:value={issuer} placeholder={provider.issuerPlaceholder} spellcheck="false" />
        {#if provider.issuerHint}<small class="field-hint">{provider.issuerHint}</small>{/if}
      </label>
      <label class="field">
        <span>Port de retour</span>
        <input class="input" type="number" min="1024" max="65535" bind:value={port} />
      </label>
      <label class="field span2">
        <span>Client ID</span>
        <input class="input mono" bind:value={clientId} spellcheck="false" autocomplete="off" />
      </label>
      {#if provider.groupPlaceholder}
        <label class="field">
          <span>Groupe exigé</span>
          <input class="input" bind:value={group} placeholder={provider.groupPlaceholder} spellcheck="false" />
          {#if provider.groupHint}<small class="field-hint">{provider.groupHint}</small>{/if}
        </label>
      {/if}
      {#if provider.secretRequired}
        <label class="field span3">
          <span>Secret client</span>
          <PasswordInput bind:value={clientSecret} placeholder={status?.hasClientSecret ? "À ressaisir" : ""} />
        </label>
      {/if}

      <details class="span3" bind:open={advanced}>
        <summary>Options avancées</summary>
        <div class="grid inner">
          <label class="field span2">
            <span>Scopes</span>
            <input class="input mono" bind:value={scopes} spellcheck="false" />
            <small class="field-hint">Séparés par des espaces ; « openid » est obligatoire.</small>
          </label>
          <label class="field">
            <span>Donnée des groupes</span>
            <input class="input mono" bind:value={groupsClaim} spellcheck="false" />
            <small class="field-hint">Nom ou chemin pointé (realm_access.roles).</small>
          </label>
          {#if !provider.secretRequired}
            <label class="field span3">
              <span>Secret client</span>
              <PasswordInput bind:value={clientSecret} placeholder="Facultatif : client public de préférence" />
            </label>
          {/if}
        </div>
      </details>
    </form>

    {#if waiting}
      <div class="wait">
        <span class="spinner"></span>
        <div>
          <strong>Connexion en cours dans votre navigateur…</strong>
          <span>Authentifiez-vous sur {provider.id === "generic" ? "votre fournisseur" : provider.name}, puis revenez ici.</span>
        </div>
        <button class="btn ghost" onclick={() => api.oidcCancel()}>Annuler</button>
      </div>
    {/if}
    {#if error}<p class="error-text">{error}</p>{/if}

    <div class="actions">
      <button class="btn" onclick={() => (editing = false)} disabled={waiting}>Annuler</button>
      <button class="btn primary" type="submit" form="oidc-form" disabled={!formValid || waiting}>
        <Icon name="shield" size={16} /> Tester et activer
      </button>
    </div>
  {/if}
</section>

{#if askPassword}
  <Modal title="Confirmer avec le mot de passe maître" onclose={() => (askPassword = false)} width={440}>
    <form id="oidc-password" class="pw" onsubmit={enroll}>
      <p class="hint">
        Pour activer {provider.id === "generic" ? "le fournisseur d'identité" : provider.name}, confirmez votre mot de
        passe maître. Votre navigateur s'ouvrira ensuite sur <strong>{issuer.trim()}</strong>.
      </p>
      <PasswordInput bind:value={password} autofocus />
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (askPassword = false)}>Annuler</button>
      <button class="btn primary" type="submit" form="oidc-password" disabled={!password}>Continuer</button>
    {/snippet}
  </Modal>
{/if}

{#if confirmDisable}
  <ConfirmDialog
    title="Désactiver le fournisseur d'identité ?"
    message={`Le déverrouillage par ${status?.providerName ?? "le fournisseur d'identité"} sera supprimé et sa clé retirée du trousseau système. Le mot de passe maître restera le seul moyen d'ouvrir le coffre.`}
    confirmLabel="Désactiver"
    onconfirm={disable}
    onclose={() => (confirmDisable = false)}
  />
{/if}

<style>
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
  .hint {
    color: var(--text-muted);
    font-size: 12.5px;
    margin-bottom: 12px;
  }
  dl {
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 6px 14px;
    margin: 0 0 14px;
    font-size: 13px;
  }
  dt {
    color: var(--text-muted);
  }
  dd {
    margin: 0;
    word-break: break-all;
    user-select: text;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .steps {
    margin: 0 0 16px;
    padding-left: 20px;
    color: var(--text-muted);
    font-size: 12.5px;
    line-height: 1.9;
  }
  .uri {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .uri code {
    padding: 1px 6px;
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
    user-select: text;
  }
  .tiny {
    width: 24px;
    height: 24px;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 14px;
    margin-bottom: 14px;
  }
  .span2 {
    grid-column: span 2;
  }
  .span3 {
    grid-column: 1 / -1;
  }
  .providers {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 14px;
  }
  .provider {
    padding: 6px 12px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: 12.5px;
    cursor: pointer;
  }
  .provider:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--border-strong);
  }
  .provider.active {
    color: var(--accent);
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .beta {
    margin-left: 6px;
    padding: 1px 6px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    /* Teinte d'avertissement assombrie vers le texte : lisible en clair comme en sombre. */
    color: color-mix(in srgb, var(--warning) 65%, var(--text));
    background: color-mix(in srgb, var(--warning) 18%, transparent);
  }
  .note {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: -8px 0 14px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .note.beta-note {
    color: color-mix(in srgb, var(--warning) 65%, var(--text));
  }
  .field-hint {
    color: var(--text-faint);
    font-size: 11.5px;
  }
  details summary {
    cursor: pointer;
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .grid.inner {
    margin: 12px 0 0;
  }
  .wait {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    margin-bottom: 12px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .wait div {
    flex: 1;
    display: flex;
    flex-direction: column;
    font-size: 12.5px;
  }
  .wait div span {
    color: var(--text-muted);
  }
  .spinner {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
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
  .error-text {
    margin-bottom: 12px;
  }
  .pw {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
</style>
