<script lang="ts">
  import { onDestroy, onMount, untrack } from "svelte";
  import { api, errorCode, errorMessage, type Fido2Device, type Fido2Key } from "../api";
  import { APP_NAME } from "../identity";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import PasswordInput from "./PasswordInput.svelte";

  let {
    existing,
    onclose,
    onadded,
  }: { existing: number; onclose: () => void; onadded: (key: Fido2Key) => void } = $props();

  type Step = "detect" | "create-pin" | "details" | "touch" | "done";
  const STEPS: [Step[], string][] = [
    [["detect", "create-pin"], "Brancher"],
    [["details"], "Renseigner"],
    [["touch"], "Toucher"],
  ];

  let step = $state<Step>("detect");
  let device = $state<Fido2Device | null>(null);
  let detectError = $state("");
  let error = $state("");
  let busy = $state(false);

  let newPin = $state("");
  let newPinConfirm = $state("");

  // Valeurs initiales seulement : `existing` ne change pas pendant l'assistant.
  const firstKey = untrack(() => existing === 0);
  let label = $state(firstKey ? "Ma clé de sécurité" : "Clé de secours");
  let pin = $state("");
  let password = $state("");
  let touch = $state(0);
  let added = $state<Fido2Key | null>(null);

  let timer: ReturnType<typeof setTimeout> | undefined;
  let alive = true;

  // Détection en continu tant qu'on est à la première étape : brancher ou débrancher
  // la clé met l'affichage à jour tout seul.
  async function detect() {
    if (!alive || step !== "detect") return;
    try {
      device = await api.fido2Detect();
      detectError = "";
      if (device.product && firstKey && label === "Ma clé de sécurité") label = device.product;
    } catch (e) {
      device = null;
      detectError = errorMessage(e);
    }
    if (alive && step === "detect") timer = setTimeout(detect, 1200);
  }

  onMount(detect);
  onDestroy(() => {
    alive = false;
    clearTimeout(timer);
  });

  function goTo(next: Step) {
    clearTimeout(timer);
    error = "";
    step = next;
    if (next === "detect") detect();
  }

  const pinTooShort = $derived(newPin.length > 0 && [...newPin].length < (device?.minPinLength ?? 4));
  const pinMismatch = $derived(newPinConfirm.length > 0 && newPin !== newPinConfirm);
  const canCreatePin = $derived(
    !busy && newPin.length > 0 && !pinTooShort && newPin === newPinConfirm,
  );

  async function createPin(e: SubmitEvent) {
    e.preventDefault();
    if (!canCreatePin) return;
    busy = true;
    error = "";
    try {
      await api.fido2SetPin(newPin);
      pin = newPin;
      newPin = newPinConfirm = "";
      goTo("detect");
    } catch (err) {
      error = errorMessage(err);
    } finally {
      busy = false;
    }
  }

  const canEnroll = $derived(!busy && label.trim().length > 0 && pin.length > 0 && password.length > 0);

  async function enroll(e: SubmitEvent) {
    e.preventDefault();
    if (!canEnroll) return;
    busy = true;
    error = "";
    touch = 0;
    step = "touch";
    try {
      added = await api.fido2Enroll(label.trim(), pin, password, (t) => (touch = t));
      pin = password = "";
      step = "done";
      onadded(added);
    } catch (err) {
      const code = errorCode(err);
      error = errorMessage(err);
      if (code === "fido2_pin") pin = "";
      if (code === "bad_password") password = "";
      step = "details";
    } finally {
      busy = false;
    }
  }

  const currentIndex = $derived(step === "done" ? STEPS.length : STEPS.findIndex(([steps]) => steps.includes(step)));
</script>

<Modal title="Ajouter une clé de sécurité" onclose={() => !busy && onclose()} width={500}>
  <ol class="stepper">
    {#each STEPS as [, name], i}
      <li class:active={i === currentIndex} class:done={i < currentIndex}>
        <span class="dot">{#if i < currentIndex}<Icon name="check" size={12} />{:else}{i + 1}{/if}</span>
        {name}
      </li>
    {/each}
  </ol>

  {#if step === "detect"}
    <div class="panel">
      {#if detectError}
        <div class="status bad"><Icon name="alert" size={20} /><div><strong>Clé illisible</strong><span>{detectError}</span></div></div>
      {:else if !device}
        <div class="status"><span class="spinner"></span><div><strong>Recherche d'une clé…</strong></div></div>
      {:else if device.state === "none"}
        <div class="status waiting">
          <Icon name="key" size={22} />
          <div>
            <strong>Branchez votre clé de sécurité</strong>
            <span>YubiKey, Nitrokey, SoloKey… sur un port USB. Elle sera détectée automatiquement.</span>
          </div>
        </div>
      {:else if device.state === "multiple"}
        <div class="status bad">
          <Icon name="alert" size={20} />
          <div><strong>Plusieurs clés sont branchées</strong><span>Ne laissez que celle à enregistrer.</span></div>
        </div>
      {:else if device.state === "unsupported"}
        <div class="status bad">
          <Icon name="alert" size={20} />
          <div>
            <strong>{device.product ?? "Cette clé"} n'est pas compatible</strong>
            <span>Il faut une clé FIDO2 avec code PIN et extension hmac-secret. Les anciennes clés U2F seules ne suffisent pas.</span>
          </div>
        </div>
      {:else}
        <div class="status ok">
          <Icon name="check" size={20} />
          <div>
            <strong>{device.product ?? "Clé de sécurité"} détectée</strong>
            {#if device.hasPin}
              <span>Code PIN défini{device.pinRetries !== null ? ` · ${device.pinRetries} essais restants avant blocage` : ""}.</span>
            {:else}
              <span>Cette clé n'a pas encore de code PIN : il est obligatoire, créons-le.</span>
            {/if}
          </div>
        </div>
      {/if}
      <p class="note">
        Gardez la clé branchée jusqu'à la fin. Ne la touchez que lorsque {APP_NAME} vous le demande : un toucher à un
        autre moment peut taper un code dans la fenêtre active.
      </p>
    </div>
  {:else if step === "create-pin"}
    <form id="fido2-pin" class="panel" onsubmit={createPin}>
      <p class="intro">
        Le code PIN protège la clé : sans lui, quelqu'un qui la trouverait ne pourrait pas ouvrir votre coffre. Il est
        enregistré <strong>dans la clé</strong> et servira aussi aux autres sites et applications qui l'utilisent.
      </p>
      <label class="field">
        <span>Nouveau code PIN</span>
        <PasswordInput bind:value={newPin} autofocus />
        {#if pinTooShort}
          <small class="error-text">Au moins {device?.minPinLength ?? 4} caractères.</small>
        {:else}
          <small>{device?.minPinLength ?? 4} caractères minimum ; chiffres, lettres ou les deux.</small>
        {/if}
      </label>
      <label class="field">
        <span>Confirmation</span>
        <PasswordInput bind:value={newPinConfirm} />
        {#if pinMismatch}<small class="error-text">Les deux codes ne correspondent pas.</small>{/if}
      </label>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
    </form>
  {:else if step === "details"}
    <form id="fido2-details" class="panel" onsubmit={enroll}>
      <label class="field">
        <span>Nom de la clé</span>
        <input class="input" bind:value={label} maxlength="60" spellcheck="false" />
        <small>Pour la reconnaître dans la liste, par exemple « YubiKey bleue ».</small>
      </label>
      <label class="field">
        <span>Code PIN de la clé</span>
        <PasswordInput bind:value={pin} autofocus={!pin} />
        <small>
          Le code de la <strong>clé</strong>, pas le mot de passe de {APP_NAME}.
          {#if device?.pinRetries}{device.pinRetries} essais avant blocage.{/if}
        </small>
      </label>
      <label class="field">
        <span>Mot de passe maître</span>
        <PasswordInput bind:value={password} autofocus={!!pin} />
        <small>Pour confirmer que c'est bien vous qui ajoutez cette clé.</small>
      </label>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
    </form>
  {:else if step === "touch"}
    <div class="panel">
      <div class="touch">
        <div class="pulse"><Icon name="key" size={30} /></div>
        {#if touch === 0}
          <strong>Préparation…</strong>
          <span>Vérification du code PIN.</span>
        {:else}
          <strong>Touchez votre clé ({touch}/2)</strong>
          <span>
            {touch === 1
              ? "Elle clignote : touchez-la pour créer l'identifiant."
              : "Elle clignote à nouveau : touchez-la une seconde fois pour terminer."}
          </span>
        {/if}
      </div>
      <ol class="touches">
        <li class:active={touch === 1} class:done={touch > 1}>1<sup>er</sup> toucher : création de l'identifiant</li>
        <li class:active={touch === 2}>2<sup>e</sup> toucher : calcul du secret</li>
      </ol>
    </div>
  {:else}
    <div class="panel">
      <div class="status ok">
        <Icon name="check" size={20} />
        <div>
          <strong>Clé « {added?.label} » enregistrée</strong>
          <span>Elle peut maintenant déverrouiller le coffre.</span>
        </div>
      </div>
      <p class="intro">
        Pour l'essayer : verrouillez avec <kbd>Ctrl</kbd>+<kbd>Maj</kbd>+<kbd>L</kbd>, choisissez « Utiliser une clé de
        sécurité », saisissez le code PIN puis touchez la clé.
        {#if firstKey}<br />Pensez à enregistrer aussi une clé de secours, au cas où celle-ci serait perdue.{/if}
      </p>
    </div>
  {/if}

  {#snippet footer()}
    {#if step === "detect"}
      <button class="btn" onclick={onclose}>Annuler</button>
      {#if device?.state === "ready" && !device.hasPin}
        <button class="btn primary" onclick={() => goTo("create-pin")}>Créer un code PIN</button>
      {:else}
        <button class="btn primary" disabled={device?.state !== "ready"} onclick={() => goTo("details")}>Continuer</button>
      {/if}
    {:else if step === "create-pin"}
      <button class="btn" onclick={() => goTo("detect")} disabled={busy}>Retour</button>
      <button class="btn primary" type="submit" form="fido2-pin" disabled={!canCreatePin}>
        {busy ? "Création…" : "Créer le code PIN"}
      </button>
    {:else if step === "details"}
      <button class="btn" onclick={() => goTo("detect")}>Retour</button>
      <button class="btn primary" type="submit" form="fido2-details" disabled={!canEnroll}>Enregistrer la clé</button>
    {:else if step === "touch"}
      <span class="footnote">Sans toucher, la clé abandonne d'elle-même au bout de 30 secondes environ.</span>
    {:else}
      <button class="btn primary" onclick={onclose}>Terminer</button>
    {/if}
  {/snippet}
</Modal>

<style>
  .stepper {
    display: flex;
    gap: 6px;
    list-style: none;
    margin: 0 0 18px;
    padding: 0;
  }
  .stepper li {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    padding-bottom: 8px;
    border-bottom: 2px solid var(--border);
    font-size: 12.5px;
    color: var(--text-faint);
  }
  .stepper li.active {
    color: var(--text);
    border-color: var(--accent);
  }
  .stepper li.done {
    color: var(--text-muted);
    border-color: var(--success);
  }
  .dot {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 11px;
    font-weight: 600;
  }
  .active .dot {
    background: var(--accent);
    border-color: var(--accent);
    color: #fff;
  }
  .done .dot {
    background: var(--success);
    border-color: var(--success);
    color: #0d0f14;
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 150px;
  }
  .intro,
  .note {
    color: var(--text-muted);
    font-size: 12.5px;
    margin: 0;
  }
  .note {
    color: var(--text-faint);
  }
  .status {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .status div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
  }
  .status div span {
    color: var(--text-muted);
    font-size: 12.5px;
  }
  .status.ok :global(svg) {
    color: var(--success);
  }
  .status.bad :global(svg) {
    color: var(--danger);
  }
  .status.waiting :global(svg) {
    color: var(--accent);
    animation: fade 1.6s ease-in-out infinite;
  }
  .touch {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 10px 0 4px;
    text-align: center;
  }
  .touch strong {
    font-size: 15px;
  }
  .touch span {
    color: var(--text-muted);
    font-size: 12.5px;
    max-width: 360px;
  }
  .pulse {
    display: grid;
    place-items: center;
    width: 64px;
    height: 64px;
    margin-bottom: 6px;
    border-radius: 50%;
    background: var(--accent-soft);
    color: var(--accent);
    animation: fade 1.2s ease-in-out infinite;
  }
  .touches {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12.5px;
    color: var(--text-faint);
  }
  .touches li::before {
    content: "○ ";
  }
  .touches li.active {
    color: var(--text);
  }
  .touches li.active::before {
    content: "● ";
    color: var(--accent);
  }
  .touches li.done {
    color: var(--text-muted);
  }
  .touches li.done::before {
    content: "✓ ";
    color: var(--success);
  }
  .footnote {
    margin-right: auto;
    font-size: 12px;
    color: var(--text-faint);
  }
  kbd {
    padding: 0 5px;
    border-radius: 4px;
    border: 1px solid var(--border);
    background: var(--bg);
    font-family: var(--font-mono);
    font-size: 11.5px;
  }
  .spinner {
    width: 18px;
    height: 18px;
    flex-shrink: 0;
    border-radius: 50%;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  /* Opacité seulement : les transformations animées font scintiller WebKitGTK. */
  @keyframes fade {
    50% {
      opacity: 0.45;
    }
  }
</style>
