<script lang="ts">
  import { t } from "../lib/i18n.svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api } from "../lib/api";
  import { errorText, fmtBytes, fmtDateTime } from "../lib/format";
  import { store } from "../lib/store.svelte";
  import type { BackupTarget } from "../lib/types";

  let targets = $state.raw<BackupTarget[]>([]);
  let loaded = $state(false);

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

  async function changeFolder(target: BackupTarget) {
    const dir = await pick(t("Where is this backup now?"), target.path);
    if (dir) store.mutate(() => api.setBackupTargetPath(target.id, dir));
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
  </header>

  <div class="body">
    <p class="hint">
      {t("Copies the library's photos to another disk, in the same date folders, plus a copy of the library database. Each run copies only what's new, and every copy is checked. Photos you delete from the library stay on the backup.")}
    </p>

    {#if loaded && !targets.length}
      <p class="empty">{t("No backup disk yet. Plug one in and click “Add backup disk…”.")}</p>
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
            {#if !target.available}<span class="badge">{t("not connected")}</span>{/if}
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
            <button disabled={busy} onclick={() => changeFolder(target)}>{t("Change folder…")}</button>
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
</style>
