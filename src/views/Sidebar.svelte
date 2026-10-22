<script lang="ts">
  import { i18n, LOCALES, t, type Locale } from "../lib/i18n.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, src } from "../lib/api";
  import { basename } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import Wordmark from "../lib/Wordmark.svelte";
  import type { Album, Category, View } from "../lib/types";

  /** Shortcut album that dragged photos would be added to. */
  let dropTarget = $state<number | null>(null);

  const PHOTOS_MIME = "application/x-foto-ids";

  const is = (v: View) => JSON.stringify(v) === JSON.stringify(store.view);

  /** Pinned albums and categories, in the order they were pinned. */
  const shortcuts = $derived(
    [
      ...store.albums.filter((a) => a.shortcut != null).map((album) => ({ order: album.shortcut!, album, category: null })),
      ...store.categories.filter((c) => c.shortcut != null).map((category) => ({ order: category.shortcut!, album: null, category })),
    ].sort((x, y) => x.order - y.order) as { order: number; album: Album | null; category: Category | null }[],
  );

  function drop(e: DragEvent, albumId: number) {
    dropTarget = null;
    const raw = e.dataTransfer?.getData(PHOTOS_MIME);
    if (!raw) return;
    e.preventDefault();
    const ids = JSON.parse(raw) as number[];
    store.mutate(() => api.addToAlbum(albumId, ids), t("Added {n} photo(s)", { n: ids.length }));
  }

  async function switchLibrary(create: boolean) {
    const dir = await open({
      directory: true,
      title: create ? t("Choose an empty folder for the new library") : t("Open FigFoto library"),
    });
    if (typeof dir === "string") store.openLibrary(dir, create);
  }
</script>

<nav class="sidebar">
  <div class="brand">
    <Wordmark size={20} />
  </div>
  <button class="primary import" onclick={() => store.openImport()}>＋ {t("Add photos")}</button>

  <div class="section">
    <button class="item" class:active={is({ kind: "library" })} onclick={() => store.setView({ kind: "library" })}>
      <span>{t("All photos")}</span><small>{store.counts.all}</small>
    </button>
    <button class="item" class:active={is({ kind: "albums" })} onclick={() => store.setView({ kind: "albums" })}>
      <span>{t("Albums")}</span><small>{store.albums.length || ""}</small>
    </button>
    <button class="item" class:active={is({ kind: "favorites" })} onclick={() => store.setView({ kind: "favorites" })}>
      <span>{t("Favorites")}</span><small>{store.counts.favorites}</small>
    </button>
    <button class="item" class:active={store.view.kind === "imports" || store.view.kind === "import"}
      onclick={() => store.setView({ kind: "imports" })}>
      <span>{t("Add history")}</span><small>{store.imports.length || ""}</small>
    </button>
    <button class="item" class:active={store.view.kind === "marks" || store.view.kind === "mark"}
      onclick={() => store.setView({ kind: "marks" })}>
      <span>{t("Marks")}</span><small>{store.counts.marked || ""}</small>
    </button>
    <button class="item" class:active={is({ kind: "lenses" })} onclick={() => store.setView({ kind: "lenses" })}>
      <span>{t("Lenses")}</span><small>{store.lenses.length || ""}</small>
    </button>
    <button class="item" class:active={is({ kind: "backup" })} onclick={() => store.setView({ kind: "backup" })}
      title={store.counts.unbacked ? t("{n} photo(s) not backed up", { n: store.counts.unbacked }) : undefined}>
      <span>{t("Backup")}</span><small class:warn={store.counts.unbacked > 0}>{store.counts.unbacked || ""}</small>
    </button>
    <button class="item" class:active={is({ kind: "duplicates" })} onclick={() => store.setView({ kind: "duplicates" })}>
      <span>{t("Duplicates")}</span>
    </button>
    <button class="item" class:active={is({ kind: "trash" })} onclick={() => store.setView({ kind: "trash" })}>
      <span>{t("Trash")}</span><small>{store.counts.trash || ""}</small>
    </button>
  </div>

  <div class="section">
    <div class="heading">{t("Shortcuts")}</div>
    {#each shortcuts as sc (sc.album ? `a${sc.album.id}` : `c${sc.category?.id}`)}
      {#if sc.album}
        {@const a = sc.album}
        <div class="item album" class:active={is({ kind: "album", id: a.id })} class:drop={dropTarget === a.id}
          role="button" tabindex="0"
          onclick={() => store.setView({ kind: "album", id: a.id })}
          onkeydown={(e) => e.key === "Enter" && store.setView({ kind: "album", id: a.id })}
          ondragover={(e) => {
            if (!e.dataTransfer?.types.includes(PHOTOS_MIME)) return;
            e.preventDefault();
            dropTarget = a.id;
          }}
          ondragleave={() => (dropTarget = null)}
          ondrop={(e) => drop(e, a.id)}>
          {#if a.cover}<img src={src(a.cover)} alt="" />{:else}<span class="nocover"></span>{/if}
          <span class="name">{a.name}</span>
          <small>{a.count}</small>
          <button class="icon del" title={t("Remove from shortcuts")} onclick={(e) => {
            e.stopPropagation();
            store.mutate(() => api.setShortcut("album", a.id, false));
          }}>×</button>
        </div>
      {:else if sc.category}
        {@const c = sc.category}
        <div class="item category" class:active={is({ kind: "category", id: c.id })} role="button" tabindex="0"
          title={store.categoryLabel(c.id)}
          onclick={() => store.setView({ kind: "category", id: c.id })}
          onkeydown={(e) => e.key === "Enter" && store.setView({ kind: "category", id: c.id })}>
          {#if c.cover}<img src={src(c.cover)} alt="" />{:else}<span class="nocover"></span>{/if}
          <span class="name">{c.name}</span>
          <small>{t("{n} album(s)", { n: store.albumsUnder(c.id).length })}</small>
          <button class="icon del" title={t("Remove from shortcuts")} onclick={(e) => {
            e.stopPropagation();
            store.mutate(() => api.setShortcut("category", c.id, false));
          }}>×</button>
        </div>
      {/if}
    {:else}
      <p class="hint">{t("Pin albums or categories with ☆ to keep them here.")}</p>
    {/each}
  </div>

  <div class="section tags">
    <div class="heading">{t("Tags")}</div>
    {#each store.tags as tag (tag.id)}
      <button class="item" class:active={is({ kind: "tag", id: tag.id })} onclick={() => store.setView({ kind: "tag", id: tag.id })}>
        <span>#{tag.name}</span><small>{tag.count}</small>
      </button>
    {:else}
      <p class="hint">{t("No tags yet. Select photos and press T.")}</p>
    {/each}
  </div>

  <div class="footer" title={store.root}>
    <span>📁 {basename(store.root ?? "")}</span>
    <button class="link" onclick={() => switchLibrary(true)}>{t("New")}</button>
    <button class="link" onclick={() => switchLibrary(false)}>{t("Switch")}</button>
    <select class="lang" value={i18n.locale} title="Language / 語言"
      onchange={(e) => i18n.set(e.currentTarget.value as Locale)}>
      {#each LOCALES as l (l.value)}<option value={l.value}>{l.label}</option>{/each}
    </select>
  </div>
</nav>

<style>
  .sidebar { width: 230px; flex-shrink: 0; display: flex; flex-direction: column; background: var(--panel);
    border-right: 1px solid var(--border); overflow-y: auto; }
  .brand { padding: 16px 16px 10px; }
  .import { margin: 0 12px 8px; }
  .section { padding: 8px 8px; border-top: 1px solid var(--border); }
  .heading { display: flex; justify-content: space-between; align-items: center; padding: 4px 8px;
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .item { display: flex; align-items: center; gap: 8px; width: 100%; padding: 6px 8px; border: none; border-radius: 6px;
    background: none; color: inherit; text-align: left; font-size: 13px; cursor: pointer; }
  .item > span:first-child:not(.nocover), .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .item:hover { background: var(--hover); }
  .item.active { background: var(--accent-bg); color: var(--accent-fg); }
  .item.drop { outline: 2px dashed var(--accent); }
  small { color: var(--muted); font-size: 11px; }
  small.warn { color: var(--danger); }
  .item img, .nocover { width: 24px; height: 24px; border-radius: 4px; object-fit: cover; background: var(--cell); flex-shrink: 0; }
  .icon { background: none; border: none; color: var(--muted); padding: 0 4px; font-size: 14px; cursor: pointer; }
  .del { visibility: hidden; }
  .item:hover .del { visibility: visible; }
  .hint { color: var(--muted); font-size: 12px; padding: 0 8px; margin: 4px 0; }
  .tags { flex: 1; }
  .footer { display: flex; align-items: center; gap: 6px; padding: 10px 14px; border-top: 1px solid var(--border);
    font-size: 12px; color: var(--muted); }
  .footer .lang { padding-inline: 4px; }
  .footer span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
