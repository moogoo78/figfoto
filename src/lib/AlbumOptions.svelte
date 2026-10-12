<script lang="ts">
  import { t } from "./i18n.svelte";
  import { store } from "./store.svelte";

  /** `<option>`s for an album `<select>`, grouped by topic. */
  let { stringIds = false }: { stringIds?: boolean } = $props();
  const groups = $derived(store.albumsByTopic(true));
  const hasTopics = $derived(groups.some((g) => g.topic));
</script>

{#each groups as g (g.topic?.id ?? 0)}
  {#if hasTopics}
    <optgroup label={g.topic?.name ?? t("No topic")}>
      {#each g.albums as a (a.id)}<option value={stringIds ? String(a.id) : a.id}>{a.name}</option>{/each}
    </optgroup>
  {:else}
    {#each g.albums as a (a.id)}<option value={stringIds ? String(a.id) : a.id}>{a.name}</option>{/each}
  {/if}
{/each}
