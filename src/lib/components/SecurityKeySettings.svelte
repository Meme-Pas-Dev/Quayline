<script lang="ts">
  import { onMount } from "svelte";
  import { api, type Fido2Key } from "../api";
  import { APP_NAME } from "../identity";
  import { handleError } from "../store.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Icon from "./Icon.svelte";
  import SecurityKeyWizard from "./SecurityKeyWizard.svelte";
  import { toast } from "./toast.svelte";

  let keys = $state<Fido2Key[]>([]);
  let loaded = $state(false);

  let adding = $state(false);
  let removing = $state<Fido2Key | null>(null);

  async function load() {
    try {
      keys = await api.fido2List();
    } catch (e) {
      handleError(e);
    }
    loaded = true;
  }

  onMount(load);

  async function added(key: Fido2Key) {
    toast(`Clé « ${key.label} » enregistrée`, "success");
    await load();
  }

  async function remove() {
    const key = removing;
    removing = null;
    if (!key) return;
    try {
      await api.fido2Remove(key.id);
      toast(`Clé « ${key.label} » retirée`, "success");
      await load();
    } catch (e) {
      handleError(e);
    }
  }
</script>

<section class="card">
  <h2><Icon name="key" size={17} /> Clés de sécurité</h2>
  <p class="hint">
    Déverrouillez {APP_NAME} avec une clé FIDO2 (YubiKey, Nitrokey, SoloKey…) et son code PIN. Le secret est calculé
    par la clé elle-même : rien n'est stocké dans le trousseau du système. Enregistrez une seconde clé en secours ; le
    mot de passe maître reste valable.
  </p>

  {#if !loaded}
    <p class="hint">Chargement…</p>
  {:else if keys.length}
    <ul>
      {#each keys as key (key.id)}
        <li>
          <Icon name="key" size={15} />
          <span>{key.label}</span>
          <button class="btn ghost icon" title="Retirer" onclick={() => (removing = key)}>
            <Icon name="trash" size={15} />
          </button>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="actions">
    <button class="btn" class:primary={!keys.length} onclick={() => (adding = true)} disabled={!loaded}>
      <Icon name="plus" size={16} /> Ajouter une clé
    </button>
  </div>
</section>

{#if adding}
  <SecurityKeyWizard existing={keys.length} onclose={() => (adding = false)} onadded={added} />
{/if}

{#if removing}
  <ConfirmDialog
    title="Retirer cette clé ?"
    message={`La clé « ${removing.label} » ne pourra plus déverrouiller le coffre.`}
    confirmLabel="Retirer"
    onconfirm={remove}
    onclose={() => (removing = null)}
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
  ul {
    list-style: none;
    margin: 0 0 14px;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 12px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 13px;
  }
  li span {
    flex: 1;
  }
  li :global(svg) {
    color: var(--text-muted);
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
</style>
