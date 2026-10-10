<script lang="ts">
  import { api, type SessionEvent } from "../api";
  import type { SessionTab } from "../store.svelte";
  import Icon from "./Icon.svelte";
  import Modal from "./Modal.svelte";
  import PasswordInput from "./PasswordInput.svelte";

  // Demandes envoyées par le backend pendant une connexion (terminal ou SFTP) :
  // approbation de la clé d'hôte et saisies (utilisateur, mot de passe, 2FA…).
  let {
    tab,
    visible,
    active = $bindable(false),
    onanswered,
  }: { tab: SessionTab; visible: boolean; active?: boolean; onanswered?: () => void } = $props();

  type HostKeyPrompt = Extract<SessionEvent, { type: "hostKey" }>;
  type InputPrompt = Extract<SessionEvent, { type: "input" }>;
  let hostKeyPrompt = $state<HostKeyPrompt | null>(null);
  let inputPrompt = $state<InputPrompt | null>(null);
  let inputValues = $state<string[]>([]);
  let acknowledgeChange = $state(false);

  $effect(() => {
    active = !!(hostKeyPrompt || inputPrompt);
    tab.pending = active;
  });

  /** Prend en charge l'événement s'il s'agit d'une demande ; renvoie `true` dans ce cas. */
  export function handle(e: SessionEvent): boolean {
    if (e.type === "hostKey") {
      acknowledgeChange = false;
      hostKeyPrompt = e;
      return true;
    }
    if (e.type === "input") {
      inputValues = e.fields.map(() => "");
      inputPrompt = e;
      return true;
    }
    return false;
  }

  /** Connexion terminée : les demandes en cours n'ont plus d'objet. */
  export function reset() {
    hostKeyPrompt = null;
    inputPrompt = null;
  }

  /** Composant détruit : refuser les demandes en attente pour débloquer le backend. */
  export function cancelPending() {
    if (hostKeyPrompt) api.sshPromptAnswer(hostKeyPrompt.promptId, null);
    if (inputPrompt) api.sshPromptAnswer(inputPrompt.promptId, null);
    reset();
  }

  function answerHostKey(accept: boolean) {
    if (!hostKeyPrompt) return;
    // Le backend exige cette réponse précise pour remplacer une clé qui a changé.
    const approval = hostKeyPrompt.changed ? ["replace-host-key"] : [];
    api.sshPromptAnswer(hostKeyPrompt.promptId, accept ? approval : null);
    hostKeyPrompt = null;
    onanswered?.();
  }

  function answerInput(values: string[] | null) {
    if (!inputPrompt) return;
    api.sshPromptAnswer(inputPrompt.promptId, values);
    inputPrompt = null;
    inputValues = [];
    onanswered?.();
  }
</script>

{#if visible && hostKeyPrompt}
  <Modal
    title={hostKeyPrompt.changed ? "La clé du serveur a changé" : "Nouveau serveur"}
    onclose={() => answerHostKey(false)}
    width={520}
  >
    {#if hostKeyPrompt.changed}
      <div class="alert">
        <Icon name="alert" size={20} />
        <p>
          La clé présentée par <strong>{hostKeyPrompt.hostname}:{hostKeyPrompt.port}</strong> ne correspond pas à celle
          enregistrée. Cela peut indiquer une <strong>attaque de l'homme du milieu</strong>, ou une réinstallation du
          serveur. N'acceptez que si vous savez pourquoi elle a changé.
        </p>
      </div>
      <div class="fp-block">
        <span>Ancienne empreinte</span>
        <code>{hostKeyPrompt.previousFingerprint}</code>
        <span>Nouvelle empreinte ({hostKeyPrompt.algorithm})</span>
        <code class="new">{hostKeyPrompt.fingerprint}</code>
      </div>
      <label class="ack">
        <input type="checkbox" bind:checked={acknowledgeChange} />
        Je sais pourquoi la clé a changé et j'ai vérifié la nouvelle empreinte.
      </label>
    {:else}
      <p class="muted">
        Première connexion à <strong>{hostKeyPrompt.hostname}:{hostKeyPrompt.port}</strong>. Vérifiez que l'empreinte
        correspond à celle du serveur (<code>ssh-keygen -lf /etc/ssh/ssh_host_*_key.pub</code>) avant de l'approuver.
      </p>
      <div class="fp-block">
        <span>{hostKeyPrompt.algorithm}</span>
        <code>{hostKeyPrompt.fingerprint}</code>
      </div>
    {/if}
    {#snippet footer()}
      {#if hostKeyPrompt?.changed}
        <button class="btn danger" disabled={!acknowledgeChange} onclick={() => answerHostKey(true)}>
          Remplacer la clé
        </button>
        <button class="btn primary" onclick={() => answerHostKey(false)}>Interrompre</button>
      {:else}
        <button class="btn" onclick={() => answerHostKey(false)}>Annuler</button>
        <button class="btn primary" onclick={() => answerHostKey(true)}>
          <Icon name="shield" size={16} /> Approuver et se connecter
        </button>
      {/if}
    {/snippet}
  </Modal>
{/if}

{#if visible && inputPrompt}
  <Modal
    title={inputPrompt.fromServer ? "Le serveur demande une réponse" : inputPrompt.title}
    onclose={() => answerInput(null)}
    width={460}
  >
    <form
      id="ssh-input"
      class="inputs"
      onsubmit={(e) => {
        e.preventDefault();
        answerInput([...inputValues]);
      }}
    >
      {#if inputPrompt.fromServer}
        <div class="server-note">
          <Icon name="server" size={16} />
          <span>
            Questions envoyées par <strong>{inputPrompt.fromServer}</strong>, pas par l'application. Ne saisissez
            jamais votre mot de passe maître ici.
          </span>
        </div>
        {#if inputPrompt.title !== "Authentification"}<p class="server-title">« {inputPrompt.title} »</p>{/if}
      {/if}
      {#if inputPrompt.message}<p class="muted server-text">{inputPrompt.message}</p>{/if}
      {#each inputPrompt.fields as field, i}
        <label class="field">
          <span>{field.label}</span>
          {#if field.secret}
            <PasswordInput bind:value={inputValues[i]} autofocus={i === 0} />
          {:else}
            <!-- svelte-ignore a11y_autofocus -->
            <input class="input" bind:value={inputValues[i]} autofocus={i === 0} spellcheck="false" autocomplete="off" />
          {/if}
        </label>
      {/each}
    </form>
    {#snippet footer()}
      <button class="btn" onclick={() => answerInput(null)}>Annuler</button>
      <button class="btn primary" type="submit" form="ssh-input">Valider</button>
    {/snippet}
  </Modal>
{/if}

<style>
  .muted {
    color: var(--text-muted);
  }
  .server-note {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 10px 12px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
    font-size: 12.5px;
    color: var(--text-muted);
  }
  .server-note :global(svg) {
    flex-shrink: 0;
    color: var(--warning);
  }
  .server-title {
    font-weight: 600;
  }
  .server-text {
    white-space: pre-line;
  }
  .alert {
    display: flex;
    gap: 12px;
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--danger-soft);
    color: var(--text);
  }
  .alert :global(svg) {
    flex-shrink: 0;
    color: var(--danger);
    margin-top: 2px;
  }
  .fp-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 14px;
    border-radius: var(--radius);
    background: var(--surface-2);
    border: 1px solid var(--border);
  }
  .fp-block span {
    font-size: 12px;
    color: var(--text-muted);
  }
  .fp-block code {
    font-size: 12.5px;
    word-break: break-all;
    user-select: text;
    margin-bottom: 4px;
  }
  .fp-block code.new {
    color: var(--danger);
  }
  .ack {
    display: flex;
    gap: 10px;
    font-size: 13px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .ack input {
    accent-color: var(--danger);
  }
  .inputs {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
</style>
