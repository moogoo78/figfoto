<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { api } from "../lib/api";
  import AlbumTiles from "../lib/AlbumTiles.svelte";
  import { store } from "../lib/store.svelte";

  /** Name being typed for a new album. */
  let newAlbum = $state<string | null>(null);

  const top = $derived(store.albumsIn(null));

  async function createAlbum() {
    const name = newAlbum?.trim();
    newAlbum = null;
    if (!name) return;
    let id = 0;
    await store.mutate(async () => (id = await api.createAlbum(name)));
    if (id) store.setView({ kind: "album", id });
  }

  const focus = (el: HTMLInputElement) => {
    el.focus();
    el.select();
  };
</script>

<section class="albums">
  <header>
    <h1>{t("Albums")}</h1>
    <span class="count">{t("{n} album(s)", { n: store.albums.length })}</span>
    <span class="grow"></span>
    {#if newAlbum != null}
      <input class="name-input" placeholder={t("Album name")} bind:value={newAlbum} use:focus onblur={createAlbum}
        onkeydown={(e) => {
          if (e.key === "Enter") createAlbum();
          if (e.key === "Escape") newAlbum = null;
        }} />
    {:else}
      <button class="primary" onclick={() => (newAlbum = "")}>＋ {t("New album")}</button>
    {/if}
  </header>

  <div class="body">
    {#if top.length}
      <AlbumTiles albums={top} />
      <p class="hint">{t("Drag an album onto another to put it inside; open an album to see the albums inside it.")}</p>
    {:else}
      <p class="hint">{t("No albums yet. Make one here, or from photos you select.")}</p>
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
  .body { flex: 1; overflow-y: auto; padding: 16px; }
  .hint { color: var(--muted); font-size: 13px; margin: 16px 0 0; }
</style>
