<script lang="ts">
  import { api, type Identity, type SecretField } from "../api";
  import { handleError, reload } from "../store.svelte";
  import Modal from "./Modal.svelte";
  import PasswordInput from "./PasswordInput.svelte";
  import { toast } from "./toast.svelte";

  let { identity, onclose }: { identity: Identity | null; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  const existing = identity;
  let label = $state(existing?.label ?? "");
  let username = $state(existing?.username ?? "");

  // Les secrets existants ne sont jamais renvoyés par le backend : champ vide = inchangé.
  let password = $state("");
  let privateKey = $state("");
  let passphrase = $state("");
  let clearPassword = $state(false);
  let clearKey = $state(false);
  let busy = $state(false);

  function field(value: string, clear: boolean): SecretField {
    if (clear) return { action: "clear" };
    if (value) return { action: "set", value };
    return { action: "keep" };
  }

  async function loadKeyFile(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    if (file.size > 64 * 1024) {
      toast("Fichier trop volumineux pour une clé privée.", "error");
      return;
    }
    privateKey = await file.text();
    clearKey = false;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    try {
      await api.identitySave({
        id: existing?.id ?? null,
        label,
        username: username || null,
        password: field(password, clearPassword),
        privateKey: field(privateKey, clearKey),
        keyPassphrase: field(passphrase, clearKey),
      });
      password = privateKey = passphrase = "";
      await reload();
      toast(existing ? "Identité mise à jour" : "Identité ajoutée", "success");
      onclose();
    } catch (err) {
      handleError(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={existing ? "Modifier l'identité" : "Nouvelle identité"} {onclose} width={560}>
  <form id="identity-form" class="stack" onsubmit={save}>
    <div class="row">
      <label class="field">
        <span>Nom</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input class="input" bind:value={label} placeholder="Compte admin" required autofocus />
      </label>
      <label class="field">
        <span>Utilisateur</span>
        <input class="input" bind:value={username} placeholder="root" spellcheck="false" />
      </label>
    </div>

    <div class="section">
      <div class="section-head">
        <h3>Mot de passe</h3>
        {#if existing?.hasPassword}
          <label class="clear"><input type="checkbox" bind:checked={clearPassword} /> Supprimer</label>
        {/if}
      </div>
      {#if !clearPassword}
        <PasswordInput bind:value={password} placeholder={existing?.hasPassword ? "•••••••• (inchangé)" : "Facultatif"} />
      {/if}
    </div>

    <div class="section">
      <div class="section-head">
        <h3>Clé privée</h3>
        {#if existing?.hasPrivateKey}
          <label class="clear"><input type="checkbox" bind:checked={clearKey} /> Supprimer</label>
        {/if}
      </div>
      {#if !clearKey}
        {#if existing?.key && !privateKey}
          <p class="current mono">{existing.key.algorithm} · {existing.key.fingerprint}</p>
        {/if}
        <textarea
          class="input mono"
          bind:value={privateKey}
          rows="5"
          spellcheck="false"
          placeholder={existing?.hasPrivateKey
            ? "Collez une nouvelle clé pour remplacer l'actuelle"
            : "-----BEGIN OPENSSH PRIVATE KEY-----"}
        ></textarea>
        <div class="row">
          <label class="btn file">
            Importer un fichier…
            <input type="file" onchange={loadKeyFile} hidden />
          </label>
          <div></div>
        </div>
        <label class="field">
          <span>Phrase de passe de la clé</span>
          <PasswordInput bind:value={passphrase} placeholder={existing?.hasPassphrase ? "•••••••• (inchangée)" : "Si la clé est chiffrée"} />
          <small>Facultatif : si elle est absente, elle sera demandée à chaque connexion.</small>
        </label>
      {/if}
    </div>
  </form>

  {#snippet footer()}
    <button class="btn" onclick={onclose}>Annuler</button>
    <button class="btn primary" type="submit" form="identity-form" disabled={busy || !label}>Enregistrer</button>
  {/snippet}
</Modal>

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: 18px;
  }
  .row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
  }
  .section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .section-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  h3 {
    font-size: 13.5px;
  }
  .clear {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--danger);
    cursor: pointer;
  }
  .current {
    font-size: 12px;
    color: var(--text-muted);
    word-break: break-all;
  }
  .file {
    justify-self: start;
  }
</style>
