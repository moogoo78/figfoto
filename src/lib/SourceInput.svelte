<script lang="ts">
  import { t } from "./i18n.svelte";
  import { SOURCE_SUGGESTIONS } from "./format";
  import { store } from "./store.svelte";

  let {
    onset,
    value = $bindable(""),
    placeholder,
    submitOnEnter = true,
  }: { onset?: (source: string) => void; value?: string; placeholder?: string; submitOnEnter?: boolean } = $props();
  const listId = `sources-${Math.random().toString(36).slice(2)}`;
  const suggestions = $derived([...new Set([...store.sources, ...SOURCE_SUGGESTIONS])]);
</script>

<input
  bind:value
  list={listId}
  placeholder={placeholder ?? t("Source (LINE, Facebook…)")}
  onkeydown={(e) => {
    if (e.key === "Enter" && submitOnEnter && onset) {
      onset(value.trim());
      value = "";
    }
    if (e.key === "Escape") e.currentTarget.blur();
    e.stopPropagation();
  }} />
<datalist id={listId}>
  {#each suggestions as s}<option value={s}></option>{/each}
</datalist>

<style>
  input { width: 180px; }
</style>
