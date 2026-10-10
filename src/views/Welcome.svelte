<script lang="ts">
  import { i18n, LOCALES, t, type Locale } from "../lib/i18n.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { store } from "../lib/store.svelte";

  async function choose(create: boolean) {
    const dir = await open({
      directory: true,
      title: create ? t("Choose an empty folder for the new library") : t("Open an existing frutyfoto library"),
    });
    if (typeof dir === "string") store.openLibrary(dir, create);
  }
</script>

<div class="welcome">
  <div class="card">
    <div class="logo">◒</div>
    <h1>frutyfoto</h1>
    <p>{t("Keep all your photos in one organised library, sorted by date, with tags, albums, ratings and duplicate detection.")}</p>
    <div class="buttons">
      <button class="primary" onclick={() => choose(true)}>{t("Create new library")}</button>
      <button onclick={() => choose(false)}>{t("Open existing library")}</button>
    </div>
    <p class="small">{t("A library is a folder. Imported photos are copied into")} <code>originals/</code> {t("inside it.")}</p>
    <select class="lang" value={i18n.locale} title="Language / 語言"
      onchange={(e) => i18n.set(e.currentTarget.value as Locale)}>
      {#each LOCALES as l (l.value)}<option value={l.value}>{l.label}</option>{/each}
    </select>
  </div>
</div>

<style>
  .welcome { height: 100vh; display: flex; align-items: center; justify-content: center; }
  .card { max-width: 440px; text-align: center; padding: 32px; }
  .logo { font-size: 56px; color: var(--accent); }
  h1 { margin: 8px 0; }
  p { color: var(--muted); line-height: 1.5; }
  .buttons { display: flex; flex-direction: column; gap: 10px; margin: 24px 0; }
  .buttons button { padding: 10px; font-size: 15px; }
  .small { font-size: 12px; }
</style>
