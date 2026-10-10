<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import Modal from "./Modal.svelte";

  let {
    title,
    message,
    confirmLabel = t("common.delete"),
    onconfirm,
    onclose,
  }: { title: string; message: string; confirmLabel?: string; onconfirm: () => void; onclose: () => void } =
    $props();
</script>

<Modal {title} {onclose} width={420}>
  <p style="color: var(--text-muted)">{message}</p>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t("common.cancel")}</button>
    <button class="btn danger" onclick={onconfirm}>{confirmLabel}</button>
  {/snippet}
</Modal>
