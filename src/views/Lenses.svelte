<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { api } from "../lib/api";
  import { store } from "../lib/store.svelte";

  let newName = $state("");

  function add() {
    const name = newName.trim();
    if (!name) return;
    newName = "";
    store.mutate(() => api.addLens(name), t("Lens added"));
  }

  function rename(old: string, input: HTMLInputElement) {
    const name = input.value.trim();
    if (!name) input.value = old;
    else if (name !== old) store.mutate(() => api.renameLens(old, name));
  }
</script>

<section class="lenses">
  <header>
    <h1>{t("Lenses")}</h1>
  </header>

  <div class="body">
    <p class="hint">
      {t("Manual and legacy lenses the camera can't record. Saved here for the whole library, they're suggested wherever you set a lens.")}
    </p>

    <form class="add" onsubmit={(e) => { e.preventDefault(); add(); }}>
      <input bind:value={newName} placeholder={t("Add lens (e.g. Helios 44-2 58mm f/2)")}
        onkeydown={(e) => e.stopPropagation()} />
      <button class="primary" disabled={!newName.trim()}>{t("Add")}</button>
    </form>

    {#each store.lenses as lens (lens.name)}
      <article class="lens">
        <input class="name" value={lens.name} title={t("Rename — photos set to this lens follow")}
          onblur={(e) => rename(lens.name, e.currentTarget)}
          onkeydown={(e) => {
            if (e.key === "Enter") e.currentTarget.blur();
            if (e.key === "Escape") { e.currentTarget.value = lens.name; e.currentTarget.blur(); }
            e.stopPropagation();
          }} />
        <span class="n">{t("{n} photo(s)", { n: lens.photos })}</span>
        <button class="danger" title={t("Remove from list (photos keep their lens)")} aria-label={t("Remove from list (photos keep their lens)")}
          onclick={() => store.mutate(() => api.removeLens(lens.name))}>✕</button>
      </article>
    {:else}
      <p class="empty">{t("No saved lenses yet.")}</p>
    {/each}
  </div>
</section>

<style>
  .lenses { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .body { flex: 1; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 8px; max-width: 640px; }
  .hint, .empty { color: var(--muted); font-size: 13px; line-height: 1.6; margin: 0 0 6px; }
  .add { display: flex; gap: 8px; margin-bottom: 8px; }
  .add input { flex: 1; }
  .lens { display: flex; align-items: center; gap: 12px; padding: 8px 10px; border: 1px solid var(--border);
    border-radius: 8px; background: var(--panel); }
  .name { flex: 1; min-width: 0; }
  .n { color: var(--muted); font-size: 13px; white-space: nowrap; }
</style>
