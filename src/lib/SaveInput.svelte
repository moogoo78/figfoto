<script lang="ts">
  import { flash } from "./flash";
  import { t } from "./i18n.svelte";

  /** A one-line field with suggestions and a Save button; Enter saves too. Saving an empty
   * field (with Enter) clears the value. */
  let {
    onset,
    suggestions,
    placeholder,
    title,
    disabled = false,
    width = 180,
  }: {
    onset: (text: string) => void;
    suggestions: string[];
    placeholder: string;
    title: string;
    disabled?: boolean;
    width?: number;
  } = $props();

  let value = $state("");
  let input: HTMLInputElement | undefined = $state();
  let saved = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const listId = `suggest-${Math.random().toString(36).slice(2)}`;

  function save() {
    onset(value.trim());
    value = "";
    flash(input);
    saved = true;
    clearTimeout(timer);
    timer = setTimeout(() => (saved = false), 1200);
  }
</script>

<span class="field">
  <input
    bind:this={input}
    bind:value
    list={listId}
    style:width="{width}px"
    {disabled}
    {placeholder}
    {title}
    onkeydown={(e) => {
      if (e.key === "Enter") save();
      if (e.key === "Escape") e.currentTarget.blur();
      e.stopPropagation();
    }} />
  <button class:saved disabled={disabled || (!value.trim() && !saved)} onclick={save}>
    {saved ? `✓ ${t("Saved")}` : t("Save")}
  </button>
</span>
<datalist id={listId}>
  {#each suggestions as s}<option value={s}></option>{/each}
</datalist>

<style>
  .field { display: inline-flex; gap: 4px; }
  button { padding: 0 10px; height: var(--control-h); white-space: nowrap; }
  button.saved { color: var(--ok); border-color: var(--ok); opacity: 1; }
</style>
