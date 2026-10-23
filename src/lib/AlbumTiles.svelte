<script lang="ts">
  import { t } from "./i18n.svelte";
  import { api, src } from "./api";
  import { canDrop, deleteAlbum, dragAlbum, drop, endDrag } from "./albumDrop";
  import { store } from "./store.svelte";
  import type { Album } from "./types";

  /** Cover tiles for albums: open, rename (double-click), pin, delete, drop photos on one, or
   * drag one onto another to put it inside. */
  let {
    albums,
    from = null,
    paths = false,
  }: {
    albums: Album[];
    /** The album whose page this is: photos dropped on a tile move out of it. Elsewhere they're added. */
    from?: number | null;
    /** Show the category each album is filed in. */
    paths?: boolean;
  } = $props();

  /** Album being renamed, and its name so far. */
  let renaming = $state<number | null>(null);
  let renameValue = $state("");
  /** Tile something is dragged over. */
  let dropOn = $state<number | null>(null);

  function finishRename() {
    const [id, name] = [renaming, renameValue.trim()];
    renaming = null;
    if (id != null && name) store.mutate(() => api.renameAlbum(id, name));
  }

  const focus = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };
</script>

<div class="grid">
  {#each albums as a (a.id)}
    <div class="tile" class:drop={dropOn === a.id} role="button" tabindex="0" draggable="true"
      ondragstart={(e) => dragAlbum(e, a.id)}
      ondragend={endDrag}
      onclick={() => store.setView({ kind: "album", id: a.id })}
      onkeydown={(e) => e.key === "Enter" && store.setView({ kind: "album", id: a.id })}
      ondragover={(e) => {
        if (!canDrop(e, a.id)) return;
        e.preventDefault();
        dropOn = a.id;
      }}
      ondragleave={() => dropOn === a.id && (dropOn = null)}
      ondrop={(e) => {
        dropOn = null;
        drop(e, a.id, from);
      }}>
      <span class="cover" class:stack={a.children > 0}>
        {#if a.cover}<img src={src(a.cover)} alt="" draggable="false" loading="lazy" />{:else}<span class="nocover"></span>{/if}
        <button class="icon pin" class:on={a.shortcut != null}
          title={a.shortcut != null ? t("Remove from shortcuts") : t("Add to shortcuts")}
          onclick={(e) => {
            e.stopPropagation();
            store.mutate(() => api.setShortcut(a.id, a.shortcut == null));
          }}>{a.shortcut != null ? "★" : "☆"}</button>
        <button class="icon del" title={t("Delete album")} onclick={(e) => {
          e.stopPropagation();
          deleteAlbum(a);
        }}>×</button>
      </span>
      {#if renaming === a.id}
        <input class="tile-input" bind:value={renameValue} use:focus onblur={finishRename} onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => {
            if (e.key === "Enter") finishRename();
            if (e.key === "Escape") renaming = null;
            e.stopPropagation();
          }} />
      {:else}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <strong title={t("Double-click to rename")} ondblclick={(e) => {
          e.stopPropagation();
          renaming = a.id;
          renameValue = a.name;
        }}>{a.name}</strong>
      {/if}
      <small>
        {#if a.children}{t(a.role === "category" ? "{n} album(s)" : "{n} section(s)", { n: a.children })} · {/if}{t("{n} photos", { n: a.total })}
      </small>
      {#if paths && a.parentId != null}<small class="path">{store.albumLabel(a.parentId)}</small>{/if}
    </div>
  {/each}
</div>

<style>
  .grid { display: grid; gap: 16px; grid-template-columns: repeat(auto-fill, minmax(160px, 1fr)); }
  .tile { display: flex; flex-direction: column; gap: 4px; min-width: 0; cursor: pointer; }
  .tile-input { width: 100%; }
  .cover { position: relative; display: block; }
  .cover img, .nocover { display: block; width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; background: var(--cell); }
  /* An album with albums inside looks like a stack. */
  .stack img, .stack .nocover { box-shadow: 4px -4px 0 -1px var(--panel), 4px -4px 0 0 var(--border), 8px -8px 0 -2px var(--panel), 8px -8px 0 -1px var(--border); }
  .tile:hover img, .tile:hover .nocover { filter: brightness(1.12); outline: 2px solid var(--accent); }
  .tile.drop .cover { outline: 2px dashed var(--accent); outline-offset: 2px; border-radius: 8px; }
  .tile strong { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tile small { color: var(--muted); font-size: 12px; }
  .tile .path { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .icon { position: absolute; top: 6px; padding: 2px 6px; border: none; border-radius: 6px; background: rgb(0 0 0 / 0.55);
    color: #fff; font-size: 15px; cursor: pointer; visibility: hidden; }
  .pin { left: 6px; }
  .del { right: 6px; }
  .pin.on { color: var(--star); visibility: visible; }
  .tile:hover .icon { visibility: visible; }
</style>
