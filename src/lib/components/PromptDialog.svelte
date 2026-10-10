<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import Modal from "./Modal.svelte";

  // Petite boîte de saisie (nouveau dossier, renommer…).
  let {
    title,
    label,
    initial = "",
    confirmLabel = t("common.confirm"),
    onsubmit,
    onclose,
  }: {
    title: string;
    label: string;
    initial?: string;
    confirmLabel?: string;
    onsubmit: (value: string) => void;
    onclose: () => void;
  } = $props();

  // svelte-ignore state_referenced_locally
  let value = $state(initial);
  let input: HTMLInputElement;

  function submit(e: SubmitEvent) {
    e.preventDefault();
    if (value.trim()) onsubmit(value.trim());
  }

  // Sélectionner le nom sans l'extension, comme dans Dolphin.
  $effect(() => {
    requestAnimationFrame(() => {
      if (!input) return;
      input.focus();
      const dot = value.lastIndexOf(".");
      input.setSelectionRange(0, dot > 0 ? dot : value.length);
    });
  });
</script>

<Modal {title} {onclose} width={420}>
  <form id="prompt-dialog" onsubmit={submit}>
    <label class="field">
      <span>{label}</span>
      <input class="input" bind:this={input} bind:value spellcheck="false" autocomplete="off" />
    </label>
  </form>
  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t("common.cancel")}</button>
    <button class="btn primary" type="submit" form="prompt-dialog" disabled={!value.trim()}>{confirmLabel}</button>
  {/snippet}
</Modal>
