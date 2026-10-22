<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { api, src } from "../lib/api";
  import { store } from "../lib/store.svelte";
  import type { Album, Topic } from "../lib/types";

  /** Name being typed for a new album or topic. */
  let newAlbum = $state<string | null>(null);
  let newTopic = $state<string | null>(null);
  /** Album or topic being renamed, and its name so far. */
  let renaming = $state<{ kind: "album" | "topic"; id: number } | null>(null);
  let renameValue = $state("");
  /** Where a dragged album would land: a topic id, or "none" (out of its topic). */
  let topicDrop = $state<number | "none" | null>(null);
  /** Album that dragged photos would be added to. */
  let photoDrop = $state<number | null>(null);

  const ALBUM_MIME = "application/x-foto-album";
  const PHOTOS_MIME = "application/x-foto-ids";
  const carries = (e: DragEvent, mime: string) => !!e.dataTransfer?.types.includes(mime);

  const groups = $derived(store.albumsByTopic());

  async function createAlbum() {
    const name = newAlbum?.trim();
    newAlbum = null;
    if (!name) return;
    let id = 0;
    await store.mutate(async () => (id = await api.createAlbum(name)));
    if (id) store.setView({ kind: "album", id });
  }

  async function createTopic() {
    const name = newTopic?.trim();
    newTopic = null;
    if (name) store.mutate(() => api.createTopic(name));
  }

  function startRename(kind: "album" | "topic", id: number, name: string) {
    renaming = { kind, id };
    renameValue = name;
  }

  function finishRename() {
    const [r, name] = [renaming, renameValue.trim()];
    renaming = null;
    if (!r || !name) return;
    store.mutate(() => (r.kind === "album" ? api.renameAlbum(r.id, name) : api.renameTopic(r.id, name)));
  }

  async function deleteAlbum(a: Album) {
    if (await ask(t('Delete album "{name}"? Photos stay in the library.', { name: a.name }), { title: t("Delete album"), kind: "warning" })) {
      store.mutate(() => api.deleteAlbum(a.id));
    }
  }

  async function deleteTopic(tp: Topic) {
    if (await ask(t('Delete topic "{name}"? Its albums stay and move out of the topic.', { name: tp.name }), { title: t("Delete topic"), kind: "warning" })) {
      store.mutate(() => api.deleteTopic(tp.id));
    }
  }

  function dropAlbum(e: DragEvent, topicId: number | null) {
    topicDrop = null;
    const raw = e.dataTransfer?.getData(ALBUM_MIME);
    if (!raw) return;
    e.preventDefault();
    store.mutate(() => api.setAlbumTopic([Number(raw)], topicId));
  }

  function dropPhotos(e: DragEvent, albumId: number) {
    photoDrop = null;
    const raw = e.dataTransfer?.getData(PHOTOS_MIME);
    if (!raw) return;
    e.preventDefault();
    const ids = JSON.parse(raw) as number[];
    store.mutate(() => api.addToAlbum(albumId, ids), t("Added {n} photo(s)", { n: ids.length }));
  }

  const focus = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };
  const renameKeys = (e: KeyboardEvent) => {
    if (e.key === "Enter") finishRename();
    if (e.key === "Escape") renaming = null;
  };
</script>

<section class="albums">
  <header>
    <h1>{t("Albums")}</h1>
    <span class="count">{t("{n} album(s)", { n: store.albums.length })}</span>
    <span class="grow"></span>
    {#if newTopic != null}
      <input class="name-input" placeholder={t("Topic name (e.g. Travel)")} bind:value={newTopic} use:focus onblur={createTopic}
        onkeydown={(e) => {
          if (e.key === "Enter") createTopic();
          if (e.key === "Escape") newTopic = null;
        }} />
    {:else if newAlbum != null}
      <input class="name-input" placeholder={t("Album name")} bind:value={newAlbum} use:focus onblur={createAlbum}
        onkeydown={(e) => {
          if (e.key === "Enter") createAlbum();
          if (e.key === "Escape") newAlbum = null;
        }} />
    {:else}
      <button onclick={() => (newTopic = "")}>＋ {t("New topic")}</button>
      <button class="primary" onclick={() => (newAlbum = "")}>＋ {t("New album")}</button>
    {/if}
  </header>

  <div class="body">
    {#each groups as g (g.topic?.id ?? 0)}
      {@const tp = g.topic}
      {#if tp || store.topics.length}
        {@const key = tp?.id ?? "none"}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <h2 class:drop={topicDrop === key}
          ondragover={(e) => {
            if (!carries(e, ALBUM_MIME)) return;
            e.preventDefault();
            topicDrop = key;
          }}
          ondragleave={() => topicDrop === key && (topicDrop = null)}
          ondrop={(e) => dropAlbum(e, tp?.id ?? null)}>
          {#if tp && renaming?.kind === "topic" && renaming.id === tp.id}
            <input class="name-input" bind:value={renameValue} use:focus onblur={finishRename} onkeydown={renameKeys} />
          {:else if tp}
            <button class="link name" title={t("Double-click to rename")} onclick={() => store.setView({ kind: "topic", id: tp.id })}
              ondblclick={() => startRename("topic", tp.id, tp.name)}>{tp.name}</button>
          {:else}
            <span class="name none">{t("No topic")}</span>
          {/if}
          <small>{g.albums.length}</small>
          {#if tp}
            <button class="icon pin" class:on={tp.shortcut != null}
              title={tp.shortcut != null ? t("Remove from shortcuts") : t("Add to shortcuts")}
              onclick={() => store.mutate(() => api.setShortcut("topic", tp.id, tp.shortcut == null))}>{tp.shortcut != null ? "★" : "☆"}</button>
            <button class="icon del" title={t("Delete topic")} onclick={() => deleteTopic(tp)}>×</button>
          {/if}
        </h2>
      {/if}
      <div class="grid">
        {#each g.albums as a (a.id)}
          <div class="tile" class:drop={photoDrop === a.id} role="button" tabindex="0" draggable="true"
            ondragstart={(e) => e.dataTransfer?.setData(ALBUM_MIME, String(a.id))}
            onclick={() => store.setView({ kind: "album", id: a.id })}
            onkeydown={(e) => e.key === "Enter" && store.setView({ kind: "album", id: a.id })}
            ondragover={(e) => {
              if (!carries(e, PHOTOS_MIME)) return;
              e.preventDefault();
              photoDrop = a.id;
            }}
            ondragleave={() => photoDrop === a.id && (photoDrop = null)}
            ondrop={(e) => dropPhotos(e, a.id)}>
            <span class="cover">
              {#if a.cover}<img src={src(a.cover)} alt="" draggable="false" />{:else}<span class="nocover"></span>{/if}
              <button class="icon pin" class:on={a.shortcut != null}
                title={a.shortcut != null ? t("Remove from shortcuts") : t("Add to shortcuts")}
                onclick={(e) => {
                  e.stopPropagation();
                  store.mutate(() => api.setShortcut("album", a.id, a.shortcut == null));
                }}>{a.shortcut != null ? "★" : "☆"}</button>
              <button class="icon del" title={t("Delete album")} onclick={(e) => {
                e.stopPropagation();
                deleteAlbum(a);
              }}>×</button>
            </span>
            {#if renaming?.kind === "album" && renaming.id === a.id}
              <input class="tile-input" bind:value={renameValue} use:focus onblur={finishRename} onkeydown={renameKeys}
                onclick={(e) => e.stopPropagation()} />
            {:else}
              <!-- svelte-ignore a11y_no_static_element_interactions -->
              <strong title={t("Double-click to rename")} ondblclick={(e) => {
                e.stopPropagation();
                startRename("album", a.id, a.name);
              }}>{a.name}</strong>
            {/if}
            <small>{t("{n} photos", { n: a.count })}</small>
          </div>
        {:else}
          {#if tp}<p class="empty">{t("Drag albums here to file them in this topic.")}</p>{/if}
        {/each}
      </div>
    {/each}
    {#if !store.albums.length}
      <p class="empty">{t("No albums yet. Make one here, or from photos you select.")}</p>
    {:else if store.topics.length}
      <p class="hint">{t("Drag an album onto a topic to file it there.")}</p>
    {/if}
  </div>
</section>

<style>
  .albums { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .grow { flex: 1; }
  /* Explicit widths: some WebKitGTK builds otherwise squeeze a text field in a flex row to a sliver. */
  .name-input { width: 240px; flex-shrink: 0; }
  .tile-input { width: 100%; }
  .body { flex: 1; overflow-y: auto; padding: 8px 16px 16px; }
  h2 { display: flex; align-items: center; gap: 8px; margin: 12px 0 8px; padding: 4px 6px; font-size: 15px; border-radius: 6px; }
  h2.drop, .tile.drop .cover { outline: 2px dashed var(--accent); outline-offset: 2px; }
  h2 .name { font-size: 15px; font-weight: 600; color: inherit; }
  h2 .none { color: var(--muted); font-weight: 400; }
  h2 small { color: var(--muted); font-size: 12px; font-weight: 400; }
  .grid { display: grid; gap: 16px; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); }
  .tile { display: flex; flex-direction: column; gap: 4px; min-width: 0; cursor: pointer; }
  .cover { position: relative; display: block; }
  .cover img, .nocover { display: block; width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; background: var(--cell); }
  .tile:hover img, .tile:hover .nocover { filter: brightness(1.12); outline: 2px solid var(--accent); }
  .tile strong { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tile small { color: var(--muted); font-size: 12px; }
  .icon { background: none; border: none; color: var(--muted); padding: 0 4px; font-size: 15px; cursor: pointer; }
  .cover .icon { position: absolute; top: 6px; padding: 2px 6px; border-radius: 6px; background: rgb(0 0 0 / 0.55); color: #fff; }
  .cover .pin { left: 6px; }
  .cover .del { right: 6px; }
  .pin.on { color: var(--star); visibility: visible; }
  .pin, .del { visibility: hidden; }
  .tile:hover .icon, h2:hover .icon { visibility: visible; }
  .empty, .hint { grid-column: 1 / -1; color: var(--muted); font-size: 13px; margin: 4px 0; }
</style>
