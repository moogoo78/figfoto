import { t } from "./i18n.svelte";
import { ask } from "@tauri-apps/plugin-dialog";
import { api } from "./api";
import { store } from "./store.svelte";
import type { Album } from "./types";

/** Drag and drop shared by the album tiles and the album tree. */
export const ALBUM_MIME = "application/x-foto-album";
export const PHOTOS_MIME = "application/x-foto-ids";
export const carries = (e: DragEvent, mime: string) => !!e.dataTransfer?.types.includes(mime);

/** Album being dragged (its id can't be read from the drag until the drop). */
let dragged: number | null = null;

export function dragAlbum(e: DragEvent, id: number) {
  dragged = id;
  e.dataTransfer?.setData(ALBUM_MIME, String(id));
}

export function endDrag() {
  dragged = null;
}

/** Whether what's dragged can land on album `target`, or the top level when it's null. */
export function canDrop(e: DragEvent, target: number | null) {
  if (carries(e, PHOTOS_MIME)) return target != null;
  if (!carries(e, ALBUM_MIME) || dragged == null) return false;
  if (target == null) return store.albums.find((a) => a.id === dragged)?.parentId != null;
  // An album can't go inside itself or one inside it.
  if (dragged === target || store.albumsInside(dragged).some((a) => a.id === target)) return false;
  // Nor can a category (or an album holding one) go inside an album or a section.
  const into = store.albums.find((a) => a.id === target);
  const moved = [store.albums.find((a) => a.id === dragged), ...store.albumsInside(dragged)];
  return into?.role === "category" || !moved.some((a) => a?.kind === "category");
}

/** Drops photos (moved out of album `from` when given, otherwise added) or an album on `target`. */
export function drop(e: DragEvent, target: number | null, from: number | null = null) {
  const photos = e.dataTransfer?.getData(PHOTOS_MIME);
  const album = e.dataTransfer?.getData(ALBUM_MIME);
  if (photos && target != null) {
    e.preventDefault();
    const ids = JSON.parse(photos) as number[];
    if (from != null) store.mutate(() => api.movePhotos(from, target, ids), t("Moved {n} photo(s)", { n: ids.length }));
    else store.mutate(() => api.addToAlbum(target, ids), t("Added {n} photo(s)", { n: ids.length }));
  } else if (album && canDrop(e, target)) {
    e.preventDefault();
    store.mutate(() => api.moveAlbum(Number(album), target));
  }
  dragged = null;
}

/** Asks before deleting an album, saying where its photos and albums inside go. */
export async function deleteAlbum(a: Album) {
  const parent = store.albums.find((x) => x.id === a.parentId);
  const msg = parent
    ? t('Delete album "{name}"? Its photos and the albums inside it move up to "{parent}".', { name: a.name, parent: parent.name })
    : a.children
      ? t('Delete album "{name}"? Photos stay in the library; the albums inside it move to the top level.', { name: a.name })
      : t('Delete album "{name}"? Photos stay in the library.', { name: a.name });
  if (await ask(msg, { title: t("Delete album"), kind: "warning" })) store.mutate(() => api.deleteAlbum(a.id));
}
