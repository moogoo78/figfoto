<script lang="ts">
  import { t } from "./i18n.svelte";
  import { api, src } from "./api";
  import { canDrop, deleteAlbum, dragAlbum, drop, endDrag } from "./albumDrop";
  import { store } from "./store.svelte";
  import type { Album } from "./types";

  /** Every album as an indented tree: open, collapse, rename (double-click), pin, delete, drop
   * photos on one, or drag one onto another (or the top level) to move it. */

  // Collapsed albums, remembered per library in this browser only.
  const collapseKey = $derived(`figfoto.albumTree.collapsed:${store.root}`);
  let collapsed = $state<number[]>([]);
  $effect(() => {
    try {
      collapsed = JSON.parse(localStorage.getItem(collapseKey) ?? "[]");
    } catch {
      collapsed = [];
    }
  });
  function saveCollapsed(ids: number[]) {
    collapsed = ids;
    try {
      localStorage.setItem(collapseKey, JSON.stringify(collapsed));
    } catch {
      // Not remembered; fine.
    }
  }
  function toggle(id: number) {
    saveCollapsed(collapsed.includes(id) ? collapsed.filter((c) => c !== id) : [...collapsed, id]);
  }

  const byId = $derived(new Map(store.albums.map((a) => [a.id, a])));
  const parents = $derived(store.albums.filter((a) => a.children > 0).map((a) => a.id));
  const allCollapsed = $derived(parents.length > 0 && parents.every((id) => collapsed.includes(id)));
  /** Albums not hidden under a collapsed one, in tree order. */
  const rows = $derived(
    store.albums.filter((a) => {
      for (let p = a.parentId; p != null; p = byId.get(p)?.parentId ?? null) if (collapsed.includes(p)) return false;
      return true;
    }),
  );

  /** Album being renamed, and its name so far. */
  let renaming = $state<number | null>(null);
  let renameValue = $state("");
  /** Row something is dragged over; 0 is the top level. */
  let dropOn = $state<number | null>(null);

  function finishRename() {
    const [id, name] = [renaming, renameValue.trim()];
    renaming = null;
    if (id != null && name) store.mutate(() => api.renameAlbum(id, name));
  }

  const open = (a: Album) => store.setView({ kind: "album", id: a.id });

  const focus = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };
</script>

<div class="tree">
  <div class="bar">
    <div class="top" class:drop={dropOn === 0} role="region" aria-label={t("Top level")}
      ondragover={(e) => {
        if (!canDrop(e, null)) return;
        e.preventDefault();
        dropOn = 0;
      }}
      ondragleave={() => dropOn === 0 && (dropOn = null)}
      ondrop={(e) => {
        dropOn = null;
        drop(e, null);
      }}>{t("Drop an album here to move it to the top level")}</div>
    {#if parents.length}
      <button onclick={() => saveCollapsed(allCollapsed ? [] : parents)}>{allCollapsed ? t("Expand all") : t("Collapse all")}</button>
    {/if}
  </div>

  {#each rows as a (a.id)}
    <div class="row" class:drop={dropOn === a.id} style:padding-left="{8 + a.depth * 20}px" role="button" tabindex="0"
      draggable="true"
      ondragstart={(e) => dragAlbum(e, a.id)}
      ondragend={endDrag}
      onclick={() => open(a)}
      onkeydown={(e) => {
        if (e.key === "Enter") open(a);
        if (a.children && (e.key === "ArrowLeft" || e.key === "ArrowRight")) {
          if (collapsed.includes(a.id) === (e.key === "ArrowRight")) toggle(a.id);
        }
      }}
      ondragover={(e) => {
        if (!canDrop(e, a.id)) return;
        e.preventDefault();
        dropOn = a.id;
      }}
      ondragleave={() => dropOn === a.id && (dropOn = null)}
      ondrop={(e) => {
        dropOn = null;
        drop(e, a.id);
      }}>
      {#if a.children}
        <button class="caret" title={collapsed.includes(a.id) ? t("Expand") : t("Collapse")} onclick={(e) => {
          e.stopPropagation();
          toggle(a.id);
        }}>{collapsed.includes(a.id) ? "▸" : "▾"}</button>
      {:else}
        <span class="caret"></span>
      {/if}
      {#if a.cover}<img src={src(a.cover)} alt="" draggable="false" />{:else}<span class="nocover"></span>{/if}
      {#if renaming === a.id}
        <input class="name-input" bind:value={renameValue} use:focus onblur={finishRename} onclick={(e) => e.stopPropagation()}
          onkeydown={(e) => {
            if (e.key === "Enter") finishRename();
            if (e.key === "Escape") renaming = null;
            e.stopPropagation();
          }} />
      {:else}
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <span class="name" title={t("Double-click to rename")} ondblclick={(e) => {
          e.stopPropagation();
          renaming = a.id;
          renameValue = a.name;
        }}>{a.name}</span>
      {/if}
      {#if a.role !== "album"}<span class="role">{a.role === "category" ? t("Category") : t("Section")}</span>{/if}
      <small>
        {#if a.children}{t(a.role === "category" ? "{n} album(s)" : "{n} section(s)", { n: a.children })} · {/if}{t("{n} photos", { n: a.total })}
      </small>
      <span class="grow"></span>
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
    </div>
  {/each}
</div>

<style>
  .tree { display: flex; flex-direction: column; gap: 2px; }
  .bar { display: flex; align-items: center; gap: 10px; margin-bottom: 8px; }
  .top { flex: 1; padding: 6px 10px; border: 1px dashed var(--border); border-radius: 6px; color: var(--muted); font-size: 12px; }
  .top.drop { border-color: var(--accent); color: var(--fg); }
  .row { display: flex; align-items: center; gap: 8px; min-width: 0; padding: 4px 8px; border-radius: 6px; cursor: pointer; }
  .row:hover { background: var(--hover); }
  .row.drop { outline: 2px dashed var(--accent); outline-offset: -2px; }
  .caret { width: 18px; flex-shrink: 0; padding: 0; border: none; background: none; color: var(--muted); cursor: pointer; }
  img, .nocover { width: 32px; height: 32px; flex-shrink: 0; object-fit: cover; border-radius: 4px; background: var(--cell); }
  .name { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Explicit width: some WebKitGTK builds otherwise squeeze a text field in a flex row to a sliver. */
  .name-input { width: 240px; flex-shrink: 0; }
  small { color: var(--muted); font-size: 12px; white-space: nowrap; }
  .role { padding: 0 6px; border: 1px solid var(--border); border-radius: 4px; color: var(--muted); font-size: 11px; white-space: nowrap; }
  .grow { flex: 1; }
  .icon { padding: 2px 6px; border: none; border-radius: 6px; background: none; color: var(--muted); font-size: 15px;
    cursor: pointer; visibility: hidden; }
  .pin.on { color: var(--star); visibility: visible; }
  .row:hover .icon { visibility: visible; }
</style>
