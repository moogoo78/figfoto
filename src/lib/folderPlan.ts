import type { FolderInfo, FolderRole } from "./types";

/** Ready-made plans: the chosen folder is one album holding everything; each folder right
 * inside it is an album (deeper ones merged in); or every folder inside is an album, nested
 * like the folders. */
export type FolderPreset = "one" | "inside" | "every";

function presetRole(preset: FolderPreset, folder: FolderInfo, alone: boolean): FolderRole {
  if (preset === "one") return folder.depth === 0 ? "album" : "parent";
  // The chosen folder (my-fotos) only holds the albums, unless it's the only folder.
  if (folder.depth === 0) return alone ? "album" : "parent";
  return preset === "every" || folder.depth === 1 ? "album" : "parent";
}

export function presetPlan(folders: FolderInfo[], preset: FolderPreset): Record<string, FolderRole> {
  const alone = folders.length === 1;
  return Object.fromEntries(folders.map((f) => [f.path, presetRole(preset, f, alone)]));
}

/** Where a folder's photos go: skipped, into no album, or into the album at a path (names from
 * the top down). Mirrors `place` in src-tauri/src/import.rs. */
export type Place = { kind: "skip" } | { kind: "loose" } | { kind: "in"; albums: string[] };

export function place(folder: FolderInfo, byPath: Map<string, FolderInfo>, roles: Record<string, FolderRole>): Place {
  const parts = folder.path ? folder.path.split("/") : [];
  const albums: string[] = [];
  for (let depth = 0; depth <= parts.length; depth++) {
    const path = parts.slice(0, depth).join("/");
    const name = depth === 0 ? (byPath.get("")?.name ?? "") : parts[depth - 1];
    const role = roles[path] ?? "parent";
    if (role === "skip") return { kind: "skip" };
    if (name && role === "album") albums.push(name);
  }
  return albums.length ? { kind: "in", albums } : { kind: "loose" };
}
