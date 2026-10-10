<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import type { QrMatrix } from "../api";

  let { qr, size = 200 }: { qr: QrMatrix; size?: number } = $props();

  // Marge blanche de 4 modules exigée par la norme pour une lecture fiable.
  const QUIET = 4;

  const path = $derived.by(() => {
    let d = "";
    for (let y = 0; y < qr.size; y++) {
      for (let x = 0; x < qr.size; x++) {
        if (qr.modules[y * qr.size + x]) d += `M${x + QUIET} ${y + QUIET}h1v1h-1z`;
      }
    }
    return d;
  });
</script>

<!-- Toujours sombre sur clair, quel que soit le thème : condition de lecture par les téléphones. -->
<svg
  width={size}
  height={size}
  viewBox="0 0 {qr.size + QUIET * 2} {qr.size + QUIET * 2}"
  shape-rendering="crispEdges"
  role="img"
  aria-label={t("qr.label")}
>
  <rect width="100%" height="100%" fill="#fff" />
  <path d={path} fill="#000" />
</svg>

<style>
  svg {
    display: block;
    border-radius: 12px;
  }
</style>
