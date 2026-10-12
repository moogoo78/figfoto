import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  Album, Bucket, Counts, Filter, ImportProgress, ImportRecord, Mark, MarkActionKind, MarkSlot, Photo, PhotoDetail, RunResult, SavedLens, Sort, Tag, Topic,
} from "./types";

export const src = (path: string) => convertFileSrc(path);

export const api = {
  openLibrary: (path: string, create: boolean) => invoke<{ root: string }>("open_library", { path, create }),
  currentLibrary: () => invoke<{ root: string } | null>("current_library"),

  startImport: (src: string, source: string, folderAlbums: boolean, includeDeleted: boolean) =>
    invoke<void>("start_import", { src, source: source || null, folderAlbums, includeDeleted }),
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

  listAlbums: () => invoke<Album[]>("list_albums"),
  createAlbum: (name: string) => invoke<number>("create_album", { name }),
  renameAlbum: (id: number, name: string) => invoke<void>("rename_album", { id, name }),
  listTopics: () => invoke<Topic[]>("list_topics"),
  createTopic: (name: string) => invoke<number>("create_topic", { name }),
  renameTopic: (id: number, name: string) => invoke<void>("rename_topic", { id, name }),
  deleteTopic: (id: number) => invoke<void>("delete_topic", { id }),
  /** `topicId` null takes the albums out of their topic. */
  setAlbumTopic: (albumIds: number[], topicId: number | null) => invoke<void>("set_album_topic", { albumIds, topicId }),
  deleteAlbum: (id: number) => invoke<void>("delete_album", { id }),
  addToAlbum: (albumId: number, ids: number[]) => invoke<void>("add_to_album", { albumId, ids }),
  removeFromAlbum: (albumId: number, ids: number[]) => invoke<void>("remove_from_album", { albumId, ids }),
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
};
