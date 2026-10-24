<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { untrack } from "svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { SOURCE_SUGGESTIONS, basename, dayAlbumPrefix, fmtDateTime } from "../lib/format";
  import { PhotoList } from "../lib/photoList.svelte";
  import Stars from "../lib/Stars.svelte";
  import { store } from "../lib/store.svelte";
  import SaveInput from "../lib/SaveInput.svelte";
  import AlbumMove from "../lib/AlbumMove.svelte";
  import AlbumOptions from "../lib/AlbumOptions.svelte";
  import AlbumTiles from "../lib/AlbumTiles.svelte";
  import type { AlbumDisplay, AlbumKind, ChildOrder } from "../lib/types";
  import SettingsInput from "../lib/SettingsInput.svelte";
  import LensInput from "../lib/LensInput.svelte";
  import DateInput from "../lib/DateInput.svelte";
  import TagInput from "../lib/TagInput.svelte";
  import { MARKS, type Mark } from "../lib/types";
  import VirtualGrid from "../lib/VirtualGrid.svelte";
  import FilterBar from "./FilterBar.svelte";
  import Viewer from "./Viewer.svelte";

  const list = new PhotoList();
  let grid: VirtualGrid | undefined = $state();
  let tagInput: TagInput | undefined = $state();
  let viewerIndex = $state<number | null>(null);
  let anchor: number | null = null;
  let showFilters = $state(false);
  /** Name being typed for a new album made from the selection; null when closed. */
  let newAlbumName = $state<string | null>(null);
  /** Album page: the Albums menu, and the name being typed for a new album inside. */
  let albumMenu = $state(false);
  let newChildName = $state<string | null>(null);
  /** The album the new one goes inside, when not the album shown. */
  let newChildParent = $state<{ id: number; name: string } | null>(null);

  const key = $derived(JSON.stringify([store.filter, store.sort]));
  let loadedKey = "";

  $effect(() => {
    const k = key;
    void store.version;
    untrack(() => {
      if (k !== loadedKey) {
        loadedKey = k;
        anchor = null;
        list.reload(store.filter, store.sort);
        grid?.scrollToTop();
      } else {
        list.refresh();
      }
    });
  });

  // Keep the grid scrolled to the photo shown in the viewer.
  $effect(() => {
    if (viewerIndex != null) grid?.scrollToIndex(viewerIndex);
  });

  const currentImport = $derived(
    store.view.kind === "import" ? store.imports.find((i) => i.id === (store.view as { id: number }).id) : undefined,
  );

  const title = $derived.by(() => {
    const v = store.view;
    switch (v.kind) {
      case "favorites": return t("Favorites");
      case "trash": return t("Trash");
      case "album": return store.albums.find((a) => a.id === v.id)?.name ?? t("Album");
      case "tag": return `#${store.tags.find((tag) => tag.id === v.id)?.name ?? ""}`;
      case "import": return t("Added from {name}", { name: basename(currentImport?.sourceDir ?? "") });
      case "mark": return store.markName(v.mark);
      default: return t("All photos");
    }
  });

  const selected = $derived([...store.selection]);
  const inTrash = $derived(store.view.kind === "trash");
  const albumId = $derived(store.view.kind === "album" ? store.view.id : null);
  const currentAlbum = $derived(albumId == null ? undefined : store.albums.find((a) => a.id === albumId));
  const markView = $derived(store.view.kind === "mark" ? store.view.mark : null);
  /** The albums above the one shown, top first, and the albums inside it (at any depth). */
  const above = $derived(albumId == null ? [] : store.albumPath(albumId).slice(0, -1));
  const inside = $derived(albumId == null ? [] : store.albumsInside(albumId));
  /** What goes inside the album shown: albums in a category, sections in an album or section. */
  const sections = $derived(currentAlbum != null && currentAlbum.role !== "category");
  /** Albums inside shown as tiles above the photos (the album's own only). */
  const tiles = $derived(currentAlbum?.display === "tiles" ? store.albumsIn(currentAlbum.id) : []);
  /** The photos are grouped under the albums inside. */
  const byChildren = $derived(albumId != null && store.sort === "children" && currentAlbum?.display === "inline" && inside.length > 0);

  // Collapsed albums inside this album, remembered per library in this browser only.
  const collapseKey = $derived(`figfoto.collapsedAlbums:${store.root}:${albumId}`);
  let collapsedAlbums = $state<number[]>([]);
  $effect(() => {
    try {
      collapsedAlbums = JSON.parse(localStorage.getItem(collapseKey) ?? "[]");
    } catch {
      collapsedAlbums = [];
    }
  });
  function toggleChild(id: number) {
    saveCollapsed(collapsedAlbums.includes(id) ? collapsedAlbums.filter((c) => c !== id) : [...collapsedAlbums, id]);
  }
  /** Every album inside, and the album's own photos (0). */
  const allChildren = $derived([...inside.map((a) => a.id), 0]);
  const allCollapsed = $derived(inside.length > 0 && allChildren.every((id) => collapsedAlbums.includes(id)));
  /** Collapses every album inside, or expands them all when they already are. */
  function toggleAllChildren() {
    saveCollapsed(allCollapsed ? [] : allChildren);
  }
  function saveCollapsed(ids: number[]) {
    collapsedAlbums = ids;
    try {
      localStorage.setItem(collapseKey, JSON.stringify(collapsedAlbums));
    } catch {
      // Not remembered; fine.
    }
  }

  function select(i: number, e: MouseEvent) {
    const p = list.get(i);
    if (!p) return;
    if (e.shiftKey && anchor != null) {
      if (!(e.ctrlKey || e.metaKey)) store.selection.clear();
      const [a, b] = anchor < i ? [anchor, i] : [i, anchor];
      // The range may span pages that aren't loaded, so ask for its ids.
      list.idsInRange(a, b - a + 1).then((ids) => ids.forEach((id) => store.selection.add(id)));
      return;
    }
    if (e.ctrlKey || e.metaKey) {
      if (store.selection.has(p.id)) store.selection.delete(p.id);
      else store.selection.add(p.id);
    } else {
      store.selection.clear();
      store.selection.add(p.id);
    }
    anchor = i;
  }

  async function selectAll() {
    for (const id of await list.allIds()) store.selection.add(id);
  }

  async function emptyTrash() {
    const msg = t("Remove {n} photo(s) from the library? The files are moved to the system Trash, where you can still recover them. Adding the same photos again will skip them.", { n: store.counts.trash });
    if (!(await ask(msg, { title: t("Empty trash"), kind: "warning" }))) return;
    store.mutate(() => api.emptyTrash(), t("Moved to the system Trash"));
  }

  /** Suggests an album name from the selected photos' dates. */
  async function suggestAlbumName(): Promise<string> {
    const span = await api.dateSpan(selected).catch(() => null);
    if (!span) return "";
    return dayAlbumPrefix(span[0]);
  }

  async function createAlbumFromSelection() {
    const name = newAlbumName?.trim();
    newAlbumName = null;
    if (!name) return;
    const ids = selected;
    let id = 0;
    await store.mutate(async () => {
      id = await api.createAlbum(name);
      await api.addToAlbum(id, ids);
    }, t("Album “{name}” created with {n} photo(s)", { name, n: ids.length }));
    if (id) store.selection.clear();
  }

  async function dayAction(action: "select" | "album", day: string, start: number, count: number) {
    const ids = await list.idsInRange(start, count);
    store.selection.clear();
    for (const id of ids) store.selection.add(id);
    anchor = start;
    if (action === "album") newAlbumName = dayAlbumPrefix(day) || day;
  }

  /** Adds an album inside the album shown (or inside `newChildParent`); the selected photos, if
   * any, move into it. */
  async function createChild() {
    const name = newChildName?.trim();
    const parent = newChildParent?.id ?? albumId;
    newChildName = null;
    newChildParent = null;
    if (!name || albumId == null || parent == null) return;
    const [album, ids] = [albumId, selected];
    await store.mutate(async () => {
      const id = await api.createAlbum(name, parent);
      if (ids.length) await api.movePhotos(album, id, ids);
    }, ids.length ? t("Album “{name}” made with {n} photo(s)", { name, n: ids.length }) : t("Album “{name}” added", { name }));
    if (currentAlbum?.display === "inline") store.sort = "children";
    store.selection.clear();
  }

  function setKind(kind: AlbumKind) {
    albumMenu = false;
    if (albumId == null) return;
    const album = albumId;
    store.mutate(() => api.setAlbumKind(album, kind), kind === "category" ? t("Now a category") : t("Now an album"));
  }

  function splitByDay() {
    albumMenu = false;
    if (albumId == null) return;
    const album = albumId;
    store.mutate(() => api.splitByDay(album), (n) => (n ? t("Split into {n} day(s)", { n }) : t("Every photo is already in an album inside")));
    if (currentAlbum?.display === "inline") store.sort = "children";
  }

  function setChildOrder(order: ChildOrder) {
    albumMenu = false;
    if (albumId == null || currentAlbum?.childOrder === order) return;
    const album = albumId;
    store.mutate(() => api.setChildOrder(album, order));
  }

  function setDisplay(display: AlbumDisplay) {
    albumMenu = false;
    if (albumId == null || currentAlbum?.display === display) return;
    const album = albumId;
    store.mutate(() => api.setAlbumDisplay(album, display));
    store.sort = display === "inline" && inside.length ? "children" : "takenAsc";
  }

  /** Moves the selected photos into an album inside (or the album shown itself). */
  function moveTo(target: number) {
    if (albumId == null) return;
    const [album, ids] = [albumId, selected];
    store.mutate(() => api.movePhotos(album, target, ids), t("Moved {n} photo(s)", { n: ids.length }));
  }

  async function onalbum(action: "rename" | "delete" | "drop" | "add", id: number, arg: { name?: string; ids?: number[]; children?: number }) {
    if (albumId == null) return;
    const album = albumId;
    if (action === "add") {
      newChildParent = { id, name: arg.name ?? "" };
      newChildName = "";
    } else if (action === "rename" && arg.name) store.mutate(() => api.renameAlbum(id, arg.name!));
    else if (action === "drop" && arg.ids?.length) {
      store.mutate(() => api.movePhotos(album, id || album, arg.ids!), t("Moved {n} photo(s)", { n: arg.ids.length }));
    } else if (action === "delete") {
      const parent = store.albums.find((a) => a.id === id)?.parentId;
      const msg = t('Delete album "{name}"? Its photos and the albums inside it move up to "{parent}".', {
        name: arg.name ?? "",
        parent: store.albums.find((a) => a.id === parent)?.name ?? "",
      });
      if (await ask(msg, { title: t("Delete album"), kind: "warning" })) store.mutate(() => api.deleteAlbum(id));
    }
  }

  function trashOrRestore(ids: number[]) {
    store.selection.clear();
    if (inTrash) store.mutate(() => api.restorePhotos(ids), t("Restored {n} photo(s)", { n: ids.length }));
    else store.mutate(() => api.trashPhotos(ids), t("Moved {n} photo(s) to trash", { n: ids.length }));
  }

  function onkeydown(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (viewerIndex != null || store.importOpen || t.closest("input, select, textarea")) return;
    const ids = selected;
    if ((e.ctrlKey || e.metaKey) && e.key === "a") selectAll();
    else if (e.key === "Escape") store.selection.clear();
    else if (e.key === "Enter" && anchor != null) viewerIndex = anchor;
    else if (!ids.length) return;
    else if (e.shiftKey && /^Digit[0-5]$/.test(e.code)) store.mutate(() => api.setRating(ids, Number(e.code.slice(5))));
    else if (e.key === "x" || /^[1-4]$/.test(e.key)) store.toggleMark(ids, e.key as Mark);
    else if (e.key === "f") store.mutate(() => api.toggleFavorite(ids));
    else if (e.key === "t") tagInput?.focus();
    else if (e.key === "Delete") trashOrRestore(ids);
    else return;
    e.preventDefault();
  }

  const focusSelect = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };

  /** Focuses a name field; a day prefix ("240503_") is kept and the topic typed after it. */
  const focusName = (el: HTMLInputElement) => {
    el.focus();
    if (el.value.endsWith("_")) el.setSelectionRange(el.value.length, el.value.length);
    else el.select();
  };

  const singleRating = $derived(selected.length === 1 ? (list.find(selected[0])?.rating ?? 0) : 0);
</script>

<svelte:window {onkeydown} onpointerdown={(e) => albumMenu && !(e.target as Element).closest?.(".menuwrap") && (albumMenu = false)} />

<section class="library">
  <header>
    {#if currentImport}
      <button class="link" onclick={() => store.setView({ kind: "imports" })} title={t("Back to add history")}>‹ {t("Add history")}</button>
    {/if}
    {#if markView}
      <button class="link" onclick={() => store.setView({ kind: "marks" })} title={t("Back to marks")}>‹ {t("Marks")}</button>
      <span class="mark mark-{markView}">{markView}</span>
    {/if}
    {#if currentAlbum}
      <nav class="path">
        <button class="link" onclick={() => store.setView({ kind: "albums" })}>{t("Albums")}</button>
        {#each above as a (a.id)}
          <span class="sep">›</span>
          <button class="link" onclick={() => store.setView({ kind: "album", id: a.id })}>{a.name}</button>
        {/each}
        <span class="sep">›</span>
      </nav>
    {/if}
    <h1 title={currentImport?.sourceDir}>{title}</h1>
    <span class="count">
      {t("{n} photos", { n: list.total.toLocaleString() })}{#if currentImport} · {t("added {date}", { date: fmtDateTime(currentImport.startedAt) })}{/if}
    </span>
    <span class="grow"></span>
    {#if currentAlbum}
      {#if newChildName != null}
        <input
          class="albumname"
          placeholder={newChildParent
            ? store.albums.find((a) => a.id === newChildParent?.id)?.role === "category"
              ? t("Album inside “{name}”", { name: newChildParent.name })
              : t("Section of “{name}”", { name: newChildParent.name })
            : sections
              ? selected.length ? t("Section for {n} selected", { n: selected.length }) : t("Section name")
              : selected.length ? t("Album for {n} selected", { n: selected.length }) : t("Album name")}
          bind:value={newChildName}
          use:focusSelect
          onblur={() => { newChildName = null; newChildParent = null; }}
          onkeydown={(e) => {
            if (e.key === "Enter") createChild();
            if (e.key === "Escape") { newChildName = null; newChildParent = null; }
            e.stopPropagation();
          }} />
        <button class="primary" onmousedown={(e) => e.preventDefault()} onclick={createChild}
          disabled={!newChildName.trim()}>{t("Save")}</button>
      {:else}
        <span class="menuwrap">
          <button onclick={() => (albumMenu = !albumMenu)}
            title={sections ? t("Sections of this album, like Morning and Evening") : t("Albums in this category, like 240503_Kyoto")}>
            {sections ? t("Sections") : t("Albums inside")} ▾
          </button>
          {#if albumMenu}
            <div class="menu" role="menu">
              <button role="menuitem" onclick={() => { albumMenu = false; newChildName = ""; }}>
                {#if sections}
                  {selected.length ? t("New section from {n} selected", { n: selected.length }) : t("New section")}
                {:else}
                  {selected.length ? t("New album inside from {n} selected", { n: selected.length }) : t("New album inside")}
                {/if}
              </button>
              <button role="menuitem" onclick={splitByDay}
                title={currentAlbum.role === "section" ? t("This section's own photos go into a section per day") : t("This album's own photos go into an album per day, and it becomes a category")}>
                {t("Split by day")}
              </button>
              {#if currentAlbum.role !== "section"}
                <button role="menuitem" onclick={() => setKind(currentAlbum.kind === "category" ? "album" : "category")}
                  title={currentAlbum.kind === "category" ? t("An album holds the photos of a day, with sections inside") : t("A category groups albums, like Travel › Japan")}>
                  {currentAlbum.kind === "category" ? t("Make it an album") : t("Make it a category")}
                </button>
              {/if}
              <hr />
              <button role="menuitemradio" aria-checked={currentAlbum.display === "inline"} onclick={() => setDisplay("inline")}
                title={t("Show their photos here, under a heading each")}>
                <span class="check">{currentAlbum.display === "inline" ? "✓" : ""}</span>{t("Show photos")}
              </button>
              <button role="menuitemradio" aria-checked={currentAlbum.display === "tiles"} onclick={() => setDisplay("tiles")}
                title={t("Show them as album tiles")}>
                <span class="check">{currentAlbum.display === "tiles" ? "✓" : ""}</span>{t("Show as tiles")}
              </button>
              <hr />
              {#each [["date", t("Oldest first")], ["newest", t("Newest first")], ["name", t("By name")]] as [order, label] (order)}
                <button role="menuitemradio" aria-checked={currentAlbum.childOrder === order} onclick={() => setChildOrder(order as ChildOrder)}>
                  <span class="check">{currentAlbum.childOrder === order ? "✓" : ""}</span>{label}
                </button>
              {/each}
            </div>
          {/if}
        </span>
        {#if byChildren}
          <button onclick={toggleAllChildren}>{allCollapsed ? t("Expand all") : t("Collapse all")}</button>
        {/if}
      {/if}
      <AlbumMove album={currentAlbum} />
      {@const pinned = currentAlbum.shortcut != null}
      <button class="pin" class:on={pinned} title={pinned ? t("Remove from shortcuts") : t("Add to shortcuts")}
        onclick={() => store.mutate(() => api.setShortcut(currentAlbum.id, !pinned))}>{pinned ? "★" : "☆"}</button>
    {/if}
    {#if markView && list.total > 0}
      <button class:danger={markView === "x"} class:primary={markView !== "x"}
        disabled={!!store.runningMark || !store.markSlot(markView)?.kind}
        title={store.markSlot(markView)?.kind ? "" : t("Set up this mark in the Marks view first")}
        onclick={() => store.runMark(markView)}>
        {store.runningMark === markView ? t("Running…") : markView === "x" ? t("Move {n} to trash", { n: list.total }) : t("Run on {n}", { n: list.total })}
      </button>
    {/if}
    {#if inTrash && store.counts.trash > 0}
      <button class="danger" onclick={emptyTrash}>{t("Empty trash")}</button>
    {/if}
    <button class:active={showFilters || store.hasQuery} onclick={() => (showFilters = !showFilters)}>
      {t("Filter")}{store.hasQuery ? " •" : ""}
    </button>
    <select bind:value={store.sort} title={t("Sort")}>
      <option value="takenDesc">{t("Newest first")}</option>
      <option value="takenAsc">{t("Oldest first")}</option>
      <option value="importedDesc">{t("Recently added")}</option>
      <option value="ratingDesc">{t("Highest rated")}</option>
      {#if currentAlbum?.display === "inline" && inside.length}<option value="children">{sections ? t("By section") : t("By album inside")}</option>{/if}
    </select>
    <input type="range" min="100" max="360" step="20" bind:value={store.thumbSize} title={t("Thumbnail size")} />
  </header>

  {#if showFilters}<FilterBar />{/if}

  {#if selected.length}
    <div class="selbar">
      <strong>{t("{n} selected", { n: selected.length })}</strong>
      {#if !inTrash}
        <span class="marks" role="group" aria-label={t("Mark")}>
          {#each MARKS as m}
            <button class="mark mark-{m}" title="{store.markName(m)} ({m})" onclick={() => store.toggleMark(selected, m)}>{m}</button>
          {/each}
          <button class="unmark" title={t("Clear mark")} onclick={() => store.mutate(() => api.setMark(selected, null))}>∅</button>
        </span>
        <Stars value={singleRating} onchange={(n) => store.mutate(() => api.setRating(selected, n))} />
        <button onclick={() => store.mutate(() => api.setFavorite(selected, true))}>♥ {t("Favorite")}</button>
        <button onclick={() => store.mutate(() => api.setFavorite(selected, false))}>♡ {t("Unfavorite")}</button>
        <TagInput bind:this={tagInput} onadd={(names) => store.mutate(() => api.addTags(selected, names), t("Tagged"))} />
        <SaveInput onset={(source) => store.mutate(() => api.setSource(selected, source), source ? t("Source set to {source}", { source }) : t("Source cleared"))}
          suggestions={[...new Set([...store.sources, ...SOURCE_SUGGESTIONS])]}
          placeholder={t("Source (LINE, Facebook…)")}
          title={t("Set where the selected photos came from. It replaces what they had; enter nothing to clear it.")} />
        <SettingsInput onset={(text) => store.mutate(() => api.setNote(selected, "settings", text), text ? t("Camera settings set") : t("Camera settings cleared"))} />
        <LensInput onset={(lens) => store.mutate(() => api.setLens(selected, lens), (n) =>
          !lens ? t("Lens cleared")
          : n === selected.length ? t("Lens set to {lens}", { lens })
          : t("Lens set to {lens} on {n} photo(s); {m} have a lens from the camera", { lens, n, m: selected.length - n }))} />
        <DateInput onset={(date, time) => store.mutate(() => api.setTakenAt(selected, date, time), (n) => t("Date set on {n} photo(s)", { n }))} />
        {#if newAlbumName != null}
          <input
            class="albumname"
            placeholder={t("New album name")}
            bind:value={newAlbumName}
            use:focusName
            onblur={() => (newAlbumName = null)}
            onkeydown={(e) => {
              if (e.key === "Enter") createAlbumFromSelection();
              if (e.key === "Escape") newAlbumName = null;
              e.stopPropagation();
            }} />
          <!-- mousedown would blur the name field and close it before the click lands -->
          <button class="primary" onmousedown={(e) => e.preventDefault()} onclick={createAlbumFromSelection}
            disabled={!newAlbumName.trim()}>{t("Save")}</button>
        {:else}
          <button onclick={async () => (newAlbumName = await suggestAlbumName())}>＋ {t("New album")}</button>
        {/if}
        {#if store.albums.length}
          <select
            onchange={(e) => {
              const id = Number(e.currentTarget.value);
              e.currentTarget.value = "";
              if (id) store.mutate(() => api.addToAlbum(id, selected), t("Added to album"));
            }}>
            <option value="">{t("Add to album…")}</option>
            <AlbumOptions />
          </select>
        {/if}
        {#if albumId != null && currentAlbum}
          <select
            onchange={(e) => {
              const v = e.currentTarget.value;
              e.currentTarget.value = "";
              if (v === "new") newChildName = "";
              else if (v) moveTo(Number(v));
            }}>
            <option value="">{sections ? t("Move to section…") : t("Move to album inside…")}</option>
            {#each inside as a (a.id)}
              <option value={a.id}>{"\u00a0\u00a0".repeat(a.depth - currentAlbum.depth - 1)}{a.name}</option>
            {/each}
            {#if inside.length}<option value={currentAlbum.id}>{t("{name} itself", { name: currentAlbum.name })}</option>{/if}
            <option value="new">{sections ? t("New section…") : t("New album inside…")}</option>
          </select>
          <button onclick={() => store.mutate(() => api.removeFromAlbum(albumId, selected, true))}>{t("Remove from album")}</button>
          {#if selected.length === 1}
            <button onclick={() => store.mutate(() => api.setAlbumCover(albumId, selected[0]), t("Cover set"))}>{t("Set as cover")}</button>
          {/if}
        {/if}
      {/if}
      <span class="grow"></span>
      <button class:danger={!inTrash} onclick={() => trashOrRestore(selected)}>{inTrash ? t("Restore") : t("Trash")}</button>
      <button onclick={() => store.selection.clear()}>{t("Clear")}</button>
    </div>
  {/if}

  <div class="body">
    {#if tiles.length}
      <div class="tiles" class:full={list.loaded && list.total === 0}><AlbumTiles albums={tiles} from={albumId} /></div>
    {/if}
    <div class="photos">
      {#if !list.loaded}
        <div></div>
      {:else if list.total === 0}
        {#if !tiles.length}
          <div class="empty">
            {#if store.hasQuery}
              <p>{t("No photos match the current filters.")}</p>
              <button onclick={() => store.clearQuery()}>{t("Clear filters")}</button>
            {:else if store.view.kind === "library"}
              <p>{t("Your library is empty.")}</p>
              <button class="primary" onclick={() => store.openImport()}>{t("Add photos")}</button>
            {:else if inTrash}
              <p>{t("Trash is empty.")}</p>
            {:else}
              <p>{t("Nothing here yet.")}</p>
            {/if}
          </div>
        {/if}
      {:else}
        <VirtualGrid bind:this={grid} {list} onselect={select} onopen={(i) => (viewerIndex = i)}
          ondayaction={inTrash ? undefined : dayAction} showOrigin={albumId != null}
          onalbum={byChildren ? onalbum : undefined} ownLabel={t("Only in {name}", { name: currentAlbum?.name ?? "" })}
          collapsed={collapsedAlbums} ontoggle={byChildren ? toggleChild : undefined} />
      {/if}
    </div>
  </div>
</section>

{#if viewerIndex != null}
  <Viewer {list} bind:index={viewerIndex} onclose={() => (viewerIndex = null)} />
{/if}

<style>
  .library { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .grow { flex: 1; }
  input[type="range"] { width: 100px; }
  .selbar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 8px 16px;
    background: var(--accent-bg); border-bottom: 1px solid var(--border); }
  .body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .tiles { flex-shrink: 0; max-height: 45%; overflow-y: auto; padding: 16px; border-bottom: 1px solid var(--border); }
  .tiles.full { flex: 1; max-height: none; border-bottom: none; }
  .photos { flex: 1; min-height: 0; }
  .path { display: flex; align-items: center; gap: 6px; font-size: 13px; min-width: 0; }
  .path .link { color: var(--muted); white-space: nowrap; }
  .path .link:hover { color: inherit; }
  .path .sep { color: var(--muted); }
  .marks { display: inline-flex; gap: 3px; }
  .marks button { padding: 0 7px; height: 26px; line-height: 26px; border: none; }
  .marks .unmark { background: var(--cell); color: var(--muted); }
  .albumname { width: 200px; }
  .menuwrap { position: relative; }
  .pin.on { color: var(--star); }
  .menu { position: absolute; right: 0; top: 100%; z-index: 20; display: flex; flex-direction: column; min-width: 170px;
    margin-top: 4px; padding: 4px; background: var(--panel); border: 1px solid var(--border); border-radius: 6px;
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.35); }
  .menu button { border: none; background: none; text-align: left; padding: 6px 10px; font-size: 13px; white-space: nowrap; }
  .menu button:hover { background: var(--hover); }
  .menu hr { margin: 4px 0; border: none; border-top: 1px solid var(--border); }
  .menu .check { display: inline-block; width: 1.2em; }
  .empty { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 8px; color: var(--muted); }
</style>
