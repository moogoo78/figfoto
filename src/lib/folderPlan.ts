import type { AlbumKind, FolderInfo, FolderRole } from "./types";

/** Ready-made plans: the chosen folder is one album holding everything; each folder right
 * inside it is an album (deeper ones merged in, or made sections of it); or folders holding
 * folders are categories, with an album for each folder at the bottom. */
export type FolderPreset = "one" | "inside" | "sections" | "every";

function presetRole(preset: FolderPreset, folder: FolderInfo, alone: boolean, hasInside: boolean): FolderRole {
  if (preset === "one") return folder.depth === 0 ? "album" : "parent";
  // The chosen folder (my-fotos) only holds the albums, unless it's the only folder.
  if (folder.depth === 0) return alone ? "album" : "parent";
  if (preset === "every") return hasInside ? "category" : "album";
  if (folder.depth === 1) return "album";
  return preset === "sections" ? "section" : "parent";
}

export function presetPlan(folders: FolderInfo[], preset: FolderPreset): Record<string, FolderRole> {
  const alone = folders.length === 1;
  const hasInside = (f: FolderInfo) => folders.some((g) => g.depth > f.depth && (f.path === "" || g.path.startsWith(`${f.path}/`)));
  return Object.fromEntries(folders.map((f) => [f.path, presetRole(preset, f, alone, hasInside(f))]));
}

/** Where a folder's photos go: skipped, into no album, or into the album at a path (names and
 * kinds from the top down). Mirrors `place` in src-tauri/src/import.rs. */
export type Place = { kind: "skip" } | { kind: "loose" } | { kind: "in"; albums: { name: string; kind: AlbumKind }[] };

export function place(folder: FolderInfo, byPath: Map<string, FolderInfo>, roles: Record<string, FolderRole>): Place {
  const parts = folder.path ? folder.path.split("/") : [];
  const albums: { name: string; kind: AlbumKind }[] = [];
  let inAlbum = false;
  for (let depth = 0; depth <= parts.length; depth++) {
    const path = parts.slice(0, depth).join("/");
    const name = depth === 0 ? (byPath.get("")?.name ?? "") : parts[depth - 1];
    const role = roles[path] ?? "parent";
    if (role === "skip") return { kind: "skip" };
    if (!name || role === "parent") continue;
    if (role === "category" && !inAlbum) albums.push({ name, kind: "category" });
    else {
      albums.push({ name, kind: "album" });
      inAlbum = true;
    }
  }
  return albums.length ? { kind: "in", albums } : { kind: "loose" };
}

/** Whether a folder above `folder` is an album or section, so it can only be a section. */
export function underAlbum(folder: FolderInfo, roles: Record<string, FolderRole>): boolean {
  const parts = folder.path ? folder.path.split("/") : [];
  for (let depth = 0; depth < parts.length; depth++) {
    const role = roles[parts.slice(0, depth).join("/")];
    if (role === "album" || role === "section") return true;
  }
  return false;
}
