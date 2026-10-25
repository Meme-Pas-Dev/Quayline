<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import { api, type Host, type HostInput, type HostKind } from "../api";
  import Icon from "./Icon.svelte";
  import IdentityForm from "./IdentityForm.svelte";
  import { data, handleError, reload } from "../store.svelte";
  import Modal from "./Modal.svelte";
  import { toast } from "./toast.svelte";

  // `copy` : formulaire pré-rempli à partir de `host`, enregistré comme un nouvel hôte.
  let { host, onclose, copy = false }: { host: Host | null; onclose: () => void; copy?: boolean } = $props();

  const COLORS = ["#7c8cff", "#38d6c4", "#3ddc97", "#f7b955", "#ff8a5c", "#ff6b81", "#c084fc"];

  // Copie locale du formulaire, initialisée une seule fois à l'ouverture.
  // svelte-ignore state_referenced_locally
  let form = $state({
    kind: (host?.kind ?? "ssh") as HostKind,
    label: host ? (copy ? t("hostForm.copySuffix", { name: host.label }) : host.label) : "",
    hostname: host?.hostname ?? "",
    port: host?.port ?? 22,
    username: host?.username ?? "",
    group: host?.group ?? "",
    identityId: host?.identityId ?? "",
    color: host?.color ?? COLORS[0],
    tags: host?.tags.join(", ") ?? "",
    notes: host?.notes ?? "",
  });
  let busy = $state(false);

  // « Ajouter une identité » dans la liste : formulaire d'identité par-dessus celui-ci,
  // puis la nouvelle identité est choisie. La liste garde l'ancien choix entre-temps.
  const NEW_IDENTITY = "__new__";
  let creatingIdentity = $state(false);
  function onidentitychange(e: Event & { currentTarget: HTMLSelectElement }) {
    if (e.currentTarget.value === NEW_IDENTITY) {
      e.currentTarget.value = form.identityId;
      creatingIdentity = true;
    } else {
      form.identityId = e.currentTarget.value;
    }
  }

  const groups = $derived([...new Set(data.hosts.map((h) => h.group).filter(Boolean))] as string[]);

  async function save(e: SubmitEvent) {
    e.preventDefault();
    busy = true;
    const input: HostInput = {
      id: copy ? null : (host?.id ?? null),
      kind: form.kind,
      label: form.label || form.hostname,
      hostname: form.hostname,
      port: Number(form.port),
      username: form.username || null,
      group: form.group || null,
      identityId: form.identityId || null,
      color: form.color,
      tags: form.tags.split(",").map((t) => t.trim()).filter(Boolean),
      notes: form.notes,
    };
    try {
      await api.hostSave(input);
      await reload();
      toast(t(host && !copy ? "hostForm.updated" : "hostForm.added"), "success");
      onclose();
    } catch (err) {
      handleError(err);
    } finally {
      busy = false;
    }
  }
</script>

<Modal title={t(copy ? "hostForm.titleDuplicate" : host ? "hostForm.titleEdit" : "hostForm.titleNew")} {onclose} width={560}>
  <form id="host-form" class="grid" onsubmit={save}>
    <div class="field span3">
      <span>{t("hostForm.kind")}</span>
      <div class="kinds" role="radiogroup" aria-label={t("hostForm.kind")}>
        <button
          type="button"
          role="radio"
          aria-checked={form.kind === "ssh"}
          class="kind"
          class:selected={form.kind === "ssh"}
          onclick={() => (form.kind = "ssh")}
        >
          <Icon name="terminal" size={18} />
          <span><strong>SSH</strong><small>{t("hostForm.kindSsh")}</small></span>
        </button>
        <button
          type="button"
          role="radio"
          aria-checked={form.kind === "sftp"}
          class="kind"
          class:selected={form.kind === "sftp"}
          onclick={() => (form.kind = "sftp")}
        >
          <Icon name="folder" size={18} />
          <span><strong>SFTP</strong><small>{t("hostForm.kindSftp")}</small></span>
        </button>
      </div>
      <small>{t("hostForm.kindHint")}</small>
    </div>

    <label class="field span2">
      <span>{t("hostForm.address")}</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input class="input" bind:value={form.hostname} placeholder={t("hostForm.addressPlaceholder")} required autofocus spellcheck="false" />
    </label>
    <label class="field">
      <span>{t("hostForm.port")}</span>
      <input class="input" type="number" min="1" max="65535" bind:value={form.port} required />
    </label>

    <label class="field span2">
      <span>{t("hostForm.name")}</span>
      <input class="input" bind:value={form.label} placeholder={form.hostname || t("hostForm.namePlaceholder")} />
    </label>
    <label class="field">
      <span>{t("hostForm.group")}</span>
      <input class="input" bind:value={form.group} list="host-groups" placeholder={t("hostForm.groupPlaceholder")} />
      <datalist id="host-groups">
        {#each groups as g}<option value={g}></option>{/each}
      </datalist>
    </label>

    <label class="field">
      <span>{t("hostForm.user")}</span>
      <input class="input" bind:value={form.username} placeholder={t("hostForm.userPlaceholder")} spellcheck="false" />
    </label>
    <label class="field span2">
      <span>{t("hostForm.identity")}</span>
      <select class="input" value={form.identityId} onchange={onidentitychange}>
        <option value="">{t("hostForm.identityNone")}</option>
        {#each data.identities as i}<option value={i.id}>{i.label}</option>{/each}
        <option value={NEW_IDENTITY}>{t("hostForm.identityAdd")}</option>
      </select>
    </label>

    <label class="field span3">
      <span>{t("hostForm.tags")}</span>
      <input class="input" bind:value={form.tags} placeholder="prod, web, debian" />
    </label>

    <div class="field span3">
      <span>{t("hostForm.color")}</span>
      <div class="colors">
        {#each COLORS as c}
          <button
            type="button"
            class="swatch"
            class:selected={form.color === c}
            style:background={c}
            onclick={() => (form.color = c)}
            aria-label={t("hostForm.colorOption", { color: c })}
          ></button>
        {/each}
      </div>
    </div>

    <label class="field span3">
      <span>{t("hostForm.notes")}</span>
      <textarea class="input" bind:value={form.notes} rows="3"></textarea>
    </label>
  </form>

  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t("common.cancel")}</button>
    <button class="btn primary" type="submit" form="host-form" disabled={busy || !form.hostname}>{t("common.save")}</button>
  {/snippet}
</Modal>

{#if creatingIdentity}
  <IdentityForm identity={null} onclose={() => (creatingIdentity = false)} onsaved={(saved) => (form.identityId = saved.id)} />
{/if}

<style>
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 14px;
  }
  .span2 {
    grid-column: span 2;
  }
  .span3 {
    grid-column: span 3;
  }
  .colors {
    display: flex;
    gap: 10px;
  }
  .kinds {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .kind {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-muted);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .kind span {
    display: flex;
    flex-direction: column;
  }
  .kind strong {
    color: var(--text);
    font-size: 13.5px;
  }
  .kind small {
    font-size: 12px;
  }
  .kind.selected {
    border-color: var(--accent);
    box-shadow: var(--ring);
    color: var(--accent);
  }
  .swatch {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 2px solid transparent;
    box-shadow: inset 0 0 0 2px rgba(0, 0, 0, 0.15);
  }
  .swatch:hover {
    box-shadow: inset 0 0 0 2px rgba(0, 0, 0, 0.15), 0 0 0 3px var(--accent-soft);
  }
  .swatch.selected {
    border-color: var(--text);
  }
</style>
