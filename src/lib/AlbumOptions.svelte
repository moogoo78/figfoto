<script lang="ts">
  import { t } from "./i18n.svelte";
  import { store } from "./store.svelte";

  /** `<option>`s for an album `<select>`, grouped by category (labelled with its full path). */
  let { stringIds = false }: { stringIds?: boolean } = $props();
  const groups = $derived(store.albumsByCategory(true));
  const hasCategories = $derived(groups.some((g) => g.category));
</script>

{#each groups as g (g.category?.id ?? 0)}
  {#if hasCategories}
    <optgroup label={g.category ? store.categoryLabel(g.category.id) : t("No category")}>
      {#each g.albums as a (a.id)}<option value={stringIds ? String(a.id) : a.id}>{a.name}</option>{/each}
    </optgroup>
  {:else}
    {#each g.albums as a (a.id)}<option value={stringIds ? String(a.id) : a.id}>{a.name}</option>{/each}
  {/if}
{/each}
