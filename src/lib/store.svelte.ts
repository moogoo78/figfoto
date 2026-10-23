import { SvelteSet } from "svelte/reactivity";
import { api } from "./api";
import { errorText } from "./format";
import { t } from "./i18n.svelte";
import type { FolderPreset } from "./folderPlan";
import type { Album, Counts, DateSource, Filter, FolderRole, ImportProgress, ImportRecord, Mark, MarkSlot, SavedLens, Sort, Tag, View } from "./types";

interface Toast {
  id: number;
  text: string;
  error: boolean;
}

class Store {
  ready = $state(false);
  root = $state<string | null>(null);
  view = $state<View>({ kind: "library" });

  /** Filter-bar fields; combined with the current view in `filter`. */
  query = $state({ dateFrom: "", dateTo: "", minRating: 0, camera: "", origin: "", dateSource: "" as DateSource | "", text: "", tagIds: [] as number[] });
  sort = $state<Sort>("takenDesc");
  thumbSize = $state(180);

  tags = $state.raw<Tag[]>([]);
  /** Every album as a tree, each followed by the albums inside it. */
  albums = $state.raw<Album[]>([]);
  cameras = $state.raw<string[]>([]);
  counts = $state.raw<Counts>({ all: 0, favorites: 0, trash: 0, marked: 0, unbacked: 0 });
  marks = $state.raw<MarkSlot[]>([]);
  sources = $state.raw<string[]>([]);
  /** Lenses in the library (EXIF or set by hand), for suggestions. */
  lenses = $state.raw<SavedLens[]>([]);
  settingsNotes = $state.raw<string[]>([]);
  imports = $state.raw<ImportRecord[]>([]);

  selection = new SvelteSet<number>();
  /** Bumped whenever photo data changes, so lists re-fetch. */
  version = $state(0);

  importOpen = $state(false);
  /** Source folder to prefill when the import dialog opens. */
  importPreset = $state<{ dir: string; folderAlbums: boolean; preset: FolderPreset; plan: Record<string, FolderRole> | null } | null>(null);
  progress = $state.raw<ImportProgress | null>(null);
  toasts = $state<Toast[]>([]);
  /** Mark whose batch is running, with copy progress. */
  runningMark = $state<Mark | null>(null);
  markProgress = $state<{ done: number; total: number } | null>(null);
  /** Backup target being backed up or verified, with progress. */
  backupTask = $state<{ id: number; kind: "run" | "verify" } | null>(null);
  backupProgress = $state<{ done: number; total: number } | null>(null);

  #toastId = 0;
  #lastImportRefresh = 0;

  get filter(): Filter {
    const q = this.query;
    const v = this.view;
    const tagIds = [...q.tagIds];
    if (v.kind === "tag" && !tagIds.includes(v.id)) tagIds.push(v.id);
    return {
      dateFrom: q.dateFrom || undefined,
      dateTo: q.dateTo || undefined,
      minRating: q.minRating || undefined,
      camera: q.camera || undefined,
      origin: q.origin || undefined,
      dateSource: q.dateSource || undefined,
      text: q.text || undefined,
      mark: v.kind === "mark" ? v.mark : undefined,
      tagIds,
      albumId: v.kind === "album" ? v.id : undefined,
      importId: v.kind === "import" ? v.id : undefined,
      favorite: v.kind === "favorites" ? true : undefined,
      trashed: v.kind === "trash",
    };
  }

  get hasQuery(): boolean {
    const q = this.query;
    return !!(q.dateFrom || q.dateTo || q.minRating || q.camera || q.origin || q.dateSource || q.text || q.tagIds.length);
  }

  async init() {
    await api.onImportProgress((p) => this.#onProgress(p));
    await api.onMarkProgress((p) => (this.markProgress = { done: p.done, total: p.total }));
    await api.onBackupProgress((p) => (this.backupProgress = { done: p.done, total: p.total }));
    await api.onDatesFixed((n) => {
      this.toast(t("Dated {n} photo(s) from their file or folder names", { n }));
      this.version++;
    });
    try {
      const lib = await api.currentLibrary();
      this.root = lib?.root ?? null;
      if (this.root) await this.refreshMeta();
    } catch (e) {
      this.toast(errorText(e), true);
    }
    this.ready = true;
  }

  async openLibrary(path: string, create: boolean) {
    try {
      const lib = await api.openLibrary(path, create);
      this.root = lib.root;
      this.view = { kind: "library" };
      if (this.sort === "children") this.sort = "takenDesc";
      this.clearQuery();
      this.selection.clear();
      await this.refreshMeta();
      this.version++;
    } catch (e) {
      this.toast(errorText(e), true);
    }
  }

  /** Opens the Add photos dialog; with `dir`, prefilled to add that folder again the same way. */
  openImport(dir = "", folderAlbums = false, preset: FolderPreset = "one", plan: Record<string, FolderRole> | null = null) {
    this.importPreset = dir ? { dir, folderAlbums, preset, plan } : null;
    this.importOpen = true;
  }

  setView(view: View) {
    const fromAlbum = this.view.kind === "album";
    this.view = view;
    this.selection.clear();
    // Albums read oldest first (by the albums inside when it shows them inline); everything
    // else goes back to newest first on the way out, and "by album" means nothing elsewhere.
    if (view.kind === "album") {
      const album = this.albums.find((a) => a.id === view.id);
      this.sort = album?.children && album.display === "inline" ? "children" : "takenAsc";
    } else if (fromAlbum || this.sort === "children") {
      this.sort = "takenDesc";
    }
  }

  clearQuery() {
    // Mutate in place: components hold a reference to `query`.
    Object.assign(this.query, { dateFrom: "", dateTo: "", minRating: 0, camera: "", origin: "", dateSource: "", text: "", tagIds: [] });
  }

  async refreshMeta() {
    const [tags, albums, cameras, counts, imports, marks, sources, lenses, settingsNotes] = await Promise.all([
      api.listTags(),
      api.listAlbums(),
      api.listCameras(),
      api.counts(),
      api.listImports(),
      api.listMarks(),
      api.listSources(),
      api.listLenses(),
      api.listSettingsNotes(),
    ]);
    this.marks = marks;
    this.sources = sources;
    this.lenses = lenses;
    this.settingsNotes = settingsNotes;
    this.imports = imports;
    this.tags = tags;
    this.albums = albums;
    this.cameras = cameras;
    this.counts = counts;
    // Leave views that no longer exist.
    const v = this.view;
    if (
      (v.kind === "album" && !albums.some((a) => a.id === v.id)) ||
      (v.kind === "tag" && !tags.some((tag) => tag.id === v.id)) ||
      (v.kind === "import" && !imports.some((i) => i.id === v.id))
    ) {
      this.setView({ kind: "library" });
    }
  }

  /** Runs a mutation, reports errors, then refreshes lists and sidebar data. */
  /** Runs a change, then refreshes. `okText` may be built from the result. */
  async mutate<T>(fn: () => Promise<T>, okText?: string | ((result: T) => string)) {
    try {
      const result = await fn();
      const text = typeof okText === "function" ? okText(result) : okText;
      if (text) this.toast(text);
    } catch (e) {
      this.toast(errorText(e), true);
    }
    this.version++;
    await this.refreshMeta().catch(() => {});
  }

  /** The albums from the top down to `id`, `id` included. */
  albumPath(id: number | null): Album[] {
    const path: Album[] = [];
    for (let a = this.albums.find((x) => x.id === id); a && !path.includes(a); a = this.albums.find((x) => x.id === a!.parentId)) {
      path.unshift(a);
    }
    return path;
  }

  /** "Japan › Kyoto" */
  albumLabel(id: number | null): string {
    return this.albumPath(id).map((a) => a.name).join(" › ");
  }

  /** The albums inside `id`, at any depth, in tree order. */
  albumsInside(id: number): Album[] {
    const start = this.albums.findIndex((a) => a.id === id);
    if (start < 0) return [];
    const depth = this.albums[start].depth;
    const out: Album[] = [];
    for (let i = start + 1; i < this.albums.length && this.albums[i].depth > depth; i++) out.push(this.albums[i]);
    return out;
  }

  /** The albums directly inside `id` (the top-level albums with null), in order. */
  albumsIn(id: number | null): Album[] {
    return this.albums.filter((a) => a.parentId === id);
  }

  markSlot(mark: Mark): MarkSlot | undefined {
    return this.marks.find((s) => s.mark === mark);
  }

  /** "Delete", the label given to a custom mark, or "Mark 2". */
  markName(mark: Mark): string {
    return this.markSlot(mark)?.label || (mark === "x" ? t("Delete") : t("Mark {n}", { n: mark }));
  }

  /** Marks photos with `mark`, or clears it if they all carry it already. */
  toggleMark(ids: number[], mark: Mark) {
    return this.mutate(() => api.toggleMark(ids, mark));
  }

  /** Runs the batch action of `mark` on every photo carrying it. */
  async runMark(mark: Mark) {
    if (this.runningMark) return;
    this.runningMark = mark;
    this.markProgress = null;
    try {
      const r = await api.runMark(mark);
      const parts = [t("{name}: {n} photo(s) done", { name: this.markName(mark), n: r.done })];
      if (r.skipped) parts.push(t("{n} already there", { n: r.skipped }));
      if (r.errors.length) parts.push(`${t("{n} failed", { n: r.errors.length })} — ${r.errors.slice(0, 3).join("; ")}`);
      this.toast(parts.join(", "), r.errors.length > 0);
    } catch (e) {
      this.toast(errorText(e), true);
    }
    this.runningMark = null;
    this.markProgress = null;
    this.version++;
    await this.refreshMeta().catch(() => {});
  }

  /** Backs up to (`run`) or checks (`verify`) a backup disk; one task at a time. */
  async backup(id: number, kind: "run" | "verify") {
    if (this.backupTask) return;
    this.backupTask = { id, kind };
    this.backupProgress = null;
    try {
      if (kind === "run") {
        const r = await api.runBackup(id);
        const parts = [t("Backup: {n} photo(s) copied", { n: r.copied })];
        if (r.skipped) parts.push(t("{n} already there", { n: r.skipped }));
        if (r.cancelled) parts.push(t("cancelled"));
        if (r.errors.length) parts.push(`${t("{n} failed", { n: r.errors.length })} — ${r.errors.slice(0, 3).join("; ")}`);
        this.toast(parts.join(", "), r.errors.length > 0);
      } else {
        const r = await api.verifyBackup(id);
        const parts = [t("Checked {n} photo(s)", { n: r.checked })];
        if (r.cancelled) parts.push(t("cancelled"));
        if (r.problems.length) {
          parts.push(`${t("{n} missing or changed, will be copied again", { n: r.problems.length })} — ${r.problems.slice(0, 3).join("; ")}`);
        }
        this.toast(parts.join(", "), r.problems.length > 0);
      }
    } catch (e) {
      this.toast(errorText(e), true);
    }
    this.backupTask = null;
    this.backupProgress = null;
    this.version++;
    await this.refreshMeta().catch(() => {});
  }

  toast(text: string, error = false) {
    const id = ++this.#toastId;
    this.toasts.push({ id, text, error });
    setTimeout(() => (this.toasts = this.toasts.filter((toast) => toast.id !== id)), error ? 6000 : 2500);
  }

  #onProgress(p: ImportProgress) {
    this.progress = p;
    const now = Date.now();
    if (p.finished || (p.added > 0 && now - this.#lastImportRefresh > 2000)) {
      this.#lastImportRefresh = now;
      this.version++;
      this.refreshMeta().catch(() => {});
    }
  }
}

export const store = new Store();
