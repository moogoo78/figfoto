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

export interface Album {
  id: number;
  name: string;
  createdAt: string;
  count: number;
  cover: string | null;
  /** The category the album is filed in, if any. */
  categoryId: number | null;
  /** Number of sections the album is split into. */
  sections: number;
  /** How the sections are ordered: by their first photo, or by name. */
  sectionKind: SectionKind;
  /** When pinned to the sidebar: its place among the shortcuts (lower first). */
  shortcut: number | null;
}

export type SectionKind = "date" | "name";

/** A named part of an album, like "Kyoto" in "2019 Japan", or "Day 2" inside Kyoto. */
export interface Section {
  id: number;
  name: string;
  /** The section it is in, if any. */
  parentId: number | null;
  /** How deep it is nested (0 at the top). */
  depth: number;
  /** Sections directly inside it; photos only go in sections with none. */
  subsections: number;
  /** Photos in it and its sub-sections. */
  count: number;
}

/** A box for albums and other categories, like "Travel" or "Travel › Japan". */
export interface Category {
  id: number;
  name: string;
  /** The category it is inside, if any. */
  parentId: number | null;
  /** Albums directly in it. */
  albums: number;
  /** Categories directly inside it. */
  categories: number;
  cover: string | null;
  /** When pinned to the sidebar: its place among the shortcuts (lower first). */
  shortcut: number | null;
}

export interface PhotoDetail {
  photo: Photo;
  tags: Tag[];
  albums: Album[];
  /** The photo's section path ("Kyoto › Day 2") in each album it has one in, keyed by album id. */
  sections: Record<number, string>;
}

export interface Bucket {
  day: string;
  count: number;
  /** When sorted by section: the section id, or 0 for the photos in none. */
  section: number | null;
  /** When sorted by section: the section name (empty for no section). */
  label: string;
  /** When sorted by section: how deep the section is nested (0 at the top). */
  depth: number;
  /** When sorted by section: the section it is in, if any. */
  parent: number | null;
  /** When sorted by section: number of sections directly inside it. */
  subsections: number;
  /** Photos in it and its sub-sections (`count` is only the ones directly in it). */
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

/** "section" only applies inside an album (elsewhere it sorts oldest first). */
export type Sort = "takenDesc" | "takenAsc" | "importedDesc" | "ratingDesc" | "section";

/** What a folder becomes when adding organized folders: a category (albums below it are
 * filed in it), an album, a section of the album above, part of the folder above ("parent"),
 * or not added ("skip"). */
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
  /** Subfolders were turned into sections of those albums. */
  folderSections: boolean;
  /** The source folder itself was the one album, its subfolders sections. */
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
  | { kind: "category"; id: number }
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
