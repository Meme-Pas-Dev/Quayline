<script lang="ts">
  import { t, type MessageKey } from "../i18n/index.svelte";
  import { KEYS } from "../platform";
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
  const STEPS: [Step[], MessageKey][] = [
    [["detect", "create-pin"], "wizard.step.plug"],
    [["details"], "wizard.step.details"],
    [["touch"], "wizard.step.touch"],
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
  const defaultLabel = t("wizard.defaultLabel");
  let label = $state(firstKey ? defaultLabel : t("wizard.backupLabel"));
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
      if (device.product && firstKey && label === defaultLabel) label = device.product;
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

  // Windows : la clé est jointe par la fenêtre du système (PIN, toucher), pas ici.
  const viaWindows = $derived(device?.state === "windows");
  const canEnroll = $derived(!busy && label.trim().length > 0 && (pin.length > 0 || viaWindows) && password.length > 0);

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

<Modal title={t("wizard.title")} onclose={() => !busy && onclose()} width={500}>
  <ol class="stepper">
    {#each STEPS as [, name], i}
      <li class:active={i === currentIndex} class:done={i < currentIndex}>
        <span class="dot">{#if i < currentIndex}<Icon name="check" size={12} />{:else}{i + 1}{/if}</span>
        {t(name)}
      </li>
    {/each}
  </ol>

  {#if step === "detect"}
    <div class="panel">
      {#if detectError}
        <div class="status bad"><Icon name="alert" size={20} /><div><strong>{t("wizard.unreadable")}</strong><span>{detectError}</span></div></div>
      {:else if !device}
        <div class="status"><span class="spinner"></span><div><strong>{t("wizard.searching")}</strong></div></div>
      {:else if device.state === "none"}
        <div class="status waiting">
          <Icon name="key" size={22} />
          <div>
            <strong>{t("wizard.plugIn")}</strong>
            <span>{t("wizard.plugInHint")}</span>
          </div>
        </div>
      {:else if device.state === "multiple"}
        <div class="status bad">
          <Icon name="alert" size={20} />
          <div><strong>{t("wizard.multiple")}</strong><span>{t("wizard.multipleHint")}</span></div>
        </div>
      {:else if device.state === "windows"}
        <div class="status ok">
          <Icon name="key" size={20} />
          <div><strong>{t("wizard.windowsReady")}</strong><span>{t("wizard.windowsReadyHint")}</span></div>
        </div>
      {:else if device.state === "unsupported"}
        <div class="status bad">
          <Icon name="alert" size={20} />
          <div>
            <strong>{t("wizard.unsupported", { device: device.product ?? t("wizard.thisKey") })}</strong>
            <span>{t("wizard.unsupportedHint")}</span>
          </div>
        </div>
      {:else}
        <div class="status ok">
          <Icon name="check" size={20} />
          <div>
            <strong>{t("wizard.detected", { device: device.product ?? t("wizard.securityKey") })}</strong>
            {#if device.hasPin}
              <span
                >{t("wizard.pinSet")}{device.pinRetries !== null ? t("wizard.retriesLeft", { count: device.pinRetries }) : ""}.</span
              >
            {:else}
              <span>{t("wizard.noPin")}</span>
            {/if}
          </div>
        </div>
      {/if}
      <p class="note">
        {t("wizard.keepPlugged", { app: APP_NAME })}
      </p>
    </div>
  {:else if step === "create-pin"}
    <form id="fido2-pin" class="panel" onsubmit={createPin}>
      <p class="intro">
        {t("wizard.pinIntro.before")}<strong>{t("wizard.pinIntro.strong")}</strong>{t("wizard.pinIntro.after")}
      </p>
      <label class="field">
        <span>{t("wizard.newPin")}</span>
        <PasswordInput bind:value={newPin} autofocus />
        {#if pinTooShort}
          <small class="error-text">{t("wizard.pinTooShort", { count: device?.minPinLength ?? 4 })}</small>
        {:else}
          <small>{t("wizard.pinHint", { count: device?.minPinLength ?? 4 })}</small>
        {/if}
      </label>
      <label class="field">
        <span>{t("lock.confirmation")}</span>
        <PasswordInput bind:value={newPinConfirm} />
        {#if pinMismatch}<small class="error-text">{t("wizard.pinMismatch")}</small>{/if}
      </label>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
    </form>
  {:else if step === "details"}
    <form id="fido2-details" class="panel" onsubmit={enroll}>
      <label class="field">
        <span>{t("wizard.keyName")}</span>
        <input class="input" bind:value={label} maxlength="60" spellcheck="false" />
        <small>{t("wizard.keyNameHint")}</small>
      </label>
      {#if !viaWindows}
      <label class="field">
        <span>{t("wizard.keyPin")}</span>
        <PasswordInput bind:value={pin} autofocus={!pin} />
        <small>
          {t("wizard.keyPinHint.before")}<strong>{t("wizard.keyPinHint.strong")}</strong>{t("wizard.keyPinHint.after", { app: APP_NAME })}
          {#if device?.pinRetries}{t("wizard.retriesBefore", { count: device.pinRetries })}{/if}
        </small>
      </label>
      {/if}
      <label class="field">
        <span>{t("lock.masterPassword")}</span>
        <PasswordInput bind:value={password} autofocus={!!pin || viaWindows} />
        <small>{t("wizard.passwordHint")}</small>
      </label>
      {#if error}<p class="error-text" role="alert">{error}</p>{/if}
    </form>
  {:else if step === "touch"}
    <div class="panel">
      <div class="touch">
        <div class="pulse"><Icon name="key" size={30} /></div>
        {#if touch === 0}
          <strong>{t("wizard.preparing")}</strong>
          <span>{t("wizard.checkingPin")}</span>
        {:else}
          <strong>{t("wizard.touchKey", { step: touch })}</strong>
          <span>{viaWindows ? t("wizard.windowsTouch", { step: touch }) : t(touch === 1 ? "wizard.touch1" : "wizard.touch2")}</span>
        {/if}
      </div>
      <ol class="touches">
        <li class:active={touch === 1} class:done={touch > 1}>{t("wizard.touchStep1")}</li>
        <li class:active={touch === 2}>{t("wizard.touchStep2")}</li>
      </ol>
    </div>
  {:else}
    <div class="panel">
      <div class="status ok">
        <Icon name="check" size={20} />
        <div>
          <strong>{t("wizard.registered", { name: added?.label ?? "" })}</strong>
          <span>{t("wizard.registeredHint")}</span>
        </div>
      </div>
      <p class="intro">
        {t("wizard.tryIt", { keys: KEYS.lock })}
        {#if firstKey}<br />{t("wizard.backupAdvice")}{/if}
      </p>
    </div>
  {/if}

  {#snippet footer()}
    {#if step === "detect"}
      <button class="btn" onclick={onclose}>{t("common.cancel")}</button>
      {#if device?.state === "ready" && !device.hasPin}
        <button class="btn primary" onclick={() => goTo("create-pin")}>{t("wizard.createPin")}</button>
      {:else}
        <button class="btn primary" disabled={device?.state !== "ready" && !viaWindows} onclick={() => goTo("details")}>{t("wizard.continue")}</button>
      {/if}
    {:else if step === "create-pin"}
      <button class="btn" onclick={() => goTo("detect")} disabled={busy}>{t("wizard.back")}</button>
      <button class="btn primary" type="submit" form="fido2-pin" disabled={!canCreatePin}>
        {t(busy ? "wizard.creating" : "wizard.createPinAction")}
      </button>
    {:else if step === "details"}
      <button class="btn" onclick={() => goTo("detect")}>{t("wizard.back")}</button>
      <button class="btn primary" type="submit" form="fido2-details" disabled={!canEnroll}>{t("wizard.registerKey")}</button>
    {:else if step === "touch"}
      <span class="footnote">{t("wizard.timeoutNote")}</span>
    {:else}
      <button class="btn primary" onclick={onclose}>{t("wizard.finish")}</button>
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
