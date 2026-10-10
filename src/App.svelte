<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import { api, type VaultStatus } from "./lib/api";
  import { data, ui } from "./lib/store.svelte";
  import Toasts from "./lib/components/Toasts.svelte";
  import LockScreen from "./lib/screens/LockScreen.svelte";
  import Shell from "./lib/screens/Shell.svelte";

  let status = $state<VaultStatus | null>(null);
  let lockReason = $state<string | undefined>();
  /** Sessions gardées ouvertes derrière l'écran de verrouillage (selon le réglage). */
  let keepSessions = $state(false);
  const openSessions = $derived(ui.sessions.filter((s) => s.status !== "closed").length);

  function sessionsSurviveLock() {
    return data.settings?.lock_closes_sessions === false && openSessions > 0;
  }

  async function refresh() {
    status = await api.vaultStatus();
  }

  async function lock() {
    keepSessions = sessionsSurviveLock();
    await api.vaultLock();
    lockReason = undefined;
    await refresh();
  }

  // Signale l'activité au backend (au plus toutes les 20 s) pour le verrouillage automatique.
  let lastPing = 0;
  function activity() {
    if (!status?.unlocked) return;
    const now = Date.now();
    if (now - lastPing > 20_000) {
      lastPing = now;
      api.vaultTouch().catch(() => {});
    }
  }

  onMount(() => {
    refresh();
    const unlisten = listen<string>("vault://locked", async (e) => {
      keepSessions = sessionsSurviveLock();
      lockReason = e.payload;
      await refresh();
    });
    return () => {
      unlisten.then((f) => f());
    };
  });
</script>

<svelte:window onkeydown={activity} onpointermove={activity} onpointerdown={activity} />

{#if status}
  {#if status.unlocked || keepSessions}
    <!-- Verrouillé avec sessions conservées : l'interface reste montée (les connexions
         vivent), mais masquée et inerte (ni clavier, ni souris, ni lecteur d'écran). -->
    <div class="shell-host" class:locked={!status.unlocked} inert={!status.unlocked}>
      <Shell onlock={lock} onlocked={refresh} />
    </div>
  {/if}
  {#if !status.unlocked}
    <div class="lock-host">
      {#key status.exists}
        <LockScreen
          mode={status.exists ? "unlock" : "setup"}
          reason={lockReason}
          backgroundSessions={keepSessions ? openSessions : 0}
          onunlocked={refresh}
        />
      {/key}
    </div>
  {/if}
{/if}

<Toasts />

<style>
  .shell-host {
    height: 100%;
  }
  .shell-host.locked {
    visibility: hidden;
  }
  .lock-host {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: var(--bg);
  }
</style>
