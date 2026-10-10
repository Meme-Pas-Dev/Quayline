<script lang="ts">
  import { t, tn } from "../i18n/index.svelte";
  import { api, type Identity } from "../api";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Icon from "../components/Icon.svelte";
  import IdentityForm from "../components/IdentityForm.svelte";
  import Modal from "../components/Modal.svelte";
  import PageHeader from "../components/PageHeader.svelte";
  import { toast } from "../components/toast.svelte";
  import { data, handleError, reload } from "../store.svelte";

  let editing = $state<Identity | null | undefined>(undefined);
  let deleting = $state<Identity | null>(null);
  let generating = $state(false);
  let genLabel = $state("");
  let genUser = $state("");
  let generated = $state<Identity | null>(null);

  const usage = (id: string) => data.hosts.filter((h) => h.identityId === id).length;

  async function copyPublicKey(identity: Identity) {
    if (!identity.key) return;
    await navigator.clipboard.writeText(identity.key.publicKey);
    toast(t("keychain.publicKeyCopied"), "success");
  }

  async function copyPassword(identity: Identity) {
    try {
      const seconds = await api.identityCopyPassword(identity.id);
      toast(
        seconds > 0
          ? t("keychain.passwordCopiedClear", { seconds })
          : t("keychain.passwordCopied"),
        "success",
        4000,
      );
    } catch (e) {
      handleError(e);
    }
  }

  async function generate(e: SubmitEvent) {
    e.preventDefault();
    try {
      generated = await api.identityGenerateKey(genLabel, genUser || null);
      await reload();
    } catch (err) {
      handleError(err);
    }
  }

  function closeGenerate() {
    generating = false;
    generated = null;
    genLabel = genUser = "";
  }

  async function confirmDelete() {
    if (!deleting) return;
    try {
      await api.identityDelete(deleting.id);
      await reload();
      toast(t("keychain.deleted"), "success");
    } catch (e) {
      handleError(e);
    }
    deleting = null;
  }
</script>

<PageHeader title={t("keychain.title")} subtitle={t("keychain.subtitle")}>
  {#snippet actions()}
    <button class="btn" onclick={() => (generating = true)}><Icon name="sparkles" size={16} /> {t("keychain.generate")}</button>
    <button class="btn primary" onclick={() => (editing = null)}><Icon name="plus" size={16} /> {t("keychain.new")}</button>
  {/snippet}
</PageHeader>

<div class="page">
  {#if data.loaded && data.identities.length === 0}
    <div class="empty">
      <div class="empty-icon"><Icon name="key" size={28} /></div>
      <h3>{t("keychain.empty.title")}</h3>
      <p>{t("keychain.empty.text")}</p>
    </div>
  {/if}

  <div class="list">
    {#each data.identities as i (i.id)}
      <div class="item">
        <div class="avatar" class:key={i.hasPrivateKey}>
          <Icon name={i.hasPrivateKey ? "key" : "password"} size={18} />
        </div>
        <div class="info">
          <div class="title">
            <strong>{i.label}</strong>
            {#if i.username}<span class="muted"><Icon name="user" size={12} /> {i.username}</span>{/if}
          </div>
          <div class="badges">
            {#if i.hasPassword}<span class="badge">{t("keychain.badge.password")}</span>{/if}
            {#if i.key}
              <span class="badge accent">{i.key.algorithm}</span>
              {#if i.key.encrypted}<span class="badge">{t("keychain.badge.encrypted")}</span>{/if}
              <span class="fp mono">{i.key.fingerprint}</span>
            {/if}
            {#if usage(i.id)}<span class="badge">{tn("keychain.hostCount", usage(i.id))}</span>{/if}
          </div>
        </div>
        <div class="actions">
          {#if i.hasPassword}
            <button class="btn ghost icon" onclick={() => copyPassword(i)} title={t("keychain.copyPassword")}><Icon name="clipboard" size={16} /></button>
          {/if}
          {#if i.key}
            <button class="btn ghost icon" onclick={() => copyPublicKey(i)} title={t("keychain.copyPublicKey")}><Icon name="copy" size={16} /></button>
          {/if}
          <button class="btn ghost icon" onclick={() => (editing = i)} title={t("common.edit")}><Icon name="edit" size={16} /></button>
          <button class="btn ghost icon" onclick={() => (deleting = i)} title={t("common.delete")}><Icon name="trash" size={16} /></button>
        </div>
      </div>
    {/each}
  </div>
</div>

{#if editing !== undefined}
  <IdentityForm identity={editing} onclose={() => (editing = undefined)} />
{/if}

{#if generating}
  <Modal title={t(generated ? "keychain.generated" : "keychain.generateTitle")} onclose={closeGenerate}>
    {#if generated?.key}
      <p class="hint">
        {t("keychain.generatedHint.before")}<code>~/.ssh/authorized_keys</code>{t("keychain.generatedHint.after")}
      </p>
      <textarea class="input mono" readonly rows="4" value={generated.key.publicKey}></textarea>
      <p class="fp mono">{generated.key.fingerprint}</p>
    {:else}
      <form id="gen-form" class="row" onsubmit={generate}>
        <label class="field">
          <span>{t("keychain.name")}</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="input" bind:value={genLabel} placeholder={t("keychain.namePlaceholder")} required autofocus />
        </label>
        <label class="field">
          <span>{t("keychain.user")}</span>
          <input class="input" bind:value={genUser} placeholder={t("common.optional")} spellcheck="false" />
        </label>
      </form>
    {/if}
    {#snippet footer()}
      {#if generated}
        <button class="btn" onclick={() => generated && copyPublicKey(generated)}><Icon name="copy" size={16} /> {t("common.copy")}</button>
        <button class="btn primary" onclick={closeGenerate}>{t("common.done")}</button>
      {:else}
        <button class="btn" onclick={closeGenerate}>{t("common.cancel")}</button>
        <button class="btn primary" type="submit" form="gen-form" disabled={!genLabel}>{t("keychain.generateAction")}</button>
      {/if}
    {/snippet}
  </Modal>
{/if}

{#if deleting}
  <ConfirmDialog
    title={t("keychain.deleteTitle")}
    message={t("keychain.deleteMessage", { name: deleting.label }) +
      (usage(deleting.id) ? ` ${tn("keychain.deleteUsage", usage(deleting.id))}` : "")}
    onconfirm={confirmDelete}
    onclose={() => (deleting = null)}
  />
{/if}

<style>
  .page {
    padding: 4px 32px 32px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .item {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 14px 14px 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    transition: border-color 0.15s;
  }
  .item:hover {
    border-color: var(--border-strong);
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    flex-shrink: 0;
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text-muted);
  }
  .avatar.key {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .muted {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .hint {
    color: var(--text-muted);
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .fp {
    font-size: 11.5px;
    color: var(--text-faint);
    word-break: break-all;
  }
  .actions {
    display: flex;
    gap: 2px;
  }
  .actions .btn {
    color: var(--text-faint);
  }
  .actions .btn:hover {
    color: var(--text);
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
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
