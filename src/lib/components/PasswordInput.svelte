<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "./Icon.svelte";

  let {
    value = $bindable(""),
    placeholder = "",
    autofocus = false,
    id,
    mono = false,
  }: { value?: string; placeholder?: string; autofocus?: boolean; id?: string; mono?: boolean } = $props();

  let visible = $state(false);
  let input: HTMLInputElement;

  onMount(() => {
    if (autofocus) requestAnimationFrame(() => input.focus());
  });
</script>

<div class="wrap">
  <!-- svelte-ignore a11y_autofocus -->
  <input
    bind:this={input}
    {id}
    class="input"
    class:mono
    type={visible ? "text" : "password"}
    bind:value
    {placeholder}
    {autofocus}
    autocomplete="off"
    autocapitalize="off"
    spellcheck="false"
  />
  <button
    type="button"
    class="toggle"
    onclick={() => (visible = !visible)}
    aria-label={visible ? "Masquer" : "Afficher"}
    tabindex="-1"
  >
    <Icon name={visible ? "eyeOff" : "eye"} size={16} />
  </button>
</div>

<style>
  .wrap {
    position: relative;
  }
  .wrap .input {
    padding-right: 40px;
  }
  .toggle {
    position: absolute;
    top: 50%;
    right: 6px;
    transform: translateY(-50%);
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-faint);
  }
  .toggle:hover {
    color: var(--text);
    background: var(--surface-hover);
  }
</style>
