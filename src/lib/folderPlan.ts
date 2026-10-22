import type { FolderInfo, FolderRole } from "./types";

/** Ready-made plans: the chosen folder is one album; each folder right inside it is one; every
 * folder is one; or folders with folders inside are categories and the rest albums. */
export type FolderPreset = "one" | "inside" | "every" | "categories";

function presetRole(preset: FolderPreset, folder: FolderInfo, hasInside: boolean): FolderRole {
  if (preset === "categories") {
    if (!hasInside) return "album";
    // The chosen folder (my-fotos) only holds the categories; it isn't one itself.
    return folder.depth === 0 ? "parent" : "category";
  }
  if (preset === "every" || folder.depth === 0) return "album";
  return preset === "inside" && folder.depth === 1 ? "album" : "section";
}

export function presetPlan(folders: FolderInfo[], preset: FolderPreset): Record<string, FolderRole> {
  const parents = new Set(folders.filter((f) => f.depth > 0).map((f) => f.path.split("/").slice(0, -1).join("/")));
  return Object.fromEntries(folders.map((f) => [f.path, presetRole(preset, f, parents.has(f.path))]));
}

/** Where a folder's photos go: skipped, into no album, or into an album (and section path),
 * filed in a category (path, none when empty). Mirrors `place` in src-tauri/src/import.rs. */
export type Place =
  | { kind: "skip" }
  | { kind: "loose"; categories: string[] }
  | { kind: "in"; album: string; sections: string[]; categories: string[] };

export function place(folder: FolderInfo, byPath: Map<string, FolderInfo>, roles: Record<string, FolderRole>): Place {
  const parts = folder.path ? folder.path.split("/") : [];
  let album: string | null = null;
  let sections: string[] = [];
  const categories: string[] = [];
  for (let depth = 0; depth <= parts.length; depth++) {
    const path = parts.slice(0, depth).join("/");
    const name = depth === 0 ? (byPath.get("")?.name ?? "") : parts[depth - 1];
    const role = roles[path] ?? "parent";
    if (role === "skip") return { kind: "skip" };
    if (!name) continue;
    if (role === "category") {
      categories.push(name);
      [album, sections] = [null, []];
    } else if (role === "album") [album, sections] = [name, []];
    else if (role === "section") album == null ? (album = name) : sections.push(name);
  }
  return album == null ? { kind: "loose", categories } : { kind: "in", album, sections, categories };
}
