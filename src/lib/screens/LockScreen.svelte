<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api, errorCode, errorMessage, type OidcDeviceStart, type OidcStatus } from "../api";
  import { APP_NAME } from "../identity";
  import Icon from "../components/Icon.svelte";
  import Logo from "../components/Logo.svelte";
  import PasswordInput from "../components/PasswordInput.svelte";
  import QrCode from "../components/QrCode.svelte";
  import StrengthMeter from "../components/StrengthMeter.svelte";

  const MIN_LENGTH = 12;

  let {
    mode,
    onunlocked,
    reason,
    backgroundSessions = 0,
  }: { mode: "setup" | "unlock"; onunlocked: () => void; reason?: string; backgroundSessions?: number } = $props();

  let password = $state("");
  let confirm = $state("");
  let acknowledged = $state(false);
  let busy = $state(false);
  let error = $state("");
  let oidc = $state<OidcStatus | null>(null);
  let oidcWaiting = $state(false);
  let device = $state<OidcDeviceStart | null>(null);
  let deviceStarting = $state(false);
  let deviceLeft = $state(0);
  let deviceTimer: ReturnType<typeof setInterval> | undefined;
  let hasSecurityKey = $state(false);
  let keyOpen = $state(false);
  let keyPin = $state("");
  let keyWaiting = $state(false);

  onMount(() => {
    if (mode !== "unlock") return;
    api.oidcStatus().then((s) => (oidc = s)).catch(() => {});
    api.fido2List().then((k) => (hasSecurityKey = k.length > 0)).catch(() => {});
  });

  async function unlockWithSecurityKey() {
    if (busy || oidcWaiting || keyWaiting || !keyPin) return;
    keyWaiting = true;
    error = "";
    try {
      await api.fido2Unlock(keyPin);
      keyPin = "";
      onunlocked();
    } catch (err) {
      error = errorMessage(err);
      if (errorCode(err) === "fido2_pin") keyPin = "";
    } finally {
      keyWaiting = false;
    }
  }

  function keyPinKeydown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      unlockWithSecurityKey();
    }
  }

  async function unlockWithProvider() {
    if (busy || oidcWaiting) return;
    oidcWaiting = true;
    error = "";
    try {
      await api.oidcUnlock();
      onunlocked();
    } catch (err) {
      if (errorCode(err) !== "cancelled") error = errorMessage(err);
    } finally {
      oidcWaiting = false;
    }
  }

  // Connexion depuis un téléphone : QR code vers la page de validation de l'instance,
  // puis attente de la validation (passkey sur le téléphone).
  async function unlockWithPhone() {
    if (busy || oidcWaiting || deviceStarting || device) return;
    deviceStarting = true;
    error = "";
    try {
      device = await api.oidcDeviceStart();
    } catch (err) {
      error = errorMessage(err);
      return;
    } finally {
      deviceStarting = false;
    }
    deviceLeft = device.expiresIn;
    deviceTimer = setInterval(() => (deviceLeft = Math.max(0, deviceLeft - 1)), 1000);
    try {
      await api.oidcDeviceWait();
      onunlocked();
    } catch (err) {
      if (errorCode(err) !== "cancelled") error = errorMessage(err);
    } finally {
      stopDevice();
    }
  }

  function stopDevice() {
    clearInterval(deviceTimer);
    device = null;
  }

  function cancelPhone() {
    void api.oidcCancel();
    stopDevice();
  }

  onDestroy(() => {
    clearInterval(deviceTimer);
    if (device) void api.oidcCancel();
  });

  const countdown = $derived(`${Math.floor(deviceLeft / 60)}:${String(deviceLeft % 60).padStart(2, "0")}`);

  // « votre compte Keycloak », ou « votre compte » pour un fournisseur sans modèle.
  const account = $derived(oidc && oidc.provider !== "generic" ? `votre compte ${oidc.providerName}` : "votre compte");

  const canSubmit = $derived(
    mode === "unlock"
      ? password.length > 0
      : password.length >= MIN_LENGTH && password === confirm && acknowledged,
  );

  async function submit(e: SubmitEvent) {
    e.preventDefault();
    if (!canSubmit || busy) return;
    busy = true;
    error = "";
    try {
      if (mode === "setup") await api.vaultCreate(password);
      else await api.vaultUnlock(password);
      password = confirm = "";
      onunlocked();
    } catch (err) {
      error = errorMessage(err);
      password = "";
    } finally {
      busy = false;
    }
  }
</script>

<main>
  <div class="glow"></div>
  <form class="card" onsubmit={submit}>
    <div class="head">
      <Logo size={56} />
      <h1>{mode === "setup" ? `Bienvenue dans ${APP_NAME}` : `${APP_NAME} est verrouillé`}</h1>
      <p>
        {#if mode === "setup"}
          Choisissez un mot de passe maître. Il chiffre vos sessions, identifiants et clés.
        {:else if device}
          Connexion depuis votre téléphone, avec {account}.
        {:else if reason === "idle"}
          Verrouillé après inactivité. Saisissez votre mot de passe maître.
        {:else}
          Saisissez votre mot de passe maître pour déverrouiller le coffre.
        {/if}
      </p>
    </div>

    <!-- Pendant la connexion par téléphone, seul le QR code reste affiché ; « Annuler »
         ramène le mot de passe et les autres moyens. -->
    {#if !device}
      <label class="field">
        <span>Mot de passe maître</span>
        <PasswordInput bind:value={password} autofocus placeholder={mode === "setup" ? `${MIN_LENGTH} caractères minimum` : ""} />
      </label>

      {#if mode === "setup"}
        <StrengthMeter {password} />
        <label class="field">
          <span>Confirmation</span>
          <PasswordInput bind:value={confirm} />
          {#if confirm && confirm !== password}<small class="error-text">Les mots de passe ne correspondent pas.</small>{/if}
        </label>
        <label class="ack">
          <input type="checkbox" bind:checked={acknowledged} />
          <span>
            J'ai compris qu'il n'existe <strong>aucun moyen de récupérer</strong> ce mot de passe : s'il est perdu, le
            coffre l'est aussi.
          </span>
        </label>
      {/if}

      {#if backgroundSessions}
        <p class="sessions-note">
          <Icon name="terminal" size={14} />
          {backgroundSessions} session{backgroundSessions > 1 ? "s restent ouvertes" : " reste ouverte"} en arrière-plan.
        </p>
      {/if}

      {#if error}<p class="error-text" role="alert">{error}</p>{/if}

      <button class="btn primary lg" type="submit" disabled={!canSubmit || busy || oidcWaiting || keyWaiting || !!device}>
        {#if busy}
          <span class="spinner"></span> {mode === "setup" ? "Création du coffre…" : "Déverrouillage…"}
        {:else}
          <Icon name={mode === "setup" ? "shield" : "lock"} size={17} />
          {mode === "setup" ? "Créer le coffre" : "Déverrouiller"}
        {/if}
      </button>

      {#if oidc || hasSecurityKey}
        <div class="or"><span>ou</span></div>
      {/if}

      {#if hasSecurityKey}
        {#if !keyOpen}
          <button type="button" class="btn lg alt" onclick={() => (keyOpen = true)} disabled={busy || oidcWaiting || !!device}>
            <Icon name="key" size={17} />
            Utiliser une clé de sécurité
          </button>
        {:else}
          <div class="key-panel">
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <label class="field" onkeydown={keyPinKeydown}>
              <span>Code PIN de la clé</span>
              <PasswordInput bind:value={keyPin} autofocus />
            </label>
            {#if keyWaiting}
              <div class="oidc-wait">
                <span class="spinner dark"></span>
                <div>
                  <strong>Touchez votre clé…</strong>
                  <span>Elle clignote pour confirmer le déverrouillage.</span>
                </div>
              </div>
            {/if}
            <div class="key-actions">
              <button type="button" class="btn ghost" onclick={() => ((keyOpen = false), (keyPin = ""))} disabled={keyWaiting}>
                Annuler
              </button>
              <button type="button" class="btn alt" onclick={unlockWithSecurityKey} disabled={!keyPin || keyWaiting || busy}>
                <Icon name="key" size={16} /> Déverrouiller avec la clé
              </button>
            </div>
          </div>
        {/if}
      {/if}

    {/if}

    {#if oidc}
      {#if device}
        <div class="device">
          <QrCode qr={device.qr} size={176} />
          <strong>Scannez ce QR code avec votre téléphone</strong>
          <span>Connectez-vous avec {account}, après avoir vérifié que le téléphone affiche ce code :</span>
          <code class="user-code">{device.userCode}</code>
          <span class="faint">Sans QR code : <span class="mono">{device.verificationUri}</span></span>
          <span class="faint">Expire dans {countdown}</span>
          <div class="device-wait">
            <span class="spinner dark"></span>
            <span>En attente de la validation…</span>
            <button type="button" class="btn ghost" onclick={cancelPhone}>Annuler</button>
          </div>
        </div>
      {:else if oidcWaiting}
        <div class="oidc-wait">
          <span class="spinner dark"></span>
          <div>
            <strong>Connexion en cours dans votre navigateur…</strong>
            <span>Connectez-vous avec {account}, puis revenez ici.</span>
          </div>
          <button type="button" class="btn ghost" onclick={() => api.oidcCancel()}>Annuler</button>
        </div>
      {:else}
        <button type="button" class="btn lg alt" onclick={unlockWithProvider} disabled={busy || keyWaiting || deviceStarting}>
          <Icon name="user" size={17} />
          Se connecter avec {oidc.providerName}
        </button>
        {#if oidc.phone}
          <button type="button" class="btn ghost phone" onclick={unlockWithPhone} disabled={busy || keyWaiting || deviceStarting}>
            {#if deviceStarting}<span class="spinner dark"></span>{:else}<Icon name="phone" size={15} />{/if}
            Avec mon téléphone (QR code)
          </button>
        {/if}
        <p class="oidc-account">{oidc.account}</p>
      {/if}
    {/if}

    <footer>
      <Icon name="shield" size={14} />
      Argon2id · XChaCha20-Poly1305 · chiffrement local
    </footer>
  </form>
</main>

<style>
  /* Défilement si la fenêtre est trop basse ; la marge automatique de la carte la
     centre sans couper le haut quand le contenu déborde. */
  main {
    position: relative;
    height: 100%;
    display: flex;
    padding: 24px 0;
    overflow-x: hidden;
    overflow-y: auto;
  }
  .glow {
    position: absolute;
    width: 720px;
    height: 720px;
    border-radius: 50%;
    background: var(--gradient);
    filter: blur(160px);
    opacity: 0.16;
    top: -260px;
    right: -200px;
    pointer-events: none;
  }
  .card {
    position: relative;
    margin: auto;
    width: 420px;
    max-width: calc(100vw - 32px);
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 36px 32px 24px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 22px;
    box-shadow: var(--shadow-lg);
  }
  .head {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    text-align: center;
    margin-bottom: 6px;
  }
  .head h1 {
    font-size: 21px;
    margin-top: 6px;
  }
  .head p {
    color: var(--text-muted);
    max-width: 330px;
  }
  .or {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--text-faint);
    font-size: 12px;
  }
  .or::before,
  .or::after {
    content: "";
    flex: 1;
    height: 1px;
    background: var(--border);
  }
  .alt {
    background: var(--surface-2);
  }
  .key-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
  }
  .key-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .phone {
    align-self: center;
    margin-top: -10px;
    font-size: 13px;
    color: var(--text-muted);
  }
  .device {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 18px 16px 12px;
    text-align: center;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12.5px;
  }
  .device > strong {
    margin-top: 6px;
    font-size: 13.5px;
  }
  .device > span {
    color: var(--text-muted);
  }
  .device .faint {
    color: var(--text-faint);
    font-size: 11.5px;
  }
  .device .faint .mono {
    word-break: break-all;
    user-select: text;
  }
  .user-code {
    padding: 6px 14px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    font-family: var(--font-mono);
    font-size: 20px;
    font-weight: 600;
    letter-spacing: 0.12em;
    user-select: text;
  }
  .device-wait {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 4px;
    color: var(--text-muted);
  }
  .oidc-account {
    margin-top: -10px;
    text-align: center;
    font-size: 12px;
    color: var(--text-faint);
  }
  .oidc-wait {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .oidc-wait div {
    flex: 1;
    display: flex;
    flex-direction: column;
    font-size: 12.5px;
  }
  .oidc-wait div span {
    color: var(--text-muted);
  }
  .spinner.dark {
    border-color: var(--border-strong);
    border-top-color: var(--accent);
    flex-shrink: 0;
  }
  .sessions-note {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .ack {
    display: flex;
    gap: 10px;
    font-size: 12.5px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .ack input {
    margin-top: 3px;
    accent-color: var(--accent);
  }
  footer {
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 6px;
    font-size: 11.5px;
    color: var(--text-faint);
  }
  .spinner {
    width: 15px;
    height: 15px;
    border-radius: 50%;
    border: 2px solid rgba(255, 255, 255, 0.35);
    border-top-color: #fff;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
