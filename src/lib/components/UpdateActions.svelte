<script lang="ts">
  // Boutons et état d'une mise à jour disponible : barre latérale (compact) et
  // Réglages → À propos.
  import { t } from "../i18n/index.svelte";
  import { APP_NAME } from "../identity";
  import { installUpdate, openRelease, restartAfterUpdate, skipUpdate, update } from "../updates.svelte";
  import Modal from "./Modal.svelte";

  let { compact = false }: { compact?: boolean } = $props();
  let confirming = $state(false);

  const percent = $derived(
    update.install?.step === "download" && update.install.total ? Math.floor((update.install.done / update.install.total) * 100) : 0,
  );

  function install() {
    confirming = false;
    installUpdate();
  }
</script>

{#if update.available}
  {#if update.install?.step === "installed"}
    <span class="state" class:compact>{t("update.installed", { version: update.available.version })}</span>
    <div class="actions" class:compact>
      <button class="btn primary" onclick={restartAfterUpdate}>{t("update.restart")}</button>
    </div>
  {:else if update.install}
    <span class="state faint" class:compact>{update.install.step === "download" ? t("update.downloading", { percent }) : t("update.verifying")}</span>
  {:else}
    <div class="actions" class:compact>
      {#if update.available.installable}
        <button class="btn primary" onclick={() => (confirming = true)}>{t("update.install")}</button>
        <button class="btn" onclick={openRelease}>{t("update.view")}</button>
      {:else}
        <button class="btn primary" onclick={openRelease}>{t(compact ? "update.view" : "update.viewRelease")}</button>
      {/if}
      {#if compact}<button class="btn ghost" onclick={skipUpdate}>{t("update.skip")}</button>{/if}
    </div>
  {/if}
  {#if compact && update.error}<span class="error-text">{update.error}</span>{/if}
{/if}

{#if confirming && update.available}
  <Modal title={t("update.installTitle", { version: update.available.version })} onclose={() => (confirming = false)} width={440}>
    <p style="color: var(--text-muted)">{t("update.installMessage", { app: APP_NAME })}</p>
    {#snippet footer()}
      <button class="btn" onclick={() => (confirming = false)}>{t("common.cancel")}</button>
      <button class="btn primary" onclick={install}>{t("update.install")}</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  /* Encart de la barre latérale : boutons centrés. */
  .actions.compact {
    justify-content: center;
  }
  .state.compact {
    text-align: center;
  }
  .state {
    font-size: 12.5px;
  }
  .faint {
    color: var(--text-faint);
  }
  /* Message d'erreur de l'encart : hauteur limitée, défilement au-delà. */
  .error-text {
    font-size: 12px;
    max-height: 7.5em;
    overflow-y: auto;
    overflow-wrap: anywhere;
  }
</style>
