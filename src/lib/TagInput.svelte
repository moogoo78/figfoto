<script lang="ts">
  import { t } from "./i18n.svelte";
  import { store } from "./store.svelte";

  let { onadd, placeholder }: { onadd: (names: string[]) => void; placeholder?: string } = $props();
  let value = $state("");
  let input: HTMLInputElement;
  const listId = `tags-${Math.random().toString(36).slice(2)}`;

  export function focus() {
    input?.focus();
  }

  function submit() {
    const names = value.split(",").map((s) => s.trim()).filter(Boolean);
    if (names.length) onadd(names);
    value = "";
  }
</script>

<input
  bind:this={input}
  bind:value
  list={listId}
  placeholder={placeholder ?? t("Add tag…")}
  onkeydown={(e) => {
    if (e.key === "Enter") submit();
    if (e.key === "Escape") input.blur();
    e.stopPropagation();
  }} />
<datalist id={listId}>
  {#each store.tags as tag (tag.id)}<option value={tag.name}></option>{/each}
</datalist>

<style>
  input { width: 180px; }
</style>
