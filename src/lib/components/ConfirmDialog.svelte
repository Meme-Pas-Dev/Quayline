<script lang="ts">
  import Modal from "./Modal.svelte";

  let {
    title,
    message,
    confirmLabel = "Supprimer",
    onconfirm,
    onclose,
  }: { title: string; message: string; confirmLabel?: string; onconfirm: () => void; onclose: () => void } =
    $props();
</script>

<Modal {title} {onclose} width={420}>
  <p style="color: var(--text-muted)">{message}</p>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>Annuler</button>
    <button class="btn danger" onclick={onconfirm}>{confirmLabel}</button>
  {/snippet}
</Modal>
