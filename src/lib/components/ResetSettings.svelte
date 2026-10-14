<script lang="ts">
  import { api, errorCode, errorMessage } from "../api";
  import { t } from "../i18n/index.svelte";
  import { notifyLocked, resetSessionState } from "../store.svelte";
  import { resetLocalPreferences } from "../reset";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import PasswordInput from "./PasswordInput.svelte";
  import { toast } from "./toast.svelte";

  let { onexport }: { onexport: () => void } = $props();

  let open = $state(false);
  let password = $state("");
  let word = $state("");
  let busy = $state(false);
  let error = $state("");

  const expected = $derived(t("reset.word"));
  const valid = $derived(password.length > 0 && word.trim() === expected);

  function show() {
    password = "";
    word = "";
    error = "";
    open = true;
  }

  function exportFirst() {
    open = false;
    onexport();
  }

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!valid || busy) return;
    busy = true;
    error = "";
    try {
      const leftovers = await api.appReset(password);
      password = "";
      open = false;
      resetSessionState();
      resetLocalPreferences();
      if (leftovers.length) toast(t("reset.leftovers", { paths: leftovers.join(", ") }), "error", 15000);
      notifyLocked();
    } catch (err) {
      error = errorCode(err) === "bad_password" ? t("err.badMasterPassword") : errorMessage(err);
    } finally {
      busy = false;
    }
  }
</script>

<section class="card">
  <h2><Icon name="trash" size={17} /> {t("reset.title")}</h2>
  <p class="hint">{t("reset.description")}</p>
  <div class="actions">
    <button class="btn danger" onclick={show}>{t("reset.open")}</button>
  </div>
</section>

{#if open}
  <Modal title={t("reset.dialogTitle")} onclose={() => !busy && (open = false)} width={500}>
    <form id="reset-form" class="form" onsubmit={submit}>
      <p class="text">{t("reset.deleted")}</p>
      <p class="text">{t("reset.kept")}</p>
      <div class="export">
        <p class="warning">{t("reset.exportHint")}</p>
        <button type="button" class="btn" onclick={exportFirst} disabled={busy}>
          <Icon name="download" size={16} />
          {t("reset.exportFirst")}
        </button>
      </div>
      <label class="field">
        <span>{t("reset.password")}</span>
        <PasswordInput bind:value={password} autofocus />
      </label>
      <label class="field">
        <span>{t("reset.typeWord", { word: expected })}</span>
        <input class="input mono" bind:value={word} autocomplete="off" autocapitalize="characters" spellcheck="false" />
      </label>
      {#if error}<p class="error-text">{error}</p>{/if}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => (open = false)} disabled={busy}>{t("common.cancel")}</button>
      <button class="btn danger" type="submit" form="reset-form" disabled={!valid || busy}>{t("reset.confirm")}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .card {
    padding: 20px 22px;
    background: var(--surface);
    border: 1px solid var(--danger-soft);
    border-radius: var(--radius-lg);
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 15px;
    margin-bottom: 8px;
    color: var(--danger);
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
  .text {
    color: var(--text-muted);
  }
  .export {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--danger-soft);
  }
  .warning {
    color: var(--danger);
    font-weight: 550;
  }
</style>
