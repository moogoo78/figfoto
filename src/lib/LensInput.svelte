<script lang="ts">
  import { t } from "./i18n.svelte";
  import { store } from "./store.svelte";

  /** Sets the lens by hand, e.g. for a manual lens that records none. Empty clears it. */
  let { onset }: { onset: (lens: string) => void } = $props();
  let value = $state("");
  const listId = `lenses-${Math.random().toString(36).slice(2)}`;
</script>

<input
  bind:value
  list={listId}
  placeholder={t("Lens (manual lenses)")}
  title={t("Set the lens for photos taken with a manual lens. Enter nothing to go back to what the camera recorded.")}
  onkeydown={(e) => {
    if (e.key === "Enter") {
      onset(value.trim());
      value = "";
    }
    if (e.key === "Escape") e.currentTarget.blur();
    e.stopPropagation();
  }} />
<datalist id={listId}>
  {#each store.lenses as lens}<option value={lens}></option>{/each}
</datalist>

<style>
  input { width: 180px; }
</style>
