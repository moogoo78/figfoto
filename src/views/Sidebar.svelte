<script lang="ts">
  import { i18n, LOCALES, t, type Locale } from "../lib/i18n.svelte";
  import { ask, open } from "@tauri-apps/plugin-dialog";
  import { api, src } from "../lib/api";
  import { basename } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import Wordmark from "../lib/Wordmark.svelte";
  import type { Album, View } from "../lib/types";

  let newAlbum = $state<string | null>(null);
  let renaming = $state<number | null>(null);
  let renameValue = $state("");
  let dropTarget = $state<number | null>(null);
  let newTopic = $state<string | null>(null);
  let renamingTopic = $state<number | null>(null);
  let addMenu = $state(false);
  /** Where a dragged album would land: a topic id, or "none" (out of its topic). */
  let topicDrop = $state<number | "none" | null>(null);

  const ALBUM_MIME = "application/x-foto-album";
  const PHOTOS_MIME = "application/x-foto-ids";
  const carries = (e: DragEvent, mime: string) => !!e.dataTransfer?.types.includes(mime);

  // Collapsed topics, remembered per library in this browser only.
  const collapseKey = $derived(`frutyfoto.collapsedTopics:${store.root}`);
  let collapsed = $state<number[]>([]);
  $effect(() => {
    try {
      collapsed = JSON.parse(localStorage.getItem(collapseKey) ?? "[]");
    } catch {
      collapsed = [];
    }
  });
  function toggleTopic(id: number) {
    collapsed = collapsed.includes(id) ? collapsed.filter((c) => c !== id) : [...collapsed, id];
    try {
      localStorage.setItem(collapseKey, JSON.stringify(collapsed));
    } catch {
      // Not remembered; fine.
    }
  }

  const is = (v: View) => JSON.stringify(v) === JSON.stringify(store.view);

  async function createAlbum() {
    const name = newAlbum?.trim();
    newAlbum = null;
    if (!name) return;
    let id = 0;
    await store.mutate(async () => (id = await api.createAlbum(name)));
    if (id) store.setView({ kind: "album", id });
  }

  async function finishRename(id: number) {
    const name = renameValue.trim();
    renaming = null;
    if (name) store.mutate(() => api.renameAlbum(id, name));
  }

  async function createTopic() {
    const name = newTopic?.trim();
    newTopic = null;
    if (name) store.mutate(() => api.createTopic(name));
  }

  function finishTopicRename(id: number) {
    const name = renameValue.trim();
    renamingTopic = null;
    if (name) store.mutate(() => api.renameTopic(id, name));
  }

  async function deleteTopic(id: number, name: string) {
    if (await ask(t('Delete topic "{name}"? Its albums stay and move out of the topic.', { name }), { title: t("Delete topic"), kind: "warning" })) {
      store.mutate(() => api.deleteTopic(id));
    }
  }

  function dropAlbum(e: DragEvent, topicId: number | null) {
    topicDrop = null;
    const raw = e.dataTransfer?.getData(ALBUM_MIME);
    if (!raw) return;
    e.preventDefault();
    store.mutate(() => api.setAlbumTopic([Number(raw)], topicId));
  }

  async function deleteAlbum(id: number, name: string) {
    if (await ask(t('Delete album "{name}"? Photos stay in the library.', { name }), { title: t("Delete album"), kind: "warning" })) {
      store.mutate(() => api.deleteAlbum(id));
    }
  }

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
      title: create ? t("Choose an empty folder for the new library") : t("Open frutyfoto library"),
    });
    if (typeof dir === "string") store.openLibrary(dir, create);
  }

  const focus = (el: HTMLInputElement) => el.focus();
</script>

{#snippet albumRow(a: Album, nested: boolean)}
  {#if renaming === a.id}
    <input
      class="inline"
      class:nested
      bind:value={renameValue}
      use:focus
      onblur={() => finishRename(a.id)}
      onkeydown={(e) => {
        if (e.key === "Enter") finishRename(a.id);
        if (e.key === "Escape") renaming = null;
      }} />
  {:else}
    <div
      class="item album"
      class:nested
      class:active={is({ kind: "album", id: a.id })}
      class:drop={dropTarget === a.id}
      role="button"
      tabindex="0"
      draggable="true"
      ondragstart={(e) => e.dataTransfer?.setData(ALBUM_MIME, String(a.id))}
      onclick={() => store.setView({ kind: "album", id: a.id })}
      onkeydown={(e) => e.key === "Enter" && store.setView({ kind: "album", id: a.id })}
      ondblclick={() => {
        renaming = a.id;
        renameValue = a.name;
      }}
      ondragover={(e) => {
        if (!carries(e, PHOTOS_MIME)) return;
        e.preventDefault();
        dropTarget = a.id;
      }}
      ondragleave={() => (dropTarget = null)}
      ondrop={(e) => drop(e, a.id)}>
      {#if a.cover}<img src={src(a.cover)} alt="" />{:else}<span class="nocover"></span>{/if}
      <span class="name" title={t("Double-click to rename")}>{a.name}</span>
      <small>{a.count}</small>
      <button class="icon del" title={t("Delete album")} onclick={(e) => {
        e.stopPropagation();
        deleteAlbum(a.id, a.name);
      }}>×</button>
    </div>
  {/if}
{/snippet}

<svelte:window onpointerdown={(e) => addMenu && !(e.target as Element).closest?.(".addwrap") && (addMenu = false)} />

<nav class="sidebar">
  <div class="brand">
    <Wordmark size={20} />
  </div>
  <button class="primary import" onclick={() => store.openImport()}>＋ {t("Add photos")}</button>

  <div class="section">
    <button class="item" class:active={is({ kind: "library" })} onclick={() => store.setView({ kind: "library" })}>
      <span>{t("All photos")}</span><small>{store.counts.all}</small>
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
    <button class="item" class:active={is({ kind: "duplicates" })} onclick={() => store.setView({ kind: "duplicates" })}>
      <span>{t("Duplicates")}</span>
    </button>
    <button class="item" class:active={is({ kind: "trash" })} onclick={() => store.setView({ kind: "trash" })}>
      <span>{t("Trash")}</span><small>{store.counts.trash || ""}</small>
    </button>
  </div>

  <div class="section">
    <div
      class="heading"
      class:drop={topicDrop === "none"}
      role="group"
      ondragover={(e) => {
        if (!carries(e, ALBUM_MIME)) return;
        e.preventDefault();
        topicDrop = "none";
      }}
      ondragleave={() => (topicDrop = null)}
      ondrop={(e) => dropAlbum(e, null)}>
      {t("Albums")}
      <span class="addwrap">
        <button class="icon" title={t("New album or topic")} onclick={() => (addMenu = !addMenu)}>＋</button>
        {#if addMenu}
          <div class="menu" role="menu">
            <button role="menuitem" onclick={() => { addMenu = false; newAlbum = ""; }}>{t("New album")}</button>
            <button role="menuitem" onclick={() => { addMenu = false; newTopic = ""; }}>{t("New topic")}</button>
          </div>
        {/if}
      </span>
    </div>
    {#if newTopic != null}
      <input
        class="inline"
        placeholder={t("Topic name (e.g. Travel)")}
        bind:value={newTopic}
        use:focus
        onblur={createTopic}
        onkeydown={(e) => {
          if (e.key === "Enter") createTopic();
          if (e.key === "Escape") newTopic = null;
        }} />
    {/if}
    {#if newAlbum != null}
      <input
        class="inline"
        placeholder={t("Album name")}
        bind:value={newAlbum}
        use:focus
        onblur={createAlbum}
        onkeydown={(e) => {
          if (e.key === "Enter") createAlbum();
          if (e.key === "Escape") newAlbum = null;
        }} />
    {/if}

    {#each store.albumsByTopic() as g (g.topic?.id ?? 0)}
      {#if g.topic}
        {@const tp = g.topic}
        {@const open = !collapsed.includes(tp.id)}
        {#if renamingTopic === tp.id}
          <input
            class="inline"
            bind:value={renameValue}
            use:focus
            onblur={() => finishTopicRename(tp.id)}
            onkeydown={(e) => {
              if (e.key === "Enter") finishTopicRename(tp.id);
              if (e.key === "Escape") renamingTopic = null;
            }} />
        {:else}
          <div
            class="item topic"
            class:active={is({ kind: "topic", id: tp.id })}
            class:drop={topicDrop === tp.id}
            role="button"
            tabindex="0"
            onclick={() => store.setView({ kind: "topic", id: tp.id })}
            onkeydown={(e) => e.key === "Enter" && store.setView({ kind: "topic", id: tp.id })}
            ondblclick={() => {
              renamingTopic = tp.id;
              renameValue = tp.name;
            }}
            ondragover={(e) => {
              if (!carries(e, ALBUM_MIME)) return;
              e.preventDefault();
              topicDrop = tp.id;
            }}
            ondragleave={() => (topicDrop = null)}
            ondrop={(e) => dropAlbum(e, tp.id)}>
            <button class="icon caret" aria-label={open ? t("Collapse") : t("Expand")} onclick={(e) => {
              e.stopPropagation();
              toggleTopic(tp.id);
            }}>{open ? "▾" : "▸"}</button>
            <span class="name" title={t("Double-click to rename")}>{tp.name}</span>
            <small>{g.albums.length}</small>
            <button class="icon del" title={t("Delete topic")} onclick={(e) => {
              e.stopPropagation();
              deleteTopic(tp.id, tp.name);
            }}>×</button>
          </div>
        {/if}
        {#if open}
          {#each g.albums as a (a.id)}{@render albumRow(a, true)}{/each}
        {/if}
      {:else}
        {#each g.albums as a (a.id)}{@render albumRow(a, false)}{/each}
      {/if}
    {/each}
    {#if !store.albums.length && newAlbum == null}
      <p class="hint">{t("Drag photos onto an album to add them.")}</p>
    {:else if store.topics.length && !store.albums.some((a) => a.topicId != null)}
      <p class="hint">{t("Drag an album onto a topic to file it there.")}</p>
    {/if}
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
  .item > span:first-child, .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .item:hover { background: var(--hover); }
  .item.active { background: var(--accent-bg); color: var(--accent-fg); }
  .item.drop { outline: 2px dashed var(--accent); }
  small { color: var(--muted); font-size: 11px; }
  .album img, .nocover { width: 24px; height: 24px; border-radius: 4px; object-fit: cover; background: var(--cell); flex-shrink: 0; }
  .icon { background: none; border: none; color: var(--muted); padding: 0 4px; font-size: 14px; cursor: pointer; }
  .del { visibility: hidden; }
  .album:hover .del, .topic:hover .del { visibility: visible; }
  .heading.drop { outline: 2px dashed var(--accent); border-radius: 6px; }
  .topic { font-weight: 600; gap: 4px; }
  .caret { width: 18px; padding: 0; font-size: 11px; }
  .nested { padding-left: 26px; }
  input.nested { width: calc(100% - 18px); margin-left: 18px; }
  .addwrap { position: relative; }
  .menu { position: absolute; right: 0; top: 100%; z-index: 10; display: flex; flex-direction: column; min-width: 130px;
    padding: 4px; background: var(--panel); border: 1px solid var(--border); border-radius: 6px;
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.35); text-transform: none; letter-spacing: 0; }
  .menu button { border: none; background: none; text-align: left; padding: 6px 10px; font-size: 13px; }
  .menu button:hover { background: var(--hover); }
  .inline { width: 100%; margin: 2px 0; }
  .hint { color: var(--muted); font-size: 12px; padding: 0 8px; margin: 4px 0; }
  .tags { flex: 1; }
  .footer { display: flex; align-items: center; gap: 6px; padding: 10px 14px; border-top: 1px solid var(--border);
    font-size: 12px; color: var(--muted); }
  .footer .lang { padding-inline: 4px; }
  .footer span { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
