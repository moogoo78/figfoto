<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { api, src } from "../lib/api";
  import CategoryMove from "../lib/CategoryMove.svelte";
  import { store } from "../lib/store.svelte";

  let { id }: { id: number } = $props();
  const category = $derived(store.categories.find((c) => c.id === id));
  /** The categories above this one, top first. */
  const above = $derived(store.categoryPath(id).slice(0, -1));
  const inside = $derived(store.categories.filter((c) => c.parentId === id));
  const albums = $derived(store.albums.filter((a) => a.categoryId === id));
  /** Name being typed for a new category inside this one. */
  let newCategory = $state<string | null>(null);

  function createCategory() {
    const name = newCategory?.trim();
    newCategory = null;
    if (name) store.mutate(() => api.createCategory(name, id));
  }

  const focus = (el: HTMLInputElement) => el.focus();
</script>

<section class="category">
  <header>
    <nav class="path">
      <button class="link" onclick={() => store.setView({ kind: "albums" })}>{t("Albums")}</button>
      {#each above as c (c.id)}
        <span class="sep">›</span>
        <button class="link" onclick={() => store.setView({ kind: "category", id: c.id })}>{c.name}</button>
      {/each}
      <span class="sep">›</span>
    </nav>
    <h1>{category?.name ?? ""}</h1>
    <span class="count">{t("{n} album(s)", { n: albums.length })}</span>
    <span class="grow"></span>
    {#if newCategory != null}
      <input class="name-input" placeholder={t("Category name (e.g. Travel)")} bind:value={newCategory} use:focus onblur={createCategory}
        onkeydown={(e) => {
          if (e.key === "Enter") createCategory();
          if (e.key === "Escape") newCategory = null;
        }} />
    {:else}
      <button onclick={() => (newCategory = "")}>＋ {t("New category")}</button>
    {/if}
    {#if category}
      {@const pinned = category.shortcut != null}
      <CategoryMove {category} />
      <button class="pin" class:on={pinned} title={pinned ? t("Remove from shortcuts") : t("Add to shortcuts")}
        onclick={() => store.mutate(() => api.setShortcut("category", category.id, !pinned))}>{pinned ? "★" : "☆"}</button>
    {/if}
  </header>

  <div class="body">
    {#each inside as c (c.id)}
      <button class="tile" onclick={() => store.setView({ kind: "category", id: c.id })}>
        <span class="cover stack">{#if c.cover}<img src={src(c.cover)} alt="" />{:else}<span class="nocover"></span>{/if}</span>
        <strong title={c.name}>{c.name}</strong>
        <small>{t("{n} album(s)", { n: store.albumsUnder(c.id).length })}</small>
      </button>
    {/each}
    {#each albums as a (a.id)}
      <button class="tile" onclick={() => store.setView({ kind: "album", id: a.id })}>
        <span class="cover">{#if a.cover}<img src={src(a.cover)} alt="" />{:else}<span class="nocover"></span>{/if}</span>
        <strong title={a.name}>{a.name}</strong>
        <small>{t("{n} photos", { n: a.count })}</small>
      </button>
    {/each}
    {#if !inside.length && !albums.length}
      <p class="empty">{t("Drag albums onto this category on the Albums page, or choose a category on the album's page.")}</p>
    {/if}
  </div>
</section>

<style>
  .category { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  .path { display: flex; align-items: center; gap: 6px; font-size: 13px; color: var(--muted); min-width: 0; }
  .path .link { color: var(--muted); white-space: nowrap; }
  .path .link:hover { color: inherit; }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .grow { flex: 1; }
  /* Explicit width: some WebKitGTK builds otherwise squeeze a text field in a flex row to a sliver. */
  .name-input { width: 240px; flex-shrink: 0; }
  .pin.on { color: var(--star); }
  .body { flex: 1; overflow-y: auto; padding: 16px; display: grid; gap: 16px; align-content: start;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); }
  .tile { display: flex; flex-direction: column; gap: 4px; padding: 0; border: none; background: none;
    text-align: left; cursor: pointer; min-width: 0; }
  .cover { display: block; position: relative; }
  .cover img, .nocover { display: block; width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; background: var(--cell); }
  /* A category looks like a stack of albums. */
  .stack img, .stack .nocover { box-shadow: 4px -4px 0 -1px var(--panel), 4px -4px 0 0 var(--border), 8px -8px 0 -2px var(--panel), 8px -8px 0 -1px var(--border); }
  .tile:hover img, .tile:hover .nocover { filter: brightness(1.12); outline: 2px solid var(--accent); }
  .tile strong { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tile small { color: var(--muted); font-size: 12px; }
  .empty { grid-column: 1 / -1; color: var(--muted); font-size: 13px; }
</style>
