<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { errorText } from "../lib/format";
  import SourceInput from "../lib/SourceInput.svelte";
  import { store } from "../lib/store.svelte";
  import { place, presetPlan, underAlbum, type FolderPreset } from "../lib/folderPlan";
  import type { FolderInfo, FolderRole } from "../lib/types";

  let source = $state("");
  /** Optional label for where these photos came from (e.g. LINE). */
  let origin = $state("");
  /** Organized folders: each folder becomes a category, an album or a section (inside the one above), part of the folder above, or is skipped. */
  let folderAlbums = $state(false);
  /** The chosen folder's folders that hold photos (organized folders), once scanned. */
  let folders = $state.raw<FolderInfo[] | null>(null);
  let scanning = $state(false);
  let preset = $state<FolderPreset>("one");
  /** What each folder becomes, by path. */
  let roles = $state<Record<string, FolderRole>>({});
  /** Choices saved by an earlier import of the same folder, applied over the preset once scanned. */
  let savedPlan: Record<string, FolderRole> | null = null;
  /** Also bring back photos that were deleted from the library (trash emptied) before. */
  let includeDeleted = $state(false);
  /** One album per day; remembered in this browser only. */
  const DAY_ALBUMS_KEY = "figfoto.import.dayAlbums";
  let dayAlbums = $state(readDayAlbums());
  function readDayAlbums() {
    try {
      return localStorage.getItem(DAY_ALBUMS_KEY) !== "false";
    } catch {
      return true;
    }
  }
  function saveDayAlbums() {
    try {
      localStorage.setItem(DAY_ALBUMS_KEY, String(dayAlbums));
    } catch {
      // Not remembered; fine.
    }
  }
  let started = $state(false);

  // "Import again" from the history view prefills the folder.
  $effect(() => {
    if (store.importOpen && store.importPreset && !started) {
      source = store.importPreset.dir;
      folderAlbums = store.importPreset.folderAlbums;
      preset = store.importPreset.preset;
      savedPlan = store.importPreset.plan;
      store.importPreset = null;
    }
  });

  // Organized folders: scan the chosen folder for its folders once both are set.
  let scannedFor = "";
  $effect(() => {
    if (!folderAlbums || !source || source === scannedFor) return;
    const dir = (scannedFor = source);
    folders = null;
    scanning = true;
    api.scanFolders(dir).then(
      (found) => {
        if (dir !== source) return;
        folders = found;
        roles = { ...presetPlan(found, preset), ...(savedPlan ?? {}) };
        savedPlan = null;
        scanning = false;
      },
      (e) => {
        scanning = false;
        scannedFor = "";
        store.toast(errorText(e), true);
      },
    );
  });

  function applyPreset(p: FolderPreset) {
    preset = p;
    if (folders) roles = presetPlan(folders, p);
  }

  // Inside an album a folder can only be a section, and a section needs an album above.
  $effect(() => {
    for (const f of folders ?? []) {
      const role = roles[f.path];
      const inAlbum = underAlbum(f, roles);
      if (inAlbum && (role === "category" || role === "album")) roles[f.path] = "section";
      if (!inAlbum && role === "section") roles[f.path] = "album";
    }
  });

  const byPath = $derived(new Map((folders ?? []).map((f) => [f.path, f])));
  const places = $derived((folders ?? []).map((f) => place(f, byPath, roles)));
  /** Categories and albums (sections included) the plan makes, and photos it leaves out. */
  const summary = $derived.by(() => {
    const [categories, albums] = [new Set<string>(), new Set<string>()];
    let skipped = 0;
    places.forEach((pl, i) => {
      if (pl.kind === "in") {
        pl.albums.forEach((a, k) => (a.kind === "category" ? categories : albums).add(pl.albums.slice(0, k + 1).map((x) => x.name).join("/")));
      }
      if (pl.kind === "skip") skipped += folders![i].photos;
    });
    return { categories: categories.size, albums: albums.size, skipped };
  });

  /** "Japan › D5", and for a category with day albums "Japan › YYMMDD_Japan". */
  function destination(pl: Extract<ReturnType<typeof place>, { kind: "in" }>) {
    const names = pl.albums.map((a) => a.name);
    const last = pl.albums[pl.albums.length - 1];
    if (last.kind === "category" && dayAlbums) names.push(`YYMMDD_${last.name}`);
    return names.join(" › ");
  }

  const p = $derived(started ? store.progress : null);
  const running = $derived(started && !p?.finished);
  const pct = $derived(p && p.total ? Math.round((p.done / p.total) * 100) : 0);

  async function pick() {
    const dir = await open({ directory: true, title: t("Choose a folder to add photos from") });
    if (typeof dir === "string") source = dir;
  }

  async function start() {
    store.progress = null;
    try {
      await api.startImport(source, origin, folderAlbums, folderAlbums ? $state.snapshot(roles) : null, includeDeleted, dayAlbums);
      started = true;
    } catch (e) {
      store.toast(errorText(e), true);
    }
  }

  function close() {
    if (running) return;
    started = false;
    includeDeleted = false;
    source = "";
    scannedFor = "";
    folders = null;
    store.importOpen = false;
  }
</script>

<svelte:window onkeydown={(e) => store.importOpen && e.key === "Escape" && close()} />

{#if store.importOpen}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" class:wide={folderAlbums && !!folders?.length && !started} role="dialog" aria-modal="true" aria-label={t("Add photos")}>
      <h2>{t("Add photos")}</h2>

      {#if !started}
        <div class="kind" role="radiogroup" aria-label={t("Folder type")}>
          <label class:on={!folderAlbums}>
            <input type="radio" name="kind" value={false} bind:group={folderAlbums} />
            <strong>{t("Camera / SD card")}</strong>
            <small>{t("Sorted by date only")}</small>
          </label>
          <label class:on={folderAlbums}>
            <input type="radio" name="kind" value={true} bind:group={folderAlbums} />
            <strong>{t("Organized folders")}</strong>
            <small>{t("Folders become categories, albums and sections")}</small>
          </label>
        </div>
        <p class="hint">
          {t("Photos are copied into your library under")} <code>originals/YYYY/MM/DD</code>{t(". The source folder is not changed. Files already in the library are skipped.")}
        </p>
        <div class="pick">
          <input readonly value={source} placeholder={t("No folder selected")} onclick={pick} />
          <button onclick={pick}>{t("Browse…")}</button>
        </div>
        {#if folderAlbums && source}
          <div class="plan">
            <div class="presets">
              <span>{t("Make:")}</span>
              {#each [
                ["one", t("One album")],
                ["inside", t("An album per folder inside")],
                ["sections", t("An album per folder inside, with sections")],
                ["every", t("Categories like the folders, albums at the bottom")],
              ] as [value, label] (value)}
                <button class:active={preset === value} onclick={() => applyPreset(value as FolderPreset)}>{label}</button>
              {/each}
            </div>
            {#if scanning}
              <p class="hint">{t("Looking through the folders…")}</p>
            {:else if folders && !folders.length}
              <p class="hint">{t("No photos in this folder.")}</p>
            {:else if folders}
              <p class="hint">{t("Choose what each folder becomes. Categories hold albums; an album holds a day or an event, with sections inside.")}</p>
              <div class="tree">
                {#each folders as f, i (f.path)}
                  {@const pl = places[i]}
                  {@const under = f.depth > 0 && pl.kind === "skip" && roles[f.path] !== "skip"}
                  <div class="folder" class:off={pl.kind === "skip"}>
                    <span class="fname" style:padding-left="{f.depth * 18}px" title={f.path || source}>{f.name}</span>
                    <small title={t("{n} photos here, {m} with the folders inside", { n: f.photos, m: f.total })}>{f.total}</small>
                    <select bind:value={roles[f.path]} disabled={under}>
                      {#if underAlbum(f, roles)}
                        <option value="section">{t("Section")}</option>
                      {:else}
                        <option value="category">{t("Category")}</option>
                        <option value="album">{t("Album")}</option>
                      {/if}
                      <option value="parent">{f.depth > 0 ? t("Into folder above") : t("No album")}</option>
                      {#if f.depth > 0}<option value="skip">{t("Skip")}</option>{/if}
                    </select>
                    <span class="dest">
                      <!-- Only folders that hold photos themselves say where those go. -->
                      {#if pl.kind === "skip"}{t("not added")}
                      {:else if f.photos && pl.kind === "loose"}→ {t("no album")}
                      {:else if f.photos && pl.kind === "in"}→ {destination(pl)}{/if}
                    </span>
                  </div>
                {/each}
              </div>
              <p class="hint">
                {[
                  ...(summary.categories ? [t("{n} categories", { n: summary.categories })] : []),
                  t("{n} album(s)", { n: summary.albums }),
                  ...(summary.skipped ? [t("{n} photos skipped", { n: summary.skipped })] : []),
                  t("Photos already in the library are added to their album without a second copy."),
                ].join(" · ")}
              </p>
            {/if}
          </div>
        {/if}
        <label class="origin">
          <span>{t("Source")} <small>{t("(optional — e.g. photos saved from LINE or Facebook)")}</small></span>
          <SourceInput bind:value={origin} submitOnEnter={false} placeholder={t("Camera / phone detected automatically")} />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={dayAlbums} onchange={saveDayAlbums} />
          <span>{t("One album per day")} <small>{t("(named like 240503; add a topic later: 240503_Kyoto)")}</small></span>
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={includeDeleted} />
          <span>{t("Include previously deleted photos")} <small>{t("(normally, photos you deleted from the library are skipped)")}</small></span>
        </label>
        <div class="buttons">
          <button onclick={close}>{t("Cancel")}</button>
          <button class="primary" disabled={!source || (folderAlbums && !folders?.length)} onclick={start}>{t("Add photos")}</button>
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
          <p><strong>{p.cancelled ? t("Stopped adding photos.") : t("Done adding photos.")}</strong></p>
          {#if p.errors.length}
            <details open={p.errors.length < 5}>
              <summary>
                {t("{n} error(s)", { n: p.failed })}{#if p.errors.length < p.failed} — {t("showing the last {n}; see Add history for all", { n: p.errors.length })}{/if}
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
            <button onclick={() => { started = false; source = ""; scannedFor = ""; folders = null; origin = ""; includeDeleted = false; }}>{t("Add more")}</button>
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
  .dialog.wide { width: min(820px, calc(100vw - 32px)); }
  .plan { margin-top: 12px; }
  .presets { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; font-size: 13px; }
  .presets span { color: var(--muted); }
  .tree { max-height: 40vh; overflow-y: auto; border: 1px solid var(--border); border-radius: 8px; padding: 4px 0; }
  .folder { display: flex; align-items: center; gap: 8px; padding: 3px 8px; font-size: 13px; }
  .folder:hover { background: var(--hover); }
  .folder.off .fname, .folder.off small { color: var(--muted); text-decoration: line-through; }
  .fname { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .folder small { color: var(--muted); min-width: 3ch; text-align: right; }
  .folder select { height: 28px; flex-shrink: 0; width: 150px; }
  .dest { width: 34%; flex-shrink: 0; color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
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
