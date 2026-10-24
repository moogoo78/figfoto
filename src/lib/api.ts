import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Album, AlbumKind, BackupResult, BackupTarget, Bucket, Counts, Filter, FolderInfo, FolderRole, ImportProgress, ImportRecord, Mark, MarkActionKind, MarkSlot, Photo, PhotoDetail, RcloneStatus, RunResult, SavedLens, Sort, Tag, ChildOrder, AlbumDisplay, VerifyResult,
} from "./types";

export const src = (path: string) => convertFileSrc(path);

export const api = {
  openLibrary: (path: string, create: boolean) => invoke<{ root: string }>("open_library", { path, create }),
  currentLibrary: () => invoke<{ root: string } | null>("current_library"),

  /** `plan` (organized folders): what each folder becomes, keyed by its path inside `src`. */
  /** `dayAlbums`: photos going to no album land in a YYMMDD album per day; folder albums spanning days become categories of them. */
  startImport: (
    src: string,
    source: string,
    folderAlbums: boolean,
    plan: Record<string, FolderRole> | null,
    includeDeleted: boolean,
    dayAlbums: boolean,
  ) => invoke<void>("start_import", { src, source: source || null, folderAlbums, plan, includeDeleted, dayAlbums }),
  /** The folders under `src` that hold photos, each followed by the ones inside it. */
  scanFolders: (src: string) => invoke<FolderInfo[]>("scan_folders", { src }),
  cancelImport: () => invoke<void>("cancel_import"),
  listImports: () => invoke<ImportRecord[]>("list_imports"),
  importErrors: (importId: number) => invoke<{ path: string; error: string }[]>("import_errors", { importId }),
  onImportProgress: (cb: (p: ImportProgress) => void): Promise<UnlistenFn> =>
    listen<ImportProgress>("import://progress", (e) => cb(e.payload)),

  listPhotos: (filter: Filter, sort: Sort, offset: number, limit: number) =>
    invoke<Photo[]>("list_photos", { filter, sort, offset, limit }),
  listPhotoIds: (filter: Filter, sort: Sort, offset: number, limit: number) =>
    invoke<number[]>("list_photo_ids", { filter, sort, offset, limit }),
  /** First and last day (`YYYY-MM-DD`) of the given photos. */
  dateSpan: (ids: number[]) => invoke<[string, string] | null>("photo_date_span", { ids }),
  dateBuckets: (filter: Filter, sort: Sort) => invoke<Bucket[]>("photo_date_buckets", { filter, sort }),
  getPhoto: (id: number) => invoke<PhotoDetail>("get_photo", { id }),
  counts: () => invoke<Counts>("counts"),

  setRating: (ids: number[], rating: number) => invoke<void>("set_rating", { ids, rating }),
  setFavorite: (ids: number[], favorite: boolean) => invoke<void>("set_favorite", { ids, favorite }),
  /** Favorites the photos, or unfavorites them if all already are. */
  toggleFavorite: (ids: number[]) => invoke<boolean>("toggle_favorite", { ids }),

  listTags: () => invoke<Tag[]>("list_tags"),
  addTags: (ids: number[], names: string[]) => invoke<void>("add_tags", { ids, names }),
  removeTag: (ids: number[], tagId: number) => invoke<void>("remove_tag", { ids, tagId }),
  listCameras: () => invoke<string[]>("list_cameras"),

  /** Every album as a tree, each followed by the albums inside it. */
  listAlbums: () => invoke<Album[]>("list_albums"),
  /** `parentId` null makes a top-level album. */
  createAlbum: (name: string, parentId: number | null = null, kind: AlbumKind = "album") =>
    invoke<number>("create_album", { name, parentId, kind }),
  setAlbumKind: (id: number, kind: AlbumKind) => invoke<void>("set_album_kind", { id, kind }),
  renameAlbum: (id: number, name: string) => invoke<void>("rename_album", { id, name }),
  /** The albums inside it, and for an album inside another its photos, move up a level. */
  deleteAlbum: (id: number) => invoke<void>("delete_album", { id }),
  /** Moves an album inside another, or to the top level with `parentId` null. */
  moveAlbum: (id: number, parentId: number | null) => invoke<void>("move_album", { id, parentId }),
  setChildOrder: (albumId: number, order: ChildOrder) => invoke<void>("set_child_order", { albumId, order }),
  setAlbumDisplay: (albumId: number, display: AlbumDisplay) => invoke<void>("set_album_display", { albumId, display }),
  /** Pins an album to the sidebar's shortcuts, or unpins it. */
  setShortcut: (albumId: number, pinned: boolean) => invoke<void>("set_shortcut", { albumId, pinned }),
  /** Moves the album's own photos into an album inside it per day; returns how many days. */
  splitByDay: (albumId: number) => invoke<number>("split_by_day", { albumId }),
  addToAlbum: (albumId: number, ids: number[]) => invoke<void>("add_to_album", { albumId, ids }),
  /** With `inside`, also out of every album inside it. */
  removeFromAlbum: (albumId: number, ids: number[], inside = false) => invoke<void>("remove_from_album", { albumId, ids, inside }),
  /** Moves photos shown on album `from`'s page (in it or an album inside) into album `to`. */
  movePhotos: (from: number, to: number, ids: number[]) => invoke<void>("move_photos", { from, to, ids }),
  setAlbumCover: (albumId: number, photoId: number) => invoke<void>("set_album_cover", { albumId, photoId }),

  findDuplicates: (threshold: number) => invoke<Photo[][]>("find_duplicates", { threshold }),
  trashPhotos: (ids: number[]) => invoke<void>("trash_photos", { ids }),
  restorePhotos: (ids: number[]) => invoke<void>("restore_photos", { ids }),
  emptyTrash: () => invoke<number>("empty_trash"),

  setMark: (ids: number[], mark: Mark | null) => invoke<void>("set_mark", { ids, mark }),
  /** Marks the photos, or clears the mark if they all carry it already. */
  toggleMark: (ids: number[], mark: Mark) => invoke<void>("toggle_mark", { ids, mark }),
  listMarks: () => invoke<MarkSlot[]>("list_marks"),
  setMarkAction: (slot: number, label: string, kind: MarkActionKind | null, target: string) =>
    invoke<void>("set_mark_action", { slot, label, kind, target }),
  runMark: (mark: Mark) => invoke<RunResult>("run_mark", { mark }),
  onMarkProgress: (cb: (p: { mark: Mark; done: number; total: number }) => void): Promise<UnlistenFn> =>
    listen<{ mark: Mark; done: number; total: number }>("marks://progress", (e) => cb(e.payload)),

  listSources: () => invoke<string[]>("list_sources"),
  setSource: (ids: number[], source: string) => invoke<void>("set_source", { ids, source }),
  listLenses: () => invoke<SavedLens[]>("list_lenses"),
  addLens: (name: string) => invoke<void>("add_lens", { name }),
  removeLens: (name: string) => invoke<void>("remove_lens", { name }),
  renameLens: (old: string, name: string) => invoke<void>("rename_lens", { old, new: name }),
  setLens: (ids: number[], lens: string) => invoke<number>("set_lens", { ids, lens }),
  listSettingsNotes: () => invoke<string[]>("list_settings_notes"),
  /** `kind` "note" is the description, "settings" the camera-settings notes, "private" the
   * photographer's private notes. Empty clears. */
  setNote: (ids: number[], kind: "note" | "settings" | "private", text: string) => invoke<void>("set_note", { ids, kind, text }),

  listBackupTargets: () => invoke<BackupTarget[]>("list_backup_targets"),
  /** `path` is the folder picked (a `figfoto-backup` folder is made inside it). */
  addBackupTarget: (path: string) => invoke<number>("add_backup_target", { path }),
  setBackupTargetPath: (id: number, path: string) => invoke<void>("set_backup_target_path", { id, path }),
  /** `path` is `remote:folder` on an rclone remote (a `figfoto-backup` folder is made in it). */
  addCloudBackupTarget: (path: string) => invoke<number>("add_cloud_backup_target", { path }),
  setCloudBackupTargetPath: (id: number, path: string) => invoke<void>("set_cloud_backup_target_path", { id, path }),
  rcloneStatus: () => invoke<RcloneStatus>("rclone_status"),
  /** `null` goes back to finding rclone on PATH. */
  setRclonePath: (path: string | null) => invoke<void>("set_rclone_path", { path }),
  renameBackupTarget: (id: number, name: string) => invoke<void>("rename_backup_target", { id, name }),
  /** Forgets the backup; its files stay on the disk. */
  removeBackupTarget: (id: number) => invoke<void>("remove_backup_target", { id }),
  runBackup: (id: number) => invoke<BackupResult>("run_backup", { id }),
  verifyBackup: (id: number) => invoke<VerifyResult>("verify_backup", { id }),
  cancelBackup: () => invoke<void>("cancel_backup"),
  /** After opening a library: how many photos got a date from their file or folder name. */
  onDatesFixed: (cb: (n: number) => void): Promise<UnlistenFn> => listen<number>("dates://fixed", (e) => cb(e.payload)),
  /** `date` YYYY-MM-DD; with `time` (HH:MM) the earliest photo gets it and the rest keep their spacing. */
  setTakenAt: (ids: number[], date: string, time: string | null) => invoke<number>("set_taken_at", { ids, date, time }),
  exportCsv: (path: string) => invoke<number>("export_csv", { path }),
  onBackupProgress: (cb: (p: { targetId: number; done: number; total: number }) => void): Promise<UnlistenFn> =>
    listen<{ targetId: number; done: number; total: number }>("backup://progress", (e) => cb(e.payload)),
};
