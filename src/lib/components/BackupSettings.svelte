<script lang="ts">
  import {
    api,
    errorMessage,
    type Duplicates,
    type ExportSelection,
    type ImportPreview,
  } from "../api";
  import { data, handleError, reload } from "../store.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import PasswordInput from "./PasswordInput.svelte";
  import StrengthMeter from "./StrengthMeter.svelte";
  import { toast } from "./toast.svelte";

  let { knownHostsCount, onimported }: { knownHostsCount: number; onimported: () => void } = $props();

  // ── Export ──
  let exporting = $state(false);
  let selection = $state<ExportSelection>({ hosts: true, identities: true, knownHosts: true, settings: true });
  let passphrase = $state("");
  let passphraseConfirm = $state("");
  let master = $state("");
  let exportBusy = $state(false);
  let exportError = $state("");

  const nothingSelected = $derived(!Object.values(selection).some(Boolean));
  const exportValid = $derived(
    !nothingSelected && passphrase.length >= 12 && passphrase === passphraseConfirm && master.length > 0,
  );

  function openExport() {
    passphrase = passphraseConfirm = master = exportError = "";
    exporting = true;
  }

  async function runExport(e: SubmitEvent) {
    e.preventDefault();
    if (!exportValid || exportBusy) return;
    exportBusy = true;
    exportError = "";
    try {
      const file = await api.vaultExport(master, passphrase, { ...selection });
      if (file) {
        exporting = false;
        toast(`Export enregistré : ${file}`, "success", 6000);
      }
    } catch (err) {
      exportError = errorMessage(err);
    } finally {
      exportBusy = false;
      master = "";
    }
  }

  // ── Import ──
  let importFile = $state<string | null>(null);
  let importPassphrase = $state("");
  let preview = $state<ImportPreview | null>(null);
  let duplicates = $state<Duplicates>("skip");
  let importSettings = $state(false);
  let importBusy = $state(false);
  let importError = $state("");

  const hasDuplicates = $derived(
    !!preview && preview.hosts.duplicates + preview.identities.duplicates + preview.knownHosts.duplicates > 0,
  );

  async function pickImport() {
    try {
      const file = await api.vaultImportPick();
      if (!file) return;
      importFile = file;
      importPassphrase = importError = "";
      preview = null;
      duplicates = "skip";
      importSettings = false;
    } catch (err) {
      handleError(err);
    }
  }

  function closeImport() {
    importFile = null;
    preview = null;
    importPassphrase = "";
    void api.vaultImportCancel();
  }

  async function decrypt(e: SubmitEvent) {
    e.preventDefault();
    if (!importPassphrase || importBusy) return;
    importBusy = true;
    importError = "";
    try {
      preview = await api.vaultImportPreview(importPassphrase);
      importPassphrase = "";
    } catch (err) {
      importError = errorMessage(err);
    } finally {
      importBusy = false;
    }
  }

  async function apply() {
    if (importBusy) return;
    importBusy = true;
    importError = "";
    try {
      const r = await api.vaultImportApply(duplicates, importSettings);
      importFile = null;
      preview = null;
      await reload();
      onimported();
      const parts = [`${r.added} ajouté${r.added > 1 ? "s" : ""}`];
      if (r.replaced) parts.push(`${r.replaced} remplacé${r.replaced > 1 ? "s" : ""}`);
      if (r.skipped) parts.push(`${r.skipped} ignoré${r.skipped > 1 ? "s" : ""}`);
      if (r.settings) parts.push("réglages appliqués");
      toast(`Import terminé : ${parts.join(", ")}`, "success", 6000);
    } catch (err) {
      importError = errorMessage(err);
    } finally {
      importBusy = false;
    }
  }

  function exportedOn(secs: number): string {
    return new Date(secs * 1000).toLocaleString("fr-FR", { dateStyle: "long", timeStyle: "short" });
  }

  function plural(n: number, word: string): string {
    return `${n} ${word}${n > 1 ? "s" : ""}`;
  }
</script>

<section class="card">
  <h2><Icon name="download" size={17} /> Sauvegarde et transfert</h2>
  <p class="hint">
    Exportez hôtes, trousseau, clés d'hôte et réglages dans un fichier chiffré par une phrase de passe, pour une
    sauvegarde ou un autre ordinateur. Les clés FIDO2 et le fournisseur d'identité ne sont pas exportés : ils se réactivent sur
    chaque installation.
  </p>
  <div class="actions">
    <button class="btn" onclick={pickImport}><Icon name="upload" size={16} /> Importer…</button>
    <button class="btn primary" onclick={openExport}><Icon name="download" size={16} /> Exporter…</button>
  </div>
</section>

{#if exporting}
  <Modal title="Exporter les données" onclose={() => !exportBusy && (exporting = false)} width={480}>
    <form id="export-form" class="form" onsubmit={runExport}>
      <fieldset>
        <legend>Contenu</legend>
        <label class="check"><input type="checkbox" bind:checked={selection.hosts} /> Hôtes ({data.hosts.length})</label>
        <label class="check">
          <input type="checkbox" bind:checked={selection.identities} /> Trousseau ({data.identities.length}), avec ses
          mots de passe et clés privées
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={selection.knownHosts} /> Clés d'hôte approuvées ({knownHostsCount})
        </label>
        <label class="check"><input type="checkbox" bind:checked={selection.settings} /> Réglages</label>
        {#if selection.hosts && !selection.identities}
          <small class="note">Les hôtes seront exportés sans leur identité.</small>
        {/if}
      </fieldset>
      <label class="field">
        <span>Phrase de passe de l'export</span>
        <PasswordInput bind:value={passphrase} placeholder="12 caractères minimum" autofocus />
      </label>
      <StrengthMeter password={passphrase} />
      <label class="field">
        <span>Confirmation</span>
        <PasswordInput bind:value={passphraseConfirm} />
        {#if passphraseConfirm && passphraseConfirm !== passphrase}
          <small class="error-text">Les phrases de passe ne correspondent pas.</small>
        {/if}
      </label>
      <p class="note">
        Elle sera demandée à l'import ; sans elle, le fichier est illisible. Choisissez-la différente du mot de passe
        maître.
      </p>
      <label class="field">
        <span>Mot de passe maître</span>
        <PasswordInput bind:value={master} />
      </label>
      {#if exportError}<p class="error-text">{exportError}</p>{/if}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (exporting = false)} disabled={exportBusy}>Annuler</button>
      <button class="btn primary" type="submit" form="export-form" disabled={!exportValid || exportBusy}>
        {exportBusy ? "Chiffrement…" : "Choisir l'emplacement…"}
      </button>
    {/snippet}
  </Modal>
{/if}

{#if importFile}
  <Modal title="Importer des données" onclose={() => !importBusy && closeImport()} width={480}>
    <p class="file"><Icon name="file" size={15} /> {importFile}</p>
    {#if !preview}
      <form id="import-form" class="form" onsubmit={decrypt}>
        <label class="field">
          <span>Phrase de passe de l'export</span>
          <PasswordInput bind:value={importPassphrase} autofocus />
        </label>
      </form>
    {:else}
      <p class="note">
        Exporté le {exportedOn(preview.createdAt)}{preview.appVersion ? ` (version ${preview.appVersion})` : ""}.
      </p>
      <table>
        <thead><tr><th></th><th>Nouveaux</th><th>Déjà présents</th></tr></thead>
        <tbody>
          <tr><td>Hôtes</td><td>{preview.hosts.new}</td><td>{preview.hosts.duplicates}</td></tr>
          <tr><td>Trousseau</td><td>{preview.identities.new}</td><td>{preview.identities.duplicates}</td></tr>
          <tr>
            <td>Clés d'hôte</td>
            <td>{preview.knownHosts.new}</td>
            <td>
              {preview.knownHosts.duplicates}
              {#if preview.knownHostsIdentical}<span class="faint">(+ {preview.knownHostsIdentical} identiques)</span>{/if}
            </td>
          </tr>
        </tbody>
      </table>
      {#if hasDuplicates}
        <fieldset>
          <legend>Éléments déjà présents</legend>
          <label class="check"><input type="radio" bind:group={duplicates} value="skip" /> Garder les miens</label>
          <label class="check">
            <input type="radio" bind:group={duplicates} value="replace" /> Remplacer par ceux du fichier
          </label>
          <label class="check">
            <input type="radio" bind:group={duplicates} value="keepBoth" /> Garder les deux (suffixe « importé »)
          </label>
          {#if preview.knownHosts.duplicates}
            <small class="note">
              {plural(preview.knownHosts.duplicates, "serveur")} a une clé différente dans le fichier : elle n'est
              remplacée qu'avec « Remplacer ». Vérifiez que le serveur a bien changé de clé.
            </small>
          {/if}
        </fieldset>
      {/if}
      {#if preview.settings}
        <label class="check">
          <input type="checkbox" bind:checked={importSettings} /> Appliquer aussi les réglages du fichier
        </label>
      {/if}
    {/if}
    {#if importError}<p class="error-text">{importError}</p>{/if}
    {#snippet footer()}
      <button class="btn" onclick={closeImport} disabled={importBusy}>Annuler</button>
      {#if !preview}
        <button class="btn primary" type="submit" form="import-form" disabled={!importPassphrase || importBusy}>
          {importBusy ? "Déchiffrement…" : "Déchiffrer"}
        </button>
      {:else}
        <button class="btn primary" onclick={apply} disabled={importBusy}>Importer</button>
      {/if}
    {/snippet}
  </Modal>
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
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
  }
  .form {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  fieldset {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0 0 14px;
    padding: 0;
    border: 0;
  }
  .form fieldset {
    margin: 0;
  }
  legend {
    margin-bottom: 8px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .check {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 13px;
  }
  .check input {
    margin-top: 3px;
  }
  .note {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
  }
  .file {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 14px;
    font-size: 13px;
    word-break: break-all;
  }
  table {
    width: 100%;
    margin: 12px 0 16px;
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
    text-align: right;
  }
  td {
    padding: 6px 0;
    border-top: 1px solid var(--border);
    text-align: right;
  }
  td:first-child {
    text-align: left;
  }
  .faint {
    color: var(--text-faint);
    font-size: 12px;
  }
</style>
