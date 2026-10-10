<script lang="ts">
  import Icon from "./Icon.svelte";
  import { toasts } from "./toast.svelte";
</script>

<div class="stack" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast {t.kind}">
      <Icon name={t.kind === "error" ? "alert" : "check"} size={16} />
      <span>{t.message}</span>
    </div>
  {/each}
</div>

<style>
  .stack {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 100;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 380px;
    padding: 10px 14px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border-strong);
    box-shadow: var(--shadow);
  }
  .success :global(svg) {
    color: var(--success);
  }
  .error :global(svg) {
    color: var(--danger);
  }
  .info :global(svg) {
    color: var(--accent);
  }
</style>
