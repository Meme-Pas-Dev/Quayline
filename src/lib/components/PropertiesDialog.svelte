<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "../i18n/index.svelte";
  import { errorMessage, type FileEntry } from "../api";
  import { displayName, formatDate, formatSize } from "../format";
  import Modal from "./Modal.svelte";

  let {
    entry,
    load,
    canChmod,
    onchmod,
    onclose,
  }: {
    entry: FileEntry;
    /** Propriétés à jour (relues à l'ouverture). */
    load: () => Promise<FileEntry>;
    canChmod: boolean;
    onchmod: (mode: number) => Promise<void>;
    onclose: () => void;
  } = $props();

  // Valeurs de départ (la fenêtre est recréée pour chaque fichier), puis relues.
  const initial = untrack(() => entry);
  let current = $state<FileEntry>(initial);
  let mode = $state<number | null>(initial.mode);
  let octal = $state(toOctal(initial.mode));
  let saving = $state(false);
  let error = $state("");

  function toOctal(m: number | null) {
    return m === null ? "" : m.toString(8).padStart(3, "0");
  }

  onMount(() => {
    load()
      .then((fresh) => {
        current = fresh;
        mode = fresh.mode;
        octal = toOctal(fresh.mode);
      })
      .catch(() => {});
  });

  const WHO = ["owner.short", "group", "others"] as const;
  const WHAT = [
    ["read", 4],
    ["write", 2],
    ["exec", 1],
  ] as const;

  /** Bit de `value` (4 lecture, 2 écriture, 1 exécution) pour `who` (0 propriétaire, 1 groupe, 2 autres). */
  function bit(who: number, value: number) {
    return value << (3 * (2 - who));
  }

  function toggle(who: number, value: number) {
    if (mode === null) return;
    mode ^= bit(who, value);
    octal = toOctal(mode);
  }

  function onoctal() {
    if (/^[0-7]{3,4}$/.test(octal)) mode = parseInt(octal, 8);
  }

  const changed = $derived(mode !== null && mode !== current.mode);
  const octalValid = $derived(/^[0-7]{3,4}$/.test(octal));
  const kindLabel = $derived(
    current.kind === "dir" ? t("props.kind.dir") : current.kind === "link" ? t("props.kind.link") : t("props.kind.file"),
  );

  async function apply() {
    if (mode === null || !changed || saving) return;
    saving = true;
    error = "";
    try {
      await onchmod(mode);
      current = { ...current, mode };
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal title={t("props.title", { name: displayName(current.name) })} {onclose} width={480}>
  <dl class="props">
    <dt>{t("props.path")}</dt>
    <dd class="mono">{displayName(current.path)}</dd>
    <dt>{t("props.kind")}</dt>
    <dd>{kindLabel}</dd>
    {#if current.kind !== "dir"}
      <dt>{t("files.size")}</dt>
      <dd>{formatSize(current.size)}</dd>
    {/if}
    <dt>{t("files.modified")}</dt>
    <dd>{formatDate(current.modified) || "—"}</dd>
    {#if current.owner || current.group}
      <dt>{t("props.owner")}</dt>
      <dd class="mono">{current.owner ?? "—"} : {current.group ?? "—"}</dd>
    {/if}
  </dl>

  {#if mode !== null}
    <table class="perms">
      <thead>
        <tr>
          <th></th>
          {#each WHAT as [w]}<th>{t(`props.${w}`)}</th>{/each}
        </tr>
      </thead>
      <tbody>
        {#each WHO as who, i}
          <tr>
            <th>{t(`props.${who}`)}</th>
            {#each WHAT as [, value]}
              <td>
                <input
                  type="checkbox"
                  checked={(mode & bit(i, value)) !== 0}
                  disabled={!canChmod}
                  onchange={() => toggle(i, value)}
                />
              </td>
            {/each}
          </tr>
        {/each}
      </tbody>
    </table>
    <label class="field octal">
      <span>{t("props.octal")}</span>
      <input class="input mono" bind:value={octal} oninput={onoctal} disabled={!canChmod} maxlength="4" spellcheck="false" />
      <span class="mono faint">{current.permissions ?? ""}</span>
    </label>
    {#if !octalValid}<small class="error-text">{t("props.octalInvalid")}</small>{/if}
  {:else}
    <p class="muted">{t("props.noPermissions")}</p>
  {/if}
  {#if error}<p class="error-text">{error}</p>{/if}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t("common.close")}</button>
    {#if canChmod && mode !== null}
      <button class="btn primary" onclick={apply} disabled={!changed || !octalValid || saving}>
        {t(saving ? "props.applying" : "props.apply")}
      </button>
    {/if}
  {/snippet}
</Modal>

<style>
  .props {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 6px 14px;
    margin: 0 0 14px;
    font-size: 13px;
  }
  dt {
    color: var(--text-faint);
  }
  dd {
    margin: 0;
    word-break: break-all;
  }
  .perms {
    border-collapse: collapse;
    font-size: 13px;
    margin-bottom: 10px;
  }
  .perms th,
  .perms td {
    padding: 4px 12px 4px 0;
    text-align: center;
  }
  .perms tbody th {
    text-align: left;
    font-weight: 500;
  }
  .perms thead th {
    font-weight: 500;
    color: var(--text-faint);
  }
  .octal {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .octal input {
    width: 80px;
  }
</style>
