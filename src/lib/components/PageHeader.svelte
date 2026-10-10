<script lang="ts">
  import type { Snippet } from "svelte";

  let { title, subtitle, actions }: { title: string; subtitle?: string; actions?: Snippet } = $props();
</script>

<header>
  <div>
    <h1>{title}</h1>
    {#if subtitle}<p>{subtitle}</p>{/if}
  </div>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
</header>

<style>
  header {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 22px 32px 18px;
    background: var(--bg);
  }
  h1 {
    font-size: 22px;
  }
  p {
    color: var(--text-muted);
    margin-top: 2px;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
</style>
