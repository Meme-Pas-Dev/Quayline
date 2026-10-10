<script lang="ts">
  import { APP_NAME } from "../identity";
  let { size = 40 }: { size?: number } = $props();
</script>

<svg width={size} height={size} viewBox="0 0 64 64" aria-label={APP_NAME}>
  <defs>
    <linearGradient id="tm-grad" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#7c8cff" />
      <stop offset="1" stop-color="#38d6c4" />
    </linearGradient>
  </defs>
  <rect width="64" height="64" rx="16" fill="url(#tm-grad)" />
  <path d="M18 22l10 10-10 10" fill="none" stroke="#fff" stroke-width="5" stroke-linecap="round" stroke-linejoin="round" />
  <path d="M33 43h14" stroke="#fff" stroke-width="5" stroke-linecap="round" />
</svg>
