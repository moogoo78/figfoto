<script lang="ts">
  import { t } from "./i18n.svelte";
  import { api } from "./api";
  import { store } from "./store.svelte";
  import type { Album } from "./types";

  /** A "Move to…" menu listing every album the given one can go inside, and the top level. */
  let { album }: { album: Album } = $props();
  /** It can't go inside itself or one inside it. */
  const blocked = $derived(new Set([album.id, ...store.albumsInside(album.id).map((a) => a.id)]));

  function move(e: Event & { currentTarget: HTMLSelectElement }) {
    const v = e.currentTarget.value;
    e.currentTarget.value = "";
    if (!v) return;
    const parentId = v === "top" ? null : Number(v);
    if (parentId !== album.parentId) store.mutate(() => api.moveAlbum(album.id, parentId));
  }
</script>

<select class="move" title={t("Move to…")} value="" onchange={move}>
  <option value="" disabled>{t("Move to…")}</option>
  <option value="top" disabled={album.parentId == null}>{t("Top level")}</option>
  {#each store.albums as a (a.id)}
    <option value={a.id} disabled={blocked.has(a.id) || a.id === album.parentId}>{"  ".repeat(a.depth)}{a.name}</option>
  {/each}
</select>

<style>
  .move { max-width: 160px; }
</style>
