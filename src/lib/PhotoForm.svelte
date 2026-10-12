<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api } from "./api";
  import { flash } from "./flash";
  import { t } from "./i18n.svelte";
  import SourceInput from "./SourceInput.svelte";
  import { store } from "./store.svelte";
  import type { Photo } from "./types";

  /** The photo's editable text fields with one Save button for all of them. Unsaved edits
   * are saved when moving to another photo or closing the viewer, so nothing is lost. */
  let { photo }: { photo: Photo } = $props();

  type Fields = { note: string; settings: string; priv: string; lens: string; source: string };
  const fieldsOf = (p: Photo): Fields => ({
    note: p.note ?? "",
    settings: p.settingsNote ?? "",
    priv: p.privateNote ?? "",
    lens: p.lensSet ? (p.lens ?? "") : "",
    source: p.source ?? "",
  });

  const empty: Fields = { note: "", settings: "", priv: "", lens: "", source: "" };
  let loadedId = -1;
  /** The values as last loaded or saved; `draft` is what's in the fields. */
  let orig = $state.raw<Fields>(empty);
  let draft = $state<Fields>({ ...empty });
  let saved = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;
  let form: HTMLFormElement | undefined = $state();

  const changed = (d: Fields, o: Fields) => (Object.keys(d) as (keyof Fields)[]).filter((k) => d[k].trim() !== o[k]);
  const dirty = $derived(changed(draft, orig).length > 0);

  /** The photo's values when last seen, to tell real changes from a stale refresh. */
  let seen: Fields = empty;

  // Load the photo's values; when switching photos, first save what was typed for the last one.
  // For the same photo, only pick up fields that changed in the library and aren't being edited.
  $effect(() => {
    const p = photo;
    untrack(() => {
      const now = fieldsOf(p);
      if (p.id !== loadedId) {
        if (loadedId >= 0 && dirty) write(loadedId, { ...draft }, changed(draft, orig));
        loadedId = p.id;
        orig = now;
        draft = { ...now };
      } else {
        const o = { ...orig };
        for (const k of Object.keys(now) as (keyof Fields)[]) {
          if (now[k] !== seen[k] && draft[k].trim() === o[k]) draft[k] = o[k] = now[k];
        }
        orig = o;
      }
      seen = now;
    });
  });

  onDestroy(() => {
    if (dirty) write(loadedId, { ...draft }, changed(draft, orig));
  });

  function write(id: number, d: Fields, keys: (keyof Fields)[]) {
    if (!keys.length) return;
    store.mutate(async () => {
      const ids = [id];
      for (const k of keys) {
        const v = d[k].trim();
        if (k === "note") await api.setNote(ids, "note", v);
        if (k === "settings") await api.setNote(ids, "settings", v);
        if (k === "priv") await api.setNote(ids, "private", v);
        if (k === "lens") await api.setLens(ids, v);
        if (k === "source") await api.setSource(ids, v);
      }
    });
  }

  function save() {
    const keys = changed(draft, orig);
    if (!keys.length) return;
    const d = { ...draft };
    orig = { ...orig, ...Object.fromEntries(keys.map((k) => [k, d[k].trim()])) };
    for (const k of keys) {
      const el = form?.querySelector(`[data-field="${k}"]`);
      flash(el?.matches("input, textarea") ? el : el?.querySelector("input"));
    }
    saved = true;
    clearTimeout(timer);
    timer = setTimeout(() => (saved = false), 1200);
    write(photo.id, d, keys);
  }

  function onkeydown(e: KeyboardEvent) {
    const single = e.target instanceof HTMLInputElement;
    if (e.key === "Enter" && (single || e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      save();
    }
    if (e.key === "Escape") (e.target as HTMLElement).blur();
    e.stopPropagation();
  }
  const lensListId = `lenses-${Math.random().toString(36).slice(2)}`;
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<form bind:this={form} onsubmit={(e) => { e.preventDefault(); save(); }} {onkeydown}>
  <h3>{t("Description")}</h3>
  <textarea data-field="note" rows="3" bind:value={draft.note} placeholder={t("What's in this photo…")}></textarea>

  <h3>{t("Camera settings")}</h3>
  <textarea data-field="settings" rows="3" bind:value={draft.settings} placeholder={t("Filter, tripod, bulb time, manual aperture…")}></textarea>

  <h3 title={t("Your own record, for you only. Never shown when sharing or publishing.")}>🔒 {t("Private notes")}</h3>
  <textarea data-field="priv" rows="3" bind:value={draft.priv} placeholder={t("Your own record, for you only. Never shown when sharing or publishing.")}></textarea>

  <h3>{t("Lens")}</h3>
  <input data-field="lens" bind:value={draft.lens} list={lensListId} disabled={photo.exifLens}
    placeholder={photo.exifLens ? (photo.lens ?? "") : t("Lens (manual lenses)")}
    title={photo.exifLens
      ? t("The camera recorded the lens for this photo.")
      : t("Set the lens for photos taken with a manual lens. Enter nothing to go back to what the camera recorded.")} />
  <datalist id={lensListId}>
    {#each store.lenses as l}<option value={l.name}></option>{/each}
  </datalist>

  <h3>{t("Source")}</h3>
  <span class="source" data-field="source"><SourceInput bind:value={draft.source} submitOnEnter={false} /></span>

  <div class="save">
    <button type="submit" class:primary={dirty} class:saved disabled={!dirty && !saved} title={t("Save (Ctrl+Enter)")}>
      {saved ? `✓ ${t("Saved")}` : t("Save")}
    </button>
    {#if dirty}<span class="unsaved">{t("Unsaved changes")}</span>{/if}
  </div>
</form>

<style>
  h3 { font-size: 12px; text-transform: uppercase; color: var(--muted); margin: 18px 0 6px; letter-spacing: 0.05em; }
  textarea { display: block; width: 100%; min-height: 72px; resize: vertical; font-size: 13px; }
  input[data-field="lens"], .source :global(input) { width: 100%; }
  .source { display: block; }
  .save { display: flex; align-items: center; gap: 10px; margin-top: 12px; }
  .save button { min-width: 96px; }
  .save button.saved { color: var(--ok); border-color: var(--ok); opacity: 1; }
  .unsaved { font-size: 12px; color: var(--star); }
</style>
