<script lang="ts">
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
    toast("Clé publique copiée", "success");
  }

  async function copyPassword(identity: Identity) {
    try {
      const seconds = await api.identityCopyPassword(identity.id);
      toast(
        seconds > 0
          ? `Mot de passe copié : il sera effacé du presse-papiers dans ${seconds} s`
          : "Mot de passe copié (effacement automatique désactivé dans les réglages)",
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
      toast("Identité supprimée", "success");
    } catch (e) {
      handleError(e);
    }
    deleting = null;
  }
</script>

<PageHeader title="Trousseau" subtitle="Identifiants et clés SSH, chiffrés dans le coffre">
  {#snippet actions()}
    <button class="btn" onclick={() => (generating = true)}><Icon name="sparkles" size={16} /> Générer une clé</button>
    <button class="btn primary" onclick={() => (editing = null)}><Icon name="plus" size={16} /> Nouvelle identité</button>
  {/snippet}
</PageHeader>

<div class="page">
  {#if data.loaded && data.identities.length === 0}
    <div class="empty">
      <div class="empty-icon"><Icon name="key" size={28} /></div>
      <h3>Trousseau vide</h3>
      <p>Enregistrez un mot de passe ou une clé SSH, ou générez une nouvelle clé Ed25519.</p>
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
            {#if i.hasPassword}<span class="badge">Mot de passe</span>{/if}
            {#if i.key}
              <span class="badge accent">{i.key.algorithm}</span>
              {#if i.key.encrypted}<span class="badge">Clé chiffrée</span>{/if}
              <span class="fp mono">{i.key.fingerprint}</span>
            {/if}
            {#if usage(i.id)}<span class="badge">{usage(i.id)} hôte{usage(i.id) > 1 ? "s" : ""}</span>{/if}
          </div>
        </div>
        <div class="actions">
          {#if i.hasPassword}
            <button class="btn ghost icon" onclick={() => copyPassword(i)} title="Copier le mot de passe"><Icon name="clipboard" size={16} /></button>
          {/if}
          {#if i.key}
            <button class="btn ghost icon" onclick={() => copyPublicKey(i)} title="Copier la clé publique"><Icon name="copy" size={16} /></button>
          {/if}
          <button class="btn ghost icon" onclick={() => (editing = i)} title="Modifier"><Icon name="edit" size={16} /></button>
          <button class="btn ghost icon" onclick={() => (deleting = i)} title="Supprimer"><Icon name="trash" size={16} /></button>
        </div>
      </div>
    {/each}
  </div>
</div>

{#if editing !== undefined}
  <IdentityForm identity={editing} onclose={() => (editing = undefined)} />
{/if}

{#if generating}
  <Modal title={generated ? "Clé générée" : "Générer une clé Ed25519"} onclose={closeGenerate}>
    {#if generated?.key}
      <p class="hint">
        La clé privée est stockée dans le coffre. Ajoutez la clé publique au fichier
        <code>~/.ssh/authorized_keys</code> du serveur :
      </p>
      <textarea class="input mono" readonly rows="4" value={generated.key.publicKey}></textarea>
      <p class="fp mono">{generated.key.fingerprint}</p>
    {:else}
      <form id="gen-form" class="row" onsubmit={generate}>
        <label class="field">
          <span>Nom</span>
          <!-- svelte-ignore a11y_autofocus -->
          <input class="input" bind:value={genLabel} placeholder="Clé portable" required autofocus />
        </label>
        <label class="field">
          <span>Utilisateur</span>
          <input class="input" bind:value={genUser} placeholder="Facultatif" spellcheck="false" />
        </label>
      </form>
    {/if}
    {#snippet footer()}
      {#if generated}
        <button class="btn" onclick={() => generated && copyPublicKey(generated)}><Icon name="copy" size={16} /> Copier</button>
        <button class="btn primary" onclick={closeGenerate}>Terminé</button>
      {:else}
        <button class="btn" onclick={closeGenerate}>Annuler</button>
        <button class="btn primary" type="submit" form="gen-form" disabled={!genLabel}>Générer</button>
      {/if}
    {/snippet}
  </Modal>
{/if}

{#if deleting}
  <ConfirmDialog
    title="Supprimer l'identité ?"
    message={`« ${deleting.label} » sera définitivement effacée du coffre.` +
      (usage(deleting.id) ? ` ${usage(deleting.id)} hôte(s) n'auront plus d'identité associée.` : "")}
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
