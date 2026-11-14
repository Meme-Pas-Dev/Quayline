<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { t } from "../i18n/index.svelte";
  import { errorMessage, type FileEntry } from "../api";
  import { displayName, formatDate, formatSize } from "../format";
  import Modal from "./Modal.svelte";

  let {
    entries,
    load,
    canChmod,
    onchmod,
    onclose,
  }: {
    /** Un ou plusieurs éléments : à plusieurs, seules les permissions sont modifiables. */
    entries: FileEntry[];
    /** Propriétés à jour (relues à l'ouverture), dans le même ordre. */
    load: () => Promise<FileEntry[]>;
    canChmod: boolean;
    onchmod: (changes: { entry: FileEntry; mode: number }[]) => Promise<void>;
    onclose: () => void;
  } = $props();

  // Valeurs de départ (la fenêtre est recréée pour chaque sélection), puis relues.
  let current = $state<FileEntry[]>(untrack(() => entries));
  const single = $derived(current.length === 1 ? current[0] : null);
  const withMode = $derived(current.filter((e) => e.mode !== null));

  // Modification demandée : bits à ajouter (`set`) et à retirer (`clear`) sur chaque
  // élément ; les autres bits de chacun restent tels quels (sélection aux droits mêlés).
  let set = $state(0);
  let clear = $state(0);
  const results = $derived(withMode.map((e) => (e.mode! & ~clear) | set));
  const uniform = $derived(results.length && results.every((r) => r === results[0]) ? results[0] : null);
  let octal = $state("");
  let saving = $state(false);
  let error = $state("");

  function toOctal(m: number | null) {
    return m === null ? "" : m.toString(8).padStart(3, "0");
  }

  // Champ octal : valeur commune, vide si les éléments diffèrent.
  function syncOctal() {
    octal = toOctal(uniform);
  }
  syncOctal();

  onMount(() => {
    load()
      .then((fresh) => {
        current = fresh;
        syncOctal();
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

  function stateOf(b: number): "on" | "off" | "mixed" {
    const on = results.filter((r) => r & b).length;
    return on === results.length ? "on" : on === 0 ? "off" : "mixed";
  }

  function toggle(b: number) {
    if (stateOf(b) === "on") {
      clear |= b;
      set &= ~b;
    } else {
      set |= b;
      clear &= ~b;
    }
    syncOctal();
  }

  function onoctal() {
    if (!/^[0-7]{3,4}$/.test(octal)) return;
    const value = parseInt(octal, 8);
    set = value;
    clear = ~value & 0o7777;
  }

  /** Case à trois états : cochée, décochée, ou mêlée (`indeterminate`). */
  function mixed(el: HTMLInputElement, value: boolean) {
    el.indeterminate = value;
    return { update: (v: boolean) => (el.indeterminate = v) };
  }

  const changes = $derived(withMode.map((entry, i) => ({ entry, mode: results[i] })).filter((c) => c.mode !== c.entry.mode));
  const octalValid = $derived(octal === "" ? uniform === null : /^[0-7]{3,4}$/.test(octal));
  const kindLabel = $derived(
    single?.kind === "dir" ? t("props.kind.dir") : single?.kind === "link" ? t("props.kind.link") : t("props.kind.file"),
  );
  const title = $derived(
    single ? t("props.title", { name: displayName(single.name) }) : t("props.titleMany", { count: current.length }),
  );

  async function apply() {
    if (!changes.length || saving) return;
    saving = true;
    error = "";
    try {
      await onchmod(changes);
      const applied = new Map(changes.map((c) => [c.entry.path, c.mode]));
      current = current.map((e) => (applied.has(e.path) ? { ...e, mode: applied.get(e.path)! } : e));
      set = clear = 0;
      syncOctal();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      saving = false;
    }
  }
</script>

<Modal {title} {onclose} width={480}>
  {#if single}
    <dl class="props">
      <dt>{t("props.path")}</dt>
      <dd class="mono">{displayName(single.path)}</dd>
      <dt>{t("props.kind")}</dt>
      <dd>{kindLabel}</dd>
      {#if single.kind !== "dir"}
        <dt>{t("files.size")}</dt>
        <dd>{formatSize(single.size)}</dd>
      {/if}
      <dt>{t("files.modified")}</dt>
      <dd>{formatDate(single.modified) || "—"}</dd>
      {#if single.owner || single.group}
        <dt>{t("props.owner")}</dt>
        <dd class="mono">{single.owner ?? "—"} : {single.group ?? "—"}</dd>
      {/if}
    </dl>
  {:else}
    <ul class="names">
      {#each current.slice(0, 6) as e (e.path)}<li class="mono">{displayName(e.name)}</li>{/each}
      {#if current.length > 6}<li class="faint">{t("props.andMore", { count: current.length - 6 })}</li>{/if}
    </ul>
    {#if withMode.length > 1}<p class="muted hint">{t("props.mixedHint")}</p>{/if}
  {/if}

  {#if withMode.length}
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
              {@const b = bit(i, value)}
              {@const st = stateOf(b)}
              <td>
                <input
                  type="checkbox"
                  checked={st === "on"}
                  use:mixed={st === "mixed"}
                  aria-checked={st === "mixed" ? "mixed" : st === "on"}
                  disabled={!canChmod}
                  onchange={() => toggle(b)}
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
      <span class="mono faint">{single?.permissions ?? ""}</span>
    </label>
    {#if !octalValid}<small class="error-text">{t("props.octalInvalid")}</small>{/if}
  {:else}
    <p class="muted">{t("props.noPermissions")}</p>
  {/if}
  {#if error}<p class="error-text">{error}</p>{/if}

  {#snippet footer()}
    <button class="btn" onclick={onclose}>{t("common.close")}</button>
    {#if canChmod && withMode.length}
      <button class="btn primary" onclick={apply} disabled={!changes.length || !octalValid || saving}>
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
  .names {
    margin: 0 0 10px;
    padding: 0;
    list-style: none;
    font-size: 13px;
  }
  .names li {
    padding: 1px 0;
    word-break: break-all;
  }
  .hint {
    font-size: 12.5px;
    margin: 0 0 12px;
  }
</style>
