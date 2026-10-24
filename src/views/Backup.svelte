<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { basename, errorText, fmtBytes, fmtDateTime } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import type { BackupTarget, RcloneStatus } from "../lib/types";

  let targets = $state.raw<BackupTarget[]>([]);
  let loaded = $state(false);

  // Cloud form: adding a cloud backup (`editing` null) or moving one.
  let cloudForm = $state<{ editing: BackupTarget | null } | null>(null);
  let rclone = $state.raw<RcloneStatus | null>(null);
  let checking = $state(false);
  let remote = $state("");
  let folder = $state("");
  let saving = $state(false);

  const BACKENDS: Record<string, string> = {
    drive: "Google Drive", b2: "Backblaze B2", protondrive: "Proton Drive", s3: "S3",
    onedrive: "OneDrive", dropbox: "Dropbox", crypt: "encrypted",
  };

  async function load() {
    try {
      targets = await api.listBackupTargets();
    } catch (e) {
      store.toast(errorText(e), true);
    }
    loaded = true;
  }

  // Reload whenever photo data changes (imports, backups, trash…).
  $effect(() => {
    store.version;
    load();
  });

  async function pick(title: string, current?: string) {
    const dir = await open({ directory: true, title, defaultPath: current || undefined });
    return typeof dir === "string" ? dir : null;
  }

  async function add() {
    const dir = await pick(t("Back up to…"));
    if (dir) store.mutate(() => api.addBackupTarget(dir), t("Backup disk added"));
  }

  /** A CSV of every photo: where it came from, where it is, its dates, albums and tags. */
  async function exportCsv() {
    const path = await save({ title: t("Export photo list"), defaultPath: "figfoto-photos.csv", filters: [{ name: "CSV", extensions: ["csv"] }] });
    if (!path) return;
    try {
      const n = await api.exportCsv(path);
      store.toast(t("{n} photo(s) exported to {name}", { n, name: basename(path) }));
    } catch (e) {
      store.toast(errorText(e), true);
    }
  }

  async function changeFolder(target: BackupTarget) {
    const dir = await pick(t("Where is this backup now?"), target.path);
    if (dir) store.mutate(() => api.setBackupTargetPath(target.id, dir));
  }

  async function checkRclone() {
    checking = true;
    try {
      rclone = await api.rcloneStatus();
    } catch (e) {
      store.toast(errorText(e), true);
    }
    checking = false;
    if (rclone && !rclone.remotes.some((r) => r.name === remote)) remote = rclone.remotes[0]?.name ?? "";
  }

  function openCloudForm(editing: BackupTarget | null) {
    cloudForm = { editing };
    remote = "";
    folder = "";
    if (editing) {
      // `remote:folder/figfoto-backup` (or an old `frutyfoto-backup`) → remote, folder
      const i = editing.path.indexOf(":");
      remote = editing.path.slice(0, i);
      folder = editing.path.slice(i + 1).replace(/\/?(figfoto|frutyfoto)-backup$/, "");
    }
    checkRclone();
  }

  async function saveCloud() {
    if (!cloudForm || !remote) return;
    const path = `${remote}:${folder.trim().replace(/^\/+|\/+$/g, "")}`;
    const editing = cloudForm.editing;
    saving = true;
    try {
      if (editing) await api.setCloudBackupTargetPath(editing.id, path);
      else await api.addCloudBackupTarget(path);
      store.toast(editing ? t("Backup folder changed") : t("Cloud backup added"));
      cloudForm = null;
      store.version++;
    } catch (e) {
      store.toast(errorText(e), true);
    }
    saving = false;
  }

  async function locateRclone() {
    const file = await open({ title: t("Where is rclone?") });
    if (typeof file !== "string") return;
    try {
      await api.setRclonePath(file);
    } catch (e) {
      store.toast(errorText(e), true);
    }
    await checkRclone();
    store.version++;
  }

  async function useRcloneFromPath() {
    await api.setRclonePath(null).catch((e) => store.toast(errorText(e), true));
    await checkRclone();
    store.version++;
  }

  function changeTarget(target: BackupTarget) {
    if (target.kind === "cloud") openCloudForm(target);
    else changeFolder(target);
  }

  function rename(target: BackupTarget, input: HTMLInputElement) {
    const name = input.value.trim();
    if (!name) input.value = target.name;
    else if (name !== target.name) store.mutate(() => api.renameBackupTarget(target.id, name));
  }

  function remove(target: BackupTarget) {
    store.mutate(() => api.removeBackupTarget(target.id), t("Backup removed; its files stay on the disk"));
  }

  const busy = $derived(!!store.backupTask);
</script>

<section class="backup">
  <header>
    <h1>{t("Backup")}</h1>
    {#if store.counts.unbacked}
      <span class="count">{t("{n} photo(s) not backed up", { n: store.counts.unbacked })}</span>
    {/if}
    <button class="primary add" onclick={add}>＋ {t("Add backup disk…")}</button>
    <button onclick={() => openCloudForm(null)}>＋ {t("Add cloud backup…")}</button>
    <button onclick={exportCsv} title={t("A CSV of every photo: where it was added from, where it is in the library, its dates, albums and tags.")}>{t("Export CSV…")}</button>
  </header>

  <div class="body">
    <p class="hint">
      {t("Copies the library's photos to another disk or to cloud storage, in the same date folders, plus a copy of the library database. Each run copies only what's new, and every copy is checked. Photos you delete from the library stay on the backup.")}
    </p>

    {#if cloudForm}
      <article class="cloud-form">
        <h2>{cloudForm.editing ? t("Move cloud backup “{name}”", { name: cloudForm.editing.name }) : t("Back up to the cloud")}</h2>
        {#if !rclone}
          <p class="hint">{t("Looking for rclone…")}</p>
        {:else if rclone.error && !rclone.path}
          <p class="hint">
            {t("Cloud backups use rclone, which isn't installed. Install it, then click “Check again”:")}
          </p>
          <ul class="install">
            <li>macOS: <code>brew install rclone</code></li>
            <li>Windows: <code>winget install Rclone.Rclone</code></li>
            <li>Linux: <code>curl https://rclone.org/install.sh | sudo bash</code></li>
          </ul>
        {:else if rclone.error}
          <p class="error">{rclone.error}</p>
        {:else if !rclone.remotes.length}
          <p class="hint">
            {t("rclone has no remotes yet. In a terminal, run rclone config and add one (Google Drive, Backblaze B2, Proton Drive…), then click “Check again”.")}
          </p>
        {:else}
          <div class="fields">
            <label>{t("Remote")}
              <select bind:value={remote}>
                {#each rclone.remotes as r (r.name)}
                  <option value={r.name}>{r.name} ({BACKENDS[r.kind] ?? r.kind})</option>
                {/each}
              </select>
            </label>
            <label class="grow">{t("Folder")}
              <input bind:value={folder} placeholder={t("e.g. Photos (for B2: the bucket name)")}
                onkeydown={(e) => { e.stopPropagation(); if (e.key === "Enter") saveCloud(); }} />
            </label>
          </div>
          <p class="hint">{t("A figfoto-backup folder is made inside it: {path}", { path: `${remote}:${folder.trim() ? folder.trim().replace(/^\/+|\/+$/g, "") + "/" : ""}figfoto-backup` })}</p>
        {/if}
        <div class="form-actions">
          {#if rclone?.path}
            <span class="rclone" title={rclone.path}>rclone {rclone.version ?? ""}</span>
          {/if}
          <button onclick={locateRclone}>{t("Locate rclone…")}</button>
          {#if rclone?.path && !rclone.error}<button onclick={useRcloneFromPath} title={t("Find rclone on PATH again")}>{t("Default")}</button>{/if}
          <button disabled={checking} onclick={checkRclone}>{t("Check again")}</button>
          <span class="spacer"></span>
          <button onclick={() => (cloudForm = null)}>{t("Cancel")}</button>
          <button class="primary" disabled={!remote || saving || !!rclone?.error} onclick={saveCloud}>
            {saving ? t("Connecting…") : cloudForm.editing ? t("Move") : t("Add")}
          </button>
        </div>
      </article>
    {/if}

    {#if loaded && !targets.length}
      <p class="empty">{t("No backup disk yet. Plug one in and click “Add backup disk…”, or back up to the cloud.")}</p>
    {/if}

    {#each targets as target (target.id)}
      {@const running = store.backupTask?.id === target.id ? store.backupTask : null}
      {@const missing = target.total - target.backedUp}
      <article class="target" class:offline={!target.available}>
        <div class="info">
          <input class="name" value={target.name} title={t("Rename")} onkeydown={(e) => e.stopPropagation()}
            onchange={(e) => rename(target, e.currentTarget)} />
          <div class="path" title={target.path}>
            {target.path}
            {#if target.kind === "cloud"}<span class="badge cloud">{t("cloud")}</span>{/if}
            {#if !target.available}
              <span class="badge">{target.kind === "cloud" ? t("remote not set up") : t("not connected")}</span>
            {/if}
          </div>
          <div class="status">
            {#if missing}
              {t("{done} of {total} photo(s) backed up · {size} to copy",
                { done: target.backedUp, total: target.total, size: fmtBytes(target.bytesToCopy) })}
            {:else}
              <span class="ok">{t("All {n} photo(s) backed up", { n: target.total })}</span>
            {/if}
          </div>
          <div class="dates">
            {t("Last backup")}: {target.lastRunAt ? fmtDateTime(target.lastRunAt) : t("never")}
            · {t("Last check")}: {target.lastVerifiedAt ? fmtDateTime(target.lastVerifiedAt) : t("never")}
          </div>
          {#if running}
            {@const p = store.backupProgress}
            <div class="progress">
              <span>{running.kind === "run" ? t("Backing up…") : t("Checking…")}
                {#if p}{p.done} / {p.total}{/if}</span>
              <div class="bar"><div style:width="{p && p.total ? (p.done / p.total) * 100 : 0}%"></div></div>
            </div>
          {/if}
        </div>

        <div class="actions">
          {#if running}
            <button onclick={() => api.cancelBackup()}>{t("Cancel")}</button>
          {:else}
            <button class="primary" disabled={!target.available || busy} onclick={() => store.backup(target.id, "run")}>
              {t("Back up now")}
            </button>
            <button disabled={!target.available || busy || !target.backedUp} title={t("Read every copy back and check it")}
              onclick={() => store.backup(target.id, "verify")}>{t("Verify")}</button>
            <button disabled={busy} onclick={() => changeTarget(target)}>{t("Change folder…")}</button>
            <button class="danger" disabled={busy} title={t("Forget this backup; its files stay on the disk")}
              onclick={() => remove(target)}>{t("Remove")}</button>
          {/if}
        </div>
      </article>
    {/each}
  </div>
</section>

<style>
  .backup { display: flex; flex-direction: column; height: 100%; }
  header { display: flex; align-items: center; gap: 10px; padding: 12px 16px; border-bottom: 1px solid var(--border); }
  h1 { font-size: 18px; margin: 0; }
  .count { color: var(--muted); font-size: 13px; }
  .add { margin-left: auto; }
  .body { flex: 1; overflow-y: auto; padding: 16px; display: flex; flex-direction: column; gap: 10px; }
  .hint, .empty { color: var(--muted); font-size: 13px; line-height: 1.6; margin: 0 0 6px; }
  .target { display: flex; align-items: flex-start; gap: 14px; padding: 12px; border: 1px solid var(--border);
    border-radius: 8px; background: var(--panel); }
  .target.offline .status, .target.offline .dates { opacity: 0.6; }
  .info { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .name { width: 240px; font-weight: 600; }
  .path { font-size: 12px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .badge { margin-left: 6px; padding: 0 6px; border-radius: 4px; border: 1px solid var(--danger); color: var(--danger); }
  .status { font-size: 13px; }
  .ok { color: var(--accent); }
  .dates { font-size: 12px; color: var(--muted); }
  .progress { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  .actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; max-width: 320px; }
  .bar { height: 6px; border-radius: 3px; background: var(--cell); overflow: hidden; }
  .bar div { height: 100%; background: var(--accent); transition: width 0.2s; }
  .badge.cloud { border-color: var(--accent); color: var(--accent); }
  .cloud-form { display: flex; flex-direction: column; gap: 8px; padding: 12px; border: 1px solid var(--accent);
    border-radius: 8px; background: var(--panel); }
  .cloud-form h2 { font-size: 14px; margin: 0; }
  .cloud-form .hint { margin: 0; }
  .install { margin: 0; padding-left: 20px; font-size: 13px; line-height: 1.8; }
  .error { color: var(--danger); font-size: 13px; margin: 0; }
  .fields { display: flex; gap: 10px; flex-wrap: wrap; }
  .fields label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  .fields .grow { flex: 1; min-width: 200px; }
  .form-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
  .rclone { font-size: 12px; color: var(--muted); }
  .spacer { flex: 1; }
</style>
