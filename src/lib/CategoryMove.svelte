<script lang="ts">
  import { t } from "./i18n.svelte";
  import { api } from "./api";
  import { store } from "./store.svelte";
  import type { Category } from "./types";

  /** A "Move to…" menu listing every category the given one can go inside, and the top level. */
  let { category }: { category: Category } = $props();
  /** It can't go inside itself or one inside it. */
  const blocked = $derived(store.categoryAndInside(category.id));

  function move(e: Event & { currentTarget: HTMLSelectElement }) {
    const v = e.currentTarget.value;
    e.currentTarget.value = "";
    if (!v) return;
    const parentId = v === "top" ? null : Number(v);
    if (parentId !== category.parentId) store.mutate(() => api.moveCategory(category.id, parentId));
  }
</script>

<select class="move" title={t("Move to…")} value="" onchange={move} onclick={(e) => e.stopPropagation()}>
  <option value="" disabled>{t("Move to…")}</option>
  <option value="top" disabled={category.parentId == null}>{t("Top level")}</option>
  {#each store.categoryTree() as { category: c, depth } (c.id)}
    <option value={c.id} disabled={blocked.has(c.id) || c.id === category.parentId}>{"  ".repeat(depth)}{c.name}</option>
  {/each}
</select>

<style>
  .move { height: 26px; max-width: 160px; font-size: 12px; }
</style>
