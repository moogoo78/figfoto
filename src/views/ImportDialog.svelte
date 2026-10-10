<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { errorText } from "../lib/format";
  import SourceInput from "../lib/SourceInput.svelte";
  import { store } from "../lib/store.svelte";

  let source = $state("");
  /** Optional label for where these photos came from (e.g. LINE). */
  let origin = $state("");
  /** Organized folders: each folder holding photos becomes an album. */
  let folderAlbums = $state(false);
  /** Also bring back photos that were deleted from the library (trash emptied) before. */
  let includeDeleted = $state(false);
  let started = $state(false);

  // "Import again" from the history view prefills the folder.
  $effect(() => {
    if (store.importOpen && store.importPreset && !started) {
      source = store.importPreset.dir;
      folderAlbums = store.importPreset.folderAlbums;
      store.importPreset = null;
    }
  });

  const p = $derived(started ? store.progress : null);
  const running = $derived(started && !p?.finished);
  const pct = $derived(p && p.total ? Math.round((p.done / p.total) * 100) : 0);

  async function pick() {
    const dir = await open({ directory: true, title: t("Choose a folder to import") });
    if (typeof dir === "string") source = dir;
  }

  async function start() {
    store.progress = null;
    try {
      await api.startImport(source, origin, folderAlbums, includeDeleted);
      started = true;
    } catch (e) {
      store.toast(errorText(e), true);
    }
  }

  function close() {
    if (running) return;
    started = false;
    includeDeleted = false;
    store.importOpen = false;
  }
</script>

<svelte:window onkeydown={(e) => store.importOpen && e.key === "Escape" && close()} />

{#if store.importOpen}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" role="dialog" aria-modal="true" aria-label={t("Import photos")}>
      <h2>{t("Import photos")}</h2>

      {#if !started}
        <div class="kind" role="radiogroup" aria-label={t("Import type")}>
          <label class:on={!folderAlbums}>
            <input type="radio" name="kind" value={false} bind:group={folderAlbums} />
            <strong>{t("Camera / SD card")}</strong>
            <small>{t("Sorted by date only")}</small>
          </label>
          <label class:on={folderAlbums}>
            <input type="radio" name="kind" value={true} bind:group={folderAlbums} />
            <strong>{t("Organized folders")}</strong>
            <small>{t("Each folder becomes an album")}</small>
          </label>
        </div>
        {#if folderAlbums}
          <p class="hint">
            {t("Each folder that holds photos becomes an album named after it (an album with that name is reused). Photos already in the library are added to the album without a second copy. Photos directly in the chosen folder aren't put in an album.")}
          </p>
        {/if}
        <p class="hint">
          {t("Photos are copied into your library under")} <code>originals/YYYY/MM/DD</code>{t(". The source folder is not changed. Files already in the library are skipped.")}
        </p>
        <div class="pick">
          <input readonly value={source} placeholder={t("No folder selected")} onclick={pick} />
          <button onclick={pick}>{t("Browse…")}</button>
        </div>
        <label class="origin">
          <span>{t("Source")} <small>{t("(optional — e.g. photos saved from LINE or Facebook)")}</small></span>
          <SourceInput bind:value={origin} submitOnEnter={false} placeholder={t("Camera / phone detected automatically")} />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={includeDeleted} />
          <span>{t("Include previously deleted photos")} <small>{t("(normally, photos you deleted from the library are skipped)")}</small></span>
        </label>
        <div class="buttons">
          <button onclick={close}>{t("Cancel")}</button>
          <button class="primary" disabled={!source} onclick={start}>{t("Import")}</button>
        </div>
      {:else}
        <div class="bar"><div style:width="{pct}%"></div></div>
        <div class="stats">
          {#if !p || p.scanning}
            <span>{t("Scanning… {n} photos found", { n: p?.total ?? 0 })}</span>
          {:else}
            <span>{p.done} / {p.total}</span>
          {/if}
          <span class="ok">{t("{n} added", { n: p?.added ?? 0 })}</span>
          <span>{t("{n} already in library", { n: p?.skipped ?? 0 })}</span>
          {#if p?.skippedDeleted}<span>{t("{n} previously deleted", { n: p.skippedDeleted })}</span>{/if}
          {#if p?.albums}<span>{t("{n} album(s)", { n: p.albums })}</span>{/if}
          {#if p?.failed}<span class="bad">{t("{n} failed", { n: p.failed })}</span>{/if}
        </div>
        {#if running}
          <p class="current" title={p?.current}>{p?.current || t("Scanning folder…")}</p>
        {:else if p}
          <p><strong>{p.cancelled ? t("Import cancelled.") : t("Import finished.")}</strong></p>
          {#if p.errors.length}
            <details open={p.errors.length < 5}>
              <summary>
                {t("{n} error(s)", { n: p.failed })}{#if p.errors.length < p.failed} — {t("showing the last {n}; see Import history for all", { n: p.errors.length })}{/if}
              </summary>
              <ul class="errors">
                {#each p.errors as err}<li><code>{err.path}</code><br />{err.error}</li>{/each}
              </ul>
            </details>
          {/if}
        {/if}
        <div class="buttons">
          {#if running}
            <button onclick={() => api.cancelImport()}>{t("Stop")}</button>
          {:else}
            <button onclick={() => { started = false; source = ""; origin = ""; includeDeleted = false; }}>{t("Import more")}</button>
            <button class="primary" onclick={close}>{t("Done")}</button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop { position: fixed; inset: 0; z-index: 60; background: rgb(0 0 0 / 0.5); display: flex; align-items: center; justify-content: center; }
  .dialog { width: min(560px, calc(100vw - 32px)); max-height: 80vh; overflow-y: auto; padding: 20px 22px;
    background: var(--panel); border: 1px solid var(--border); border-radius: 12px; box-shadow: 0 20px 60px rgb(0 0 0 / 0.5); }
  h2 { margin: 0 0 12px; font-size: 18px; }
  .kind { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; margin-bottom: 4px; }
  .kind label { display: flex; flex-direction: column; gap: 2px; padding: 10px 12px; border: 1px solid var(--border);
    border-radius: 8px; cursor: pointer; font-size: 13px; }
  .kind label.on { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 10%, transparent); }
  .kind input { position: absolute; opacity: 0; pointer-events: none; }
  .kind label:has(input:focus-visible) { outline: 2px solid var(--accent); outline-offset: 2px; }
  .kind small { color: var(--muted); }
  .hint { color: var(--muted); font-size: 13px; line-height: 1.5; }
  .pick { display: flex; gap: 8px; }
  .pick input { flex: 1; cursor: pointer; }
  .origin { display: flex; flex-direction: column; gap: 6px; margin-top: 14px; font-size: 13px; }
  .origin small { color: var(--muted); }
  .origin :global(input) { width: 100%; }
  .check { display: flex; align-items: baseline; gap: 8px; margin-top: 14px; font-size: 13px; cursor: pointer; }
  .check small { color: var(--muted); }
  .buttons { display: flex; justify-content: flex-end; gap: 8px; margin-top: 18px; }
  .bar { height: 8px; border-radius: 4px; background: var(--cell); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width 0.2s; }
  .stats { display: flex; gap: 16px; margin-top: 10px; font-size: 13px; }
  .ok { color: var(--ok); }
  .bad { color: var(--danger); }
  .current { font-size: 12px; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .errors { font-size: 12px; max-height: 200px; overflow-y: auto; padding-left: 18px; }
  .errors li { margin-bottom: 6px; }
</style>
