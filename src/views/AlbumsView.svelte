<script lang="ts">
  import { i18n, t } from "../lib/i18n.svelte";
  import { api } from "../lib/api";
  import AlbumTiles from "../lib/AlbumTiles.svelte";
  import AlbumTree from "../lib/AlbumTree.svelte";
  import { store } from "../lib/store.svelte";
  import type { Album } from "../lib/types";

  /** Name being typed for a new album or category. */
  let newAlbum = $state<string | null>(null);

  /** Daily: every album (a day's photos) by date, wherever it is filed; categories: the top-level
   * categories; tree: everything. Remembered in this browser only. */
  type Mode = "daily" | "categories" | "tree";
  const VIEW_KEY = "figfoto.albumsView";
  let mode = $state<Mode>(readMode());
  function readMode(): Mode {
    try {
      const m = localStorage.getItem(VIEW_KEY);
      return m === "categories" || m === "tree" ? m : "daily";
    } catch {
      return "daily";
    }
  }
  function setMode(m: Mode) {
    mode = m;
    try {
      localStorage.setItem(VIEW_KEY, m);
    } catch {
      // Not remembered; fine.
    }
  }

  /** Daily: only albums not filed into a category yet. */
  let unfiled = $state(false);

  const days = $derived(
    store.albums
      .filter((a) => a.role === "album" && (!unfiled || a.parentId == null))
      .sort((a, b) => (b.firstTaken ?? "").localeCompare(a.firstTaken ?? "") || a.name.localeCompare(b.name)),
  );
  /** Daily albums by year, then month ("2024-05"); albums without photos under "". */
  const years = $derived.by(() => {
    const out: { year: string; months: { month: string; albums: Album[] }[] }[] = [];
    for (const a of days) {
      const month = a.firstTaken?.slice(0, 7) ?? "";
      const year = month.slice(0, 4);
      if (out.at(-1)?.year !== year) out.push({ year, months: [] });
      const months = out.at(-1)!.months;
      if (months.at(-1)?.month !== month) months.push({ month, albums: [] });
      months.at(-1)!.albums.push(a);
    }
    return out;
  });
  const categories = $derived(store.albumsIn(null).filter((a) => a.kind === "category"));

  // Open years, per library in this browser; the newest is open until any is toggled.
  const openKey = $derived(`figfoto.albumsDaily.open:${store.root}`);
  let openYears = $state<string[] | null>(null);
  $effect(() => {
    try {
      openYears = JSON.parse(localStorage.getItem(openKey) ?? "null");
    } catch {
      openYears = null;
    }
  });
  const isOpen = (year: string) => (openYears ?? years.slice(0, 1).map((y) => y.year)).includes(year);
  function toggleYear(year: string) {
    const now = openYears ?? years.slice(0, 1).map((y) => y.year);
    openYears = now.includes(year) ? now.filter((y) => y !== year) : [...now, year];
    try {
      localStorage.setItem(openKey, JSON.stringify(openYears));
    } catch {
      // Not remembered; fine.
    }
  }

  /** Album search: every word must be in the name or the category path above it. */
  let search = $state("");
  let searchInput: HTMLInputElement | undefined = $state();
  const MAX_RESULTS = 200;
  const norm = (s: string) => s.normalize("NFKC").toLowerCase();
  const matches = $derived.by(() => {
    const words = norm(search).split(/\s+/).filter(Boolean);
    if (!words.length) return null;
    const found = store.albums.filter((a) => {
      const text = norm(`${a.name} ${store.albumLabel(a.parentId)}`);
      return words.every((w) => text.includes(w));
    });
    const newest = (x: Album, y: Album) => (y.firstTaken ?? "").localeCompare(x.firstTaken ?? "") || x.name.localeCompare(y.name);
    return [
      { label: t("Categories"), albums: found.filter((a) => a.role === "category") },
      { label: t("Albums"), albums: found.filter((a) => a.role === "album").sort(newest) },
      { label: t("Sections"), albums: found.filter((a) => a.role === "section") },
    ];
  });
  const matchCount = $derived(matches?.reduce((n, g) => n + g.albums.length, 0) ?? 0);
  /** The groups cut to MAX_RESULTS tiles in all. */
  const shown = $derived.by(() => {
    let left = MAX_RESULTS;
    return (matches ?? [])
      .map((g) => {
        const albums = g.albums.slice(0, left);
        left -= albums.length;
        return { ...g, albums };
      })
      .filter((g) => g.albums.length);
  });

  function searchKeys(e: KeyboardEvent) {
    if (e.key === "Escape") search = "";
    const first = shown[0]?.albums[0];
    if (e.key === "Enter" && first) store.setView({ kind: "album", id: first.id });
    e.stopPropagation();
  }
  /** `/` or Ctrl+F focuses the search, unless typing somewhere. */
  function windowKeys(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    if (el.closest?.("input, textarea, select, [contenteditable]")) return;
    if (e.key === "/" || ((e.ctrlKey || e.metaKey) && e.key === "f")) {
      e.preventDefault();
      searchInput?.focus();
    }
  }

  function monthLabel(month: string) {
    const d = new Date(`${month}-01T00:00:00`);
    return isNaN(d.getTime()) ? t("No photos") : d.toLocaleDateString(i18n.locale, { month: "long" });
  }

  async function createAlbum() {
    const name = newAlbum?.trim();
    newAlbum = null;
    if (!name) return;
    let id = 0;
    await store.mutate(async () => (id = await api.createAlbum(name, null, mode === "daily" ? "album" : "category")));
    if (id) store.setView({ kind: "album", id });
  }

  const focus = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };
</script>

<svelte:window onkeydown={windowKeys} />

<section class="albums">
  <header>
    <h1>{t("Albums")}</h1>
    {#if matches}
      <span class="count">{t("{n} match(es)", { n: matchCount })}</span>
    {:else}
      <span class="count">{t("{n} album(s)", { n: days.length })} · {t("{n} categories", { n: store.albums.filter((a) => a.role === "category").length })}</span>
    {/if}
    <span class="grow"></span>
    {#if store.albums.length}
      <input class="search" type="search" placeholder={t("Search albums…")} bind:value={search} bind:this={searchInput}
        onkeydown={searchKeys} title={t("Any part of a name or its category, like kyoto, 2405 or japan 1905. Press / to search.")} />
      <div class="modes" role="group">
        <button aria-pressed={mode === "daily"} onclick={() => setMode("daily")} title={t("Every album by date, wherever it is filed")}>{t("Daily")}</button>
        <button aria-pressed={mode === "categories"} onclick={() => setMode("categories")}>{t("Categories")}</button>
        <button aria-pressed={mode === "tree"} onclick={() => setMode("tree")}>{t("Tree")}</button>
      </div>
    {/if}
    {#if newAlbum != null}
      <input class="name-input" placeholder={mode === "daily" ? t("Album name, like 240503_Kyoto") : t("Category name")} bind:value={newAlbum}
        use:focus onblur={createAlbum}
        onkeydown={(e) => {
          if (e.key === "Enter") createAlbum();
          if (e.key === "Escape") newAlbum = null;
        }} />
    {:else}
      <button class="primary" onclick={() => (newAlbum = "")}>＋ {mode === "daily" ? t("New album") : t("New category")}</button>
    {/if}
  </header>

  <div class="body">
    {#if !store.albums.length}
      <p class="hint">{t("No albums yet. Make one here, or from photos you select.")}</p>
    {:else if matches}
      {#each shown as g (g.label)}
        <h3>{g.label}</h3>
        <AlbumTiles albums={g.albums} paths />
      {:else}
        <p class="hint">{t("No albums match “{q}”.", { q: search.trim() })}</p>
      {/each}
      {#if matchCount > MAX_RESULTS}
        <p class="hint">{t("Showing {shown} of {n}; type more to narrow it", { shown: MAX_RESULTS, n: matchCount })}</p>
      {/if}
    {:else if mode === "tree"}
      <AlbumTree />
      <p class="hint">{t("Drag an album onto another to put it inside, or onto the top level to take it out.")}</p>
    {:else if mode === "categories"}
      {#if categories.length}
        <AlbumTiles albums={categories} />
      {/if}
      <p class="hint">{t("Categories group albums, like Travel › Japan. Drag albums from Daily or the tree onto a category to file them.")}</p>
    {:else}
      <label class="unfiled"><input type="checkbox" bind:checked={unfiled} /> {t("Only albums not in a category")}</label>
      {#each years as y (y.year)}
        <section class="year">
          <button class="year-head" aria-expanded={isOpen(y.year)} onclick={() => toggleYear(y.year)}>
            <span class="caret">{isOpen(y.year) ? "▾" : "▸"}</span>
            <h2>{y.year || t("No photos")}</h2>
            <small>{t("{n} album(s)", { n: y.months.reduce((n, m) => n + m.albums.length, 0) })}</small>
          </button>
          {#if isOpen(y.year)}
            {#each y.months as m (m.month)}
              {#if m.month}<h3>{monthLabel(m.month)}</h3>{/if}
              <AlbumTiles albums={m.albums} paths />
            {/each}
          {/if}
        </section>
      {:else}
        <p class="hint">{t("Every album is in a category.")}</p>
      {/each}
      <p class="hint">{t("An album holds the photos of a day; name it like 240503_Kyoto. Drag it onto a category to file it; it stays here too.")}</p>
    {/if}
  </div>
</section>

<style>
  .albums { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .grow { flex: 1; }
  /* Explicit width: some WebKitGTK builds otherwise squeeze a text field in a flex row to a sliver. */
  .name-input { width: 240px; flex-shrink: 0; }
  .search { width: 200px; flex-shrink: 0; }
  .body { flex: 1; overflow-y: auto; padding: 16px; }
  .modes { display: inline-flex; }
  .modes button { border-radius: 0; }
  .modes button:first-child { border-radius: 6px 0 0 6px; }
  .modes button + button { margin-left: -1px; }
  .modes button:last-child { border-radius: 0 6px 6px 0; }
  .modes button[aria-pressed="true"] { background: var(--accent-bg); color: var(--accent-fg); }
  .hint { color: var(--muted); font-size: 13px; margin: 16px 0 0; }
  .unfiled { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); font-size: 13px; margin-bottom: 8px; }
  .year { margin-bottom: 8px; }
  .year-head { display: flex; align-items: baseline; gap: 8px; width: 100%; padding: 6px 0; border: none; background: none;
    color: inherit; text-align: left; cursor: pointer; }
  .year-head h2 { font-size: 16px; margin: 0; }
  .year-head small { color: var(--muted); font-size: 12px; }
  .caret { width: 14px; color: var(--muted); }
  h3 { font-size: 13px; font-weight: 600; color: var(--muted); margin: 12px 0 8px; }
</style>
