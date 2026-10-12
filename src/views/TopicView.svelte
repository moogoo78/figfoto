<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { src } from "../lib/api";
  import { store } from "../lib/store.svelte";

  let { id }: { id: number } = $props();
  const topic = $derived(store.topics.find((tp) => tp.id === id));
  const albums = $derived(store.albums.filter((a) => a.topicId === id));
</script>

<section class="topic">
  <header>
    <h1>{topic?.name ?? ""}</h1>
    <span class="count">{t("{n} album(s)", { n: albums.length })}</span>
  </header>

  <div class="body">
    {#each albums as a (a.id)}
      <button class="tile" onclick={() => store.setView({ kind: "album", id: a.id })}>
        {#if a.cover}<img src={src(a.cover)} alt="" />{:else}<span class="nocover"></span>{/if}
        <strong title={a.name}>{a.name}</strong>
        <small>{t("{n} photos", { n: a.count })}</small>
      </button>
    {:else}
      <p class="empty">{t("Drag albums here in the sidebar, or choose a topic on the album's page.")}</p>
    {/each}
  </div>
</section>

<style>
  .topic { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .body { flex: 1; overflow-y: auto; padding: 16px; display: grid; gap: 16px; align-content: start;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); }
  .tile { display: flex; flex-direction: column; gap: 4px; padding: 0; border: none; background: none;
    text-align: left; cursor: pointer; min-width: 0; }
  .tile img, .nocover { width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: 8px; background: var(--cell); }
  .tile:hover img, .tile:hover .nocover { filter: brightness(1.12); outline: 2px solid var(--accent); }
  .tile strong { font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tile small { color: var(--muted); font-size: 12px; }
  .empty { grid-column: 1 / -1; color: var(--muted); font-size: 13px; }
</style>
