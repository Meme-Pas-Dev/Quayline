<script lang="ts" module>
  // Pile des modales ouvertes : seule la plus haute réagit au clavier.
  const stack: symbol[] = [];
</script>

<script lang="ts">
  import { t } from "../i18n/index.svelte";
  import { onMount, type Snippet } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    title,
    onclose,
    width = 520,
    children,
    footer,
  }: { title: string; onclose: () => void; width?: number; children: Snippet; footer?: Snippet } = $props();

  const id = Symbol();
  let dialog: HTMLDivElement;

  const FOCUSABLE = "input:not([type=hidden]):not([disabled]), select, textarea, button:not([disabled]), [href], [tabindex]:not([tabindex='-1'])";

  function focusables(): HTMLElement[] {
    return [...dialog.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => !el.hidden && el.offsetParent !== null);
  }

  onMount(() => {
    stack.push(id);
    const previous = document.activeElement as HTMLElement | null;
    // L'attribut autofocus n'agit qu'au chargement de la page : on le gère ici.
    requestAnimationFrame(() => {
      const target = dialog.querySelector<HTMLElement>("[autofocus]") ?? focusables().find((el) => el.tagName !== "BUTTON") ?? focusables()[0];
      target?.focus();
    });
    return () => {
      stack.splice(stack.indexOf(id), 1);
      previous?.focus?.();
    };
  });

  function onkeydown(e: KeyboardEvent) {
    if (stack[stack.length - 1] !== id) return;
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    } else if (e.key === "Tab") {
      // Piège à focus : Tab ne sort jamais de la modale.
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const inside = dialog.contains(document.activeElement);
      if (e.shiftKey && (document.activeElement === first || !inside)) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && (document.activeElement === last || !inside)) {
        e.preventDefault();
        first.focus();
      }
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="backdrop">
  <div
    bind:this={dialog}
    class="modal"
    style:width="{width}px"
    role="dialog"
    aria-modal="true"
    aria-label={title}
  >
    <header>
      <h2>{title}</h2>
      <button class="btn ghost icon" onclick={onclose} aria-label={t("common.close")}><Icon name="x" /></button>
    </header>
    <div class="body">{@render children()}</div>
    {#if footer}<footer>{@render footer()}</footer>{/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    /* Ni flou ni animation : sous WebKitGTK, la création/destruction de calques
       de composition provoque un clignotement à l'ouverture. */
    background: var(--overlay);
  }
  .modal {
    max-width: calc(100vw - 32px);
    max-height: calc(100vh - 48px);
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 18px 20px 8px 24px;
  }
  h2 {
    font-size: 17px;
  }
  .body {
    padding: 12px 24px 20px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 14px 24px 20px;
    border-top: 1px solid var(--border);
  }
</style>
