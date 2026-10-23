export interface Photo {
  id: number;
  hash: string;
  relPath: string;
  origName: string;
  sourcePath: string | null;
  takenAt: string;
  importedAt: string;
  width: number | null;
  height: number | null;
  fileSize: number;
  mime: string | null;
  cameraMake: string | null;
  cameraModel: string | null;
  /** Set by hand if present, otherwise from EXIF. */
  lens: string | null;
  /** `lens` was set by hand (manual lens) and can be reset to EXIF. */
  lensSet: boolean;
  /** The camera recorded a lens, so none can be set by hand. */
  exifLens: boolean;
  iso: number | null;
  fNumber: number | null;
  exposure: string | null;
  focalLen: number | null;
  gpsLat: number | null;
  gpsLon: number | null;
  rating: number;
  favorite: boolean;
  trashedAt: string | null;
  mark: Mark | null;
  /** Where the photo came from, set by the user (e.g. "LINE"). */
  source: string | null;
  /** Free-text description. */
  note: string | null;
  /** Free-text notes on how it was shot. */
  settingsNote: string | null;
  /** The photographer's own record; never shared or published. */
  privateNote: string | null;
  /** Guessed from EXIF make/model. */
  device: "phone" | "camera" | "unknown";
  path: string;
  /** What the viewer shows: the original, or a JPEG preview for HEIC. */
  display: string;
  thumb: string;
}

export interface SavedLens {
  name: string;
  photos: number;
}

export interface Tag {
  id: number;
  name: string;
  count: number;
}

/**
 * An album holds photos, other albums, or both. A category groups albums (Travel › Japan); an
 * album holds a day's photos (240503_Kyoto); anything inside an album is a section of it.
 */
export interface Album {
  id: number;
  name: string;
  createdAt: string;
  /** The album it is inside, if any. */
  parentId: number | null;
  /** How deep it is (0 at the top). `listAlbums` returns the tree, each album followed by the ones inside it. */
  depth: number;
  /** Photos directly in it. */
  count: number;
  /** Photos in it and every album inside it (each counted once). */
  total: number;
  /** Albums directly inside it. */
  children: number;
  cover: string | null;
  childOrder: ChildOrder;
  display: AlbumDisplay;
  /** When pinned to the sidebar: its place among the shortcuts (lower first). */
  shortcut: number | null;
  /** As stored: a category or an album. */
  kind: AlbumKind;
  /** What it is where it sits; inside an album it is a section. */
  role: AlbumRole;
  /** When its first and last photos (its own and inside it) were taken. */
  firstTaken: string | null;
  lastTaken: string | null;
}

export type AlbumKind = "category" | "album";
export type AlbumRole = AlbumKind | "section";

/** How the albums inside an album are ordered: oldest first, newest first, or by name. */
export type ChildOrder = "date" | "newest" | "name";

/** How an album's page shows the albums inside: their photos under a heading each, or tiles. */
export type AlbumDisplay = "inline" | "tiles";

export interface PhotoDetail {
  photo: Photo;
  tags: Tag[];
  albums: Album[];
  /** Each of `albums` with the albums it is inside ("Japan › Kyoto"), keyed by album id. */
  paths: Record<number, string>;
}

export interface Bucket {
  day: string;
  count: number;
  /** When sorted by the albums inside: that album's id, or 0 for the album's own photos. */
  album: number | null;
  /** When sorted by the albums inside: the album's name (empty for the album's own photos). */
  label: string;
  /** When sorted by the albums inside: how deep it is (0 right inside the album shown). */
  depth: number;
  /** When sorted by the albums inside: the album it is in, unless that is the album shown. */
  parent: number | null;
  /** When sorted by the albums inside: number of albums directly inside it. */
  children: number;
  /** Photos in it and the albums inside it (`count` is only the ones placed directly in it). */
  total: number;
}

export interface Counts {
  all: number;
  favorites: number;
  trash: number;
  marked: number;
  /** Live photos on no backup disk; 0 while no backup is set up. */
  unbacked: number;
}

export interface Filter {
  dateFrom?: string;
  dateTo?: string;
  tagIds: number[];
  albumId?: number;
  importId?: number;
  minRating?: number;
  favorite?: boolean;
  camera?: string;
  text?: string;
  /** A mark, or "any" for every marked photo. */
  mark?: string;
  /** The user-set source, else "phone" / "camera" / "unknown". */
  origin?: string;
  trashed: boolean;
}

/** "children" (by the albums inside) only applies inside an album (elsewhere it sorts oldest first). */
export type Sort = "takenDesc" | "takenAsc" | "importedDesc" | "ratingDesc" | "children";

/** What a folder becomes when adding organized folders: a category, an album or a section
 * (each inside the one of the folder above), part of the folder above ("parent"), or not added
 * ("skip"). Anything inside an album is a section; a section with no album above is an album. */
export type FolderRole = "category" | "album" | "section" | "parent" | "skip";

/** A folder holding photos (directly or below), for planning an organized-folders import. */
export interface FolderInfo {
  /** Path inside the chosen folder, "/"-separated; "" for the chosen folder itself. */
  path: string;
  name: string;
  depth: number;
  /** Photos directly in it. */
  photos: number;
  /** Photos in it and every folder below. */
  total: number;
}

export interface ImportRecord {
  id: number;
  sourceDir: string;
  startedAt: string;
  /** null if the app quit before the import finished. */
  finishedAt: string | null;
  added: number;
  skippedDupes: number;
  /** Skipped because they had been deleted from the library before. */
  skippedDeleted: number;
  failed: number;
  cancelled: boolean;
  /** Each source folder was turned into an album. */
  folderAlbums: boolean;
  /** Subfolders were turned into sections (imports from before albums nested). */
  folderSections: boolean;
  /** The source folder itself was the one album. */
  singleAlbum: boolean;
  /** What each folder became (path inside the source folder → role), for adding it again. */
  folderPlan: Record<string, FolderRole> | null;
  /** Photos from this import still in the library. */
  photoCount: number;
  cover: string | null;
}

export interface ImportProgress {
  importId: number;
  total: number;
  done: number;
  added: number;
  skipped: number;
  /** Skipped because they had been deleted from the library before. */
  skippedDeleted: number;
  failed: number;
  current: string;
  /** Still walking the folder; `total` is the count found so far. */
  scanning: boolean;
  finished: boolean;
  cancelled: boolean;
  /** Albums photos were added to (organized-folder imports). */
  albums: number;
  /** Most recent errors only (capped); the full list is in the import history. */
  errors: { path: string; error: string }[];
}

/** What the main area is showing. */
export type View =
  | { kind: "library" }
  | { kind: "albums" }
  | { kind: "favorites" }
  | { kind: "album"; id: number }
  | { kind: "tag"; id: number }
  | { kind: "imports" }
  | { kind: "import"; id: number }
  | { kind: "marks" }
  | { kind: "mark"; mark: Mark }
  | { kind: "lenses" }
  | { kind: "backup" }
  | { kind: "duplicates" }
  | { kind: "trash" };

/** "x" marks a photo for deletion; "1".."4" are custom actions. */
export type Mark = "x" | "1" | "2" | "3" | "4";
export const MARKS: Mark[] = ["x", "1", "2", "3", "4"];

export type MarkActionKind = "trash" | "copy" | "album" | "tag";

export interface MarkSlot {
  mark: Mark;
  label: string;
  /** null for a custom mark that hasn't been set up. */
  kind: MarkActionKind | null;
  /** Folder (copy), album id (album) or tag name (tag). */
  target: string;
  count: number;
}

export interface RunResult {
  done: number;
  skipped: number;
  errors: string[];
}

export interface BackupTarget {
  id: number;
  name: string;
  /** `cloud`: an rclone remote; `path` is then `remote:folder/figfoto-backup`. */
  kind: "disk" | "cloud";
  /** The `figfoto-backup` folder. */
  path: string;
  /** The disk is connected and the folder belongs to this library; for the cloud, rclone
   *  knows the remote. */
  available: boolean;
  lastRunAt: string | null;
  lastVerifiedAt: string | null;
  backedUp: number;
  total: number;
  bytesToCopy: number;
}

export interface RcloneRemote {
  name: string;
  /** rclone backend: `drive`, `b2`, `protondrive`… */
  kind: string;
}

export interface RcloneStatus {
  path: string | null;
  version: string | null;
  remotes: RcloneRemote[];
  /** Why rclone can't be used, e.g. not installed. */
  error: string | null;
}

export interface BackupResult {
  copied: number;
  /** Already on the disk, recorded without copying. */
  skipped: number;
  errors: string[];
  cancelled: boolean;
}

export interface VerifyResult {
  checked: number;
  /** Missing or changed copies; the next backup copies them again. */
  problems: string[];
  cancelled: boolean;
}
