<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { store } from "../lib/store.svelte";

  const q = store.query;
  const tagName = (id: number) => store.tags.find((tag) => tag.id === id)?.name ?? "?";
</script>

<div class="filters">
  <label>{t("From")} <input type="date" bind:value={q.dateFrom} /></label>
  <label>{t("To")} <input type="date" bind:value={q.dateTo} /></label>
  <label>
    {t("Rating")}
    <select bind:value={q.minRating}>
      <option value={0}>{t("Any")}</option>
      {#each [1, 2, 3, 4, 5] as n}<option value={n}>{"★".repeat(n)}+</option>{/each}
    </select>
  </label>
  {#if store.cameras.length}
    <label>
      {t("Camera")}
      <select bind:value={q.camera}>
        <option value="">{t("Any")}</option>
        {#each store.cameras as c}<option value={c}>{c}</option>{/each}
      </select>
    </label>
  {/if}
  <label>
    {t("Source")}
    <select bind:value={q.origin}>
      <option value="">{t("Any")}</option>
      <option value="camera">{t("Camera")}</option>
      <option value="phone">{t("Phone")}</option>
      <option value="unknown">{t("Unknown")}</option>
      {#each store.sources as s}<option value={s}>{s}</option>{/each}
    </select>
  </label>
  <label>
    {t("Tags")}
    <select
      onchange={(e) => {
        const id = Number(e.currentTarget.value);
        e.currentTarget.value = "";
        if (id && !q.tagIds.includes(id)) q.tagIds.push(id);
      }}>
      <option value="">{t("Add…")}</option>
      {#each store.tags.filter((tag) => !q.tagIds.includes(tag.id)) as tag (tag.id)}<option value={tag.id}>{tag.name}</option>{/each}
    </select>
  </label>
  {#each q.tagIds as id (id)}
    <span class="chip">{tagName(id)}<button onclick={() => (q.tagIds = q.tagIds.filter((other) => other !== id))} aria-label={t("Remove")}>×</button></span>
  {/each}
  <input class="search" type="search" placeholder={t("File name or note…")} bind:value={q.text} />
  {#if store.hasQuery}<button onclick={() => store.clearQuery()}>{t("Reset")}</button>{/if}
</div>

<style>
  .filters { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 14px; padding: 10px 16px;
    border-bottom: 1px solid var(--border); background: var(--panel); font-size: 13px; }
  label { display: flex; align-items: center; gap: 6px; color: var(--muted); }
  .search { width: 160px; }
</style>
