<script lang="ts">
  import { intlLocale, t, tn } from "../i18n/index.svelte";
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
        toast(t("backup.exported", { file }), "success", 6000);
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
      const parts = [tn("backup.added", r.added)];
      if (r.replaced) parts.push(tn("backup.replaced", r.replaced));
      if (r.skipped) parts.push(tn("backup.skipped", r.skipped));
      if (r.settings) parts.push(t("backup.settingsApplied"));
      toast(t("backup.importDone", { details: parts.join(", ") }), "success", 6000);
    } catch (err) {
      importError = errorMessage(err);
    } finally {
      importBusy = false;
    }
  }

  function exportedOn(secs: number): string {
    return new Date(secs * 1000).toLocaleString(intlLocale(), { dateStyle: "long", timeStyle: "short" });
  }
</script>

<section class="card">
  <h2><Icon name="download" size={17} /> {t("backup.title")}</h2>
  <p class="hint">{t("backup.hint")}</p>
  <div class="actions">
    <button class="btn" onclick={pickImport}><Icon name="upload" size={16} /> {t("backup.import")}</button>
    <button class="btn primary" onclick={openExport}><Icon name="download" size={16} /> {t("backup.export")}</button>
  </div>
</section>

{#if exporting}
  <Modal title={t("backup.exportTitle")} onclose={() => !exportBusy && (exporting = false)} width={480}>
    <form id="export-form" class="form" onsubmit={runExport}>
      <fieldset>
        <legend>{t("backup.content")}</legend>
        <label class="check"
          ><input type="checkbox" bind:checked={selection.hosts} /> {t("backup.hosts", { count: data.hosts.length })}</label
        >
        <label class="check">
          <input type="checkbox" bind:checked={selection.identities} />
          {t("backup.identities", { count: data.identities.length })}
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={selection.knownHosts} /> {t("backup.knownHosts", { count: knownHostsCount })}
        </label>
        <label class="check"><input type="checkbox" bind:checked={selection.settings} /> {t("backup.settings")}</label>
        {#if selection.hosts && !selection.identities}
          <small class="note">{t("backup.hostsWithoutIdentity")}</small>
        {/if}
      </fieldset>
      <label class="field">
        <span>{t("backup.passphrase")}</span>
        <PasswordInput bind:value={passphrase} placeholder={t("lock.minLength", { count: 12 })} autofocus />
      </label>
      <StrengthMeter password={passphrase} />
      <label class="field">
        <span>{t("lock.confirmation")}</span>
        <PasswordInput bind:value={passphraseConfirm} />
        {#if passphraseConfirm && passphraseConfirm !== passphrase}
          <small class="error-text">{t("backup.passphraseMismatch")}</small>
        {/if}
      </label>
      <p class="note">
        {t("backup.passphraseNote")}
      </p>
      <label class="field">
        <span>{t("lock.masterPassword")}</span>
        <PasswordInput bind:value={master} />
      </label>
      {#if exportError}<p class="error-text">{exportError}</p>{/if}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (exporting = false)} disabled={exportBusy}>{t("common.cancel")}</button>
      <button class="btn primary" type="submit" form="export-form" disabled={!exportValid || exportBusy}>
        {t(exportBusy ? "backup.encrypting" : "backup.chooseLocation")}
      </button>
    {/snippet}
  </Modal>
{/if}

{#if importFile}
  <Modal title={t("backup.importTitle")} onclose={() => !importBusy && closeImport()} width={480}>
    <p class="file"><Icon name="file" size={15} /> {importFile}</p>
    {#if !preview}
      <form id="import-form" class="form" onsubmit={decrypt}>
        <label class="field">
          <span>{t("backup.passphrase")}</span>
          <PasswordInput bind:value={importPassphrase} autofocus />
        </label>
      </form>
    {:else}
      <p class="note">
        {t("backup.exportedOn", { date: exportedOn(preview.createdAt) })}{preview.appVersion
          ? t("backup.version", { version: preview.appVersion })
          : ""}.
      </p>
      <table>
        <thead><tr><th></th><th>{t("backup.new")}</th><th>{t("backup.existing")}</th></tr></thead>
        <tbody>
          <tr><td>{t("backup.rowHosts")}</td><td>{preview.hosts.new}</td><td>{preview.hosts.duplicates}</td></tr>
          <tr><td>{t("backup.rowKeychain")}</td><td>{preview.identities.new}</td><td>{preview.identities.duplicates}</td></tr>
          <tr>
            <td>{t("backup.rowKnownHosts")}</td>
            <td>{preview.knownHosts.new}</td>
            <td>
              {preview.knownHosts.duplicates}
              {#if preview.knownHostsIdentical}<span class="faint">{t("backup.identical", { count: preview.knownHostsIdentical })}</span>{/if}
            </td>
          </tr>
        </tbody>
      </table>
      {#if hasDuplicates}
        <fieldset>
          <legend>{t("backup.duplicates")}</legend>
          <label class="check"><input type="radio" bind:group={duplicates} value="skip" /> {t("backup.keepMine")}</label>
          <label class="check">
            <input type="radio" bind:group={duplicates} value="replace" /> {t("backup.replaceWithFile")}
          </label>
          <label class="check">
            <input type="radio" bind:group={duplicates} value="keepBoth" /> {t("backup.keepBoth")}
          </label>
          {#if preview.knownHosts.duplicates}
            <small class="note">
              {tn("backup.keyConflict", preview.knownHosts.duplicates)}
            </small>
          {/if}
        </fieldset>
      {/if}
      {#if preview.settings}
        <label class="check">
          <input type="checkbox" bind:checked={importSettings} /> {t("backup.applySettings")}
        </label>
      {/if}
    {/if}
    {#if importError}<p class="error-text">{importError}</p>{/if}
    {#snippet footer()}
      <button class="btn" onclick={closeImport} disabled={importBusy}>{t("common.cancel")}</button>
      {#if !preview}
        <button class="btn primary" type="submit" form="import-form" disabled={!importPassphrase || importBusy}>
          {t(importBusy ? "backup.decrypting" : "backup.decrypt")}
        </button>
      {:else}
        <button class="btn primary" onclick={apply} disabled={importBusy}>{t("backup.importAction")}</button>
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
