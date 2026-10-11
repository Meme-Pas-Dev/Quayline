<script lang="ts">
  import type { Snippet } from "svelte";

  // `width` : largeur du contenu de la page, centré ; l'en-tête s'aligne dessus.
  let { title, subtitle, actions, width }: { title: string; subtitle?: string; actions?: Snippet; width?: number } =
    $props();
</script>

<header style:--content-width={width ? `${width}px` : null} class:centered={!!width}>
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
  /* Marges égales à gauche et à droite autour d'un contenu de largeur limitée. */
  header.centered {
    padding-inline: max(32px, calc((100% - var(--content-width)) / 2));
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
