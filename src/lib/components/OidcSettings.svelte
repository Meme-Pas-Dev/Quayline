<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { onMount } from "svelte";
  import { api, errorCode, errorMessage, type OidcProviderId, type OidcStatus } from "../api";
  import { APP_NAME } from "../identity";
  import { OIDC_PROVIDERS, oidcProvider, providerName } from "../oidcProviders";
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
    toast(t("oidc.redirectCopied"), "success");
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
      toast(t("oidc.enabledFor", { provider: providerName(providerId), account: identity.name ?? identity.subject }), "success", 5000);
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
      toast(t("oidc.disabled"), "success");
      await load();
    } catch (e) {
      handleError(e);
    }
  }
</script>

<section class="card">
  <h2><Icon name="user" size={17} /> {t("oidc.title")}</h2>

  {#if !loaded}
    <p class="hint">{t("files.loading")}</p>
  {:else if status && !editing}
    <p class="hint">
      {status.provider === "generic"
        ? t("oidc.enabledHintGeneric")
        : t("oidc.enabledHint", { provider: providerName(status.provider) })}
    </p>
    <dl>
      <dt>{t("oidc.account")}</dt>
      <dd>{status.account}</dd>
      <dt>{t("oidc.provider")}</dt>
      <dd>
        {providerName(status.provider)}
        {#if oidcProvider(status.provider).beta}<span class="beta">{t("oidc.beta")}</span>{/if}
      </dd>
      <dt>{t("oidc.instance")}</dt>
      <dd class="mono">{status.issuer}</dd>
      <dt>Client ID</dt>
      <dd class="mono">{status.clientId}</dd>
      <dt>{t("oidc.redirectAddress")}</dt>
      <dd class="mono">{status.redirectUri}</dd>
      {#if status.requiredGroup}
        <dt>{t("oidc.requiredGroup")}</dt>
        <dd>{status.requiredGroup}</dd>
      {/if}
      <dt>{t("oidc.scopes")}</dt>
      <dd class="mono">{status.scopes}</dd>
      {#if status.hasClientSecret}
        <dt>{t("oidc.clientSecret")}</dt>
        <dd>{t("oidc.saved")}</dd>
      {/if}
    </dl>
    <div class="actions">
      <button class="btn" onclick={startEdit}>{t("oidc.reconfigure")}</button>
      <button class="btn danger" onclick={() => (confirmDisable = true)}>{t("oidc.disable")}</button>
    </div>
  {:else if !editing}
    <p class="hint">
      {t("oidc.intro.before", { app: APP_NAME })}<strong>Pocket ID</strong>, <strong>Authentik</strong>,
      <strong>Authelia</strong>, <strong>Keycloak</strong>, <strong>Microsoft Entra ID</strong>{t("oidc.intro.or")}<strong
        >Google</strong
      >{t("oidc.intro.after")}
    </p>
    <div class="actions">
      <button class="btn primary" onclick={startEdit}>{t("oidc.configure")}</button>
    </div>
  {:else}
    <div class="providers" role="radiogroup" aria-label={t("oidc.providerGroup")}>
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
          {providerName(p.id)}
          {#if p.beta}<span class="beta">{t("oidc.beta")}</span>{/if}
        </button>
      {/each}
    </div>

    <ol class="steps">
      {#each provider.before as step, i (i)}<li>{t(step)}</li>{/each}
      <li>
        {t(provider.redirectLabel ?? "oidc.redirectStep")}
        <span class="uri">
          <code>{redirectUri}</code>
          <button class="btn ghost icon tiny" title={t("common.copy")} onclick={copyRedirect}><Icon name="copy" size={14} /></button>
        </span>
      </li>
      {#each provider.after as step, i (i)}<li>{t(step)}</li>{/each}
    </ol>
    <p class="note"><Icon name="phone" size={13} /> {t("oidc.qrNote", { note: t(provider.phone) })}</p>
    {#if provider.beta}
      <p class="note beta-note">
        <Icon name="alert" size={13} /> {t("oidc.betaNote")}
      </p>
    {/if}

    <form id="oidc-form" class="grid" onsubmit={(e) => { e.preventDefault(); if (formValid) askPassword = true; }}>
      <label class="field span2">
        <span>{t("oidc.issuerLabel")}</span>
        <input class="input" bind:value={issuer} placeholder={t(provider.issuerPlaceholder)} spellcheck="false" />
        {#if provider.issuerHint}<small class="field-hint">{t(provider.issuerHint)}</small>{/if}
      </label>
      <label class="field">
        <span>{t("oidc.port")}</span>
        <input class="input" type="number" min="1024" max="65535" bind:value={port} />
      </label>
      <label class="field span2">
        <span>Client ID</span>
        <input class="input mono" bind:value={clientId} spellcheck="false" autocomplete="off" />
      </label>
      {#if provider.groupPlaceholder}
        <label class="field">
          <span>{t("oidc.requiredGroup")}</span>
          <input class="input" bind:value={group} placeholder={t(provider.groupPlaceholder)} spellcheck="false" />
          {#if provider.groupHint}<small class="field-hint">{t(provider.groupHint)}</small>{/if}
        </label>
      {/if}
      {#if provider.secretRequired}
        <label class="field span3">
          <span>{t("oidc.clientSecret")}</span>
          <PasswordInput bind:value={clientSecret} placeholder={status?.hasClientSecret ? t("oidc.secretRetype") : ""} />
        </label>
      {/if}

      <details class="span3" bind:open={advanced}>
        <summary>{t("oidc.advanced")}</summary>
        <div class="grid inner">
          <label class="field span2">
            <span>{t("oidc.scopes")}</span>
            <input class="input mono" bind:value={scopes} spellcheck="false" />
            <small class="field-hint">{t("oidc.scopesHint")}</small>
          </label>
          <label class="field">
            <span>{t("oidc.groupsClaim")}</span>
            <input class="input mono" bind:value={groupsClaim} spellcheck="false" />
            <small class="field-hint">{t("oidc.groupsClaimHint")}</small>
          </label>
          {#if !provider.secretRequired}
            <label class="field span3">
              <span>{t("oidc.clientSecret")}</span>
              <PasswordInput bind:value={clientSecret} placeholder={t("oidc.secretOptional")} />
            </label>
          {/if}
        </div>
      </details>
    </form>

    {#if waiting}
      <div class="wait">
        <span class="spinner"></span>
        <div>
          <strong>{t("lock.browserWaiting")}</strong>
          <span>{t("oidc.signInWith", { provider: provider.id === "generic" ? t("oidc.yourProvider") : providerName(provider.id) })}</span>
        </div>
        <button class="btn ghost" onclick={() => api.oidcCancel()}>{t("common.cancel")}</button>
      </div>
    {/if}
    {#if error}<p class="error-text">{error}</p>{/if}

    <div class="actions">
      <button class="btn" onclick={() => (editing = false)} disabled={waiting}>{t("common.cancel")}</button>
      <button class="btn primary" type="submit" form="oidc-form" disabled={!formValid || waiting}>
        <Icon name="shield" size={16} /> {t("oidc.testEnable")}
      </button>
    </div>
  {/if}
</section>

{#if askPassword}
  <Modal title={t("oidc.confirmTitle")} onclose={() => (askPassword = false)} width={440}>
    <form id="oidc-password" class="pw" onsubmit={enroll}>
      <p class="hint">
        {t("oidc.confirmHint.before", {
          provider: provider.id === "generic" ? t("oidc.theProvider") : providerName(provider.id),
        })}<strong>{issuer.trim()}</strong>{t("oidc.confirmHint.after")}
      </p>
      <PasswordInput bind:value={password} autofocus />
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (askPassword = false)}>{t("common.cancel")}</button>
      <button class="btn primary" type="submit" form="oidc-password" disabled={!password}>{t("wizard.continue")}</button>
    {/snippet}
  </Modal>
{/if}

{#if confirmDisable}
  <ConfirmDialog
    title={t("oidc.disableTitle")}
    message={t("oidc.disableMessage", {
      provider: status && status.provider !== "generic" ? providerName(status.provider) : t("oidc.theProvider"),
    })}
    confirmLabel={t("oidc.disable")}
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
