<script lang="ts">
  let { password }: { password: string } = $props();

  // Estimation simple de l'entropie (longueur × taille de l'alphabet utilisé).
  const bits = $derived.by(() => {
    if (!password) return 0;
    let pool = 0;
    if (/[a-z]/.test(password)) pool += 26;
    if (/[A-Z]/.test(password)) pool += 26;
    if (/[0-9]/.test(password)) pool += 10;
    if (/[^a-zA-Z0-9]/.test(password)) pool += 33;
    const unique = new Set(password).size;
    return Math.round(Math.min(password.length, unique * 1.5) * Math.log2(Math.max(pool, 2)));
  });

  const level = $derived(bits < 50 ? 0 : bits < 70 ? 1 : bits < 90 ? 2 : 3);
  const labels = ["Faible", "Moyen", "Bon", "Excellent"];
  const colors = ["var(--danger)", "var(--warning)", "var(--accent-2)", "var(--success)"];
</script>

<div class="meter" aria-label="Robustesse : {labels[level]}">
  <div class="bars">
    {#each [0, 1, 2, 3] as i}
      <span style:background={password && i <= level ? colors[level] : undefined}></span>
    {/each}
  </div>
  {#if password}<small style:color={colors[level]}>{labels[level]} · ~{bits} bits</small>{/if}
</div>

<style>
  .meter {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .bars {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 6px;
  }
  .bars span {
    height: 4px;
    border-radius: 4px;
    background: var(--border);
    transition: background 0.2s;
  }
  small {
    font-size: 12px;
    font-weight: 550;
  }
</style>
