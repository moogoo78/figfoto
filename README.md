# frutyfoto

A desktop photo organizer (Tauri 2 + Svelte 5). Adding photos copies them into a managed library
sorted by date, then lets you browse, rate, tag, group into albums and weed out duplicates.

## Use flow

`*` = planned.

```
                        ┌────────────────────────────┐
                        │  Open / create a library   │
                        │  (pick a library folder)   │
                        └─────────────┬──────────────┘
                                      │
   SOURCES                            ▼
 ┌──────────────┐        ┌────────────────────────────┐
 │ hard drive   │        │         ADD PHOTOS         │
 │ SD card      │──────▶ │ • camera/SD  → by date     │
 │ phone folder │        │ • organized folders        │
 │ LINE / FB    │        │     → albums (+ sections)  │
 │ *cloud sync  │        │ • source label (LINE, …)   │
 └──────────────┘        │ skip: same hash, deleted,  │
                         │       unchanged path       │
                         └─────────────┬──────────────┘
                                       │ copy (sources never modified)
                                       ▼
                ┌──────────────────────────────────────────┐
                │              LIBRARY (disk)              │
                │ originals/YYYY/MM/DD/   albums/<album>/  │
                │ .frutyfoto/ db · thumbs · previews       │
                └─────────────────────┬────────────────────┘
                                      │
                                      ▼
                ┌──────────────────────────────────────────┐
                │      BROWSE  (All photos, by date)       │
                │   grid ⇄ viewer · filter · sort          │
                └───┬───────────────┬──────────────────┬───┘
                    │               │                  │
                    ▼               ▼                  ▼
        ┌──────────────────┐ ┌───────────────┐ ┌──────────────────┐
        │      CURATE      │ │   DESCRIBE    │ │    ORGANIZE      │
        │ X / 1-4 marks    │ │ description   │ │ albums → files   │
        │ rating ★, fav ♥  │ │ camera, lens  │ │   move into      │
        │ duplicates view  │ │ private notes │ │   albums/<name>  │
        │ Del → trash      │ │ date fix      │ │ topics, sections │
        └────────┬─────────┘ └───────────────┘ │ tags             │
                 │                             └──────────────────┘
                 ▼
        ┌──────────────────┐
        │  MARKS: run batch│
        │ X  → library trash ──▶ Empty trash ──▶ system Trash
        │ 1-4→ copy to folder / add to album / add tag
        └────────┬─────────┘
                 │
                 ▼
   OUTPUT ┌───────────────────────────────────────────────────┐
          │ • copy to target folder / export album            │
          │ • BACKUP to 2nd disk: copy new, check hash, verify│
          │     └▶ *offload originals (keep thumbnail/screen) │
          │ • *publish album online ⇄ sync back edits         │
          └───────────────────────────────────────────────────┘
```

## Library layout

```
<library>/
  originals/YYYY/MM/DD/<name>[-N].<ext>   # copied photos (sources are never modified)
  albums/<album>/<name>[-N].<ext>         # photos that belong to an album
  .frutyfoto/library.db                     # SQLite index
  .frutyfoto/thumbs/<xx>/<hash>.jpg         # 400px thumbnails
  .frutyfoto/previews/<xx>/<hash>.jpg       # full-size JPEG for HEIC/TIFF (webview can't show them)
  .frutyfoto/trash/                         # trashed photos (restorable until emptied)
```

Added photos are filed by date under `originals/`. Adding a photo to an album **moves the file** into
`albums/<album>/`; renaming the album renames the folder. A photo in several albums lives in the
first one's folder; when it leaves all albums it goes back to `originals/YYYY/MM/DD/`. To make an
album from a day, hover the day header in the grid and click **＋ New album** (or select photos and
click **＋ New album** in the selection bar).

**Empty trash** moves the files to the system Trash (not a permanent delete), so they can still be
recovered from the desktop.

### Marks (fast culling)

While browsing, press `X` to mark photos for deletion and `1`–`4` for custom actions (press again
to unmark). The **Marks** view sets up what each of `1`–`4` does — *copy to folder* (e.g. another
disk; `albums/Trip/a.jpg` → `<target>/Trip/a.jpg`, identical copies are skipped), *add to album* or
*add tag* — and runs a mark on all its photos in one batch. `X` moves them to the library trash.
Processed photos lose their mark.

### Sources

Each photo shows whether it came from a **phone** or a **camera** (guessed from EXIF make/model) in
album views and the viewer. You can label a source yourself (e.g. LINE, Facebook) per photo, for the
selection, or for a whole batch in the **Add photos** dialog; filter by it in **Filter → Source**.

The date comes from EXIF `DateTimeOriginal` → `DateTimeDigitized` → `DateTime` → file mtime.
Exact duplicates (same BLAKE3 hash) are skipped when adding photos; near-duplicates are found with a
64-bit perceptual hash in the **Duplicates** view.

Supported formats: JPEG, PNG, WebP, TIFF, GIF, HEIC/HEIF. HEIC is decoded with libheif + libde265
(compiled into the app). HEIC and TIFF get a full-size JPEG preview in `.frutyfoto/previews/`
for the viewer; missing previews are created in the background when a library is opened.

## Development (Docker)

Everything builds and runs in a container; the window is shown on the host display via Wayland
(X11 fallback).

```sh
docker compose build                      # dev image: Rust, Node 22, pnpm, webkit2gtk
docker compose up dev                     # pnpm install && pnpm tauri dev
docker compose run --rm sh bash -c "cd src-tauri && cargo test"
docker compose run --rm sh pnpm check     # svelte-check / tsc
docker compose run --rm bundle            # .deb + AppImage into ./release
```

**For real use, run the AppImage** (`./release/frutyfoto_*.AppImage`). It runs natively,
bundles WebKit (so nothing needs installing on the host), and its folder picker can see all your
files. The dev container can only see the folders mounted into it (below).

Inside the dev container, the host's `~/Pictures` (override with `PHOTOS_DIR=...`) is mounted
read-only at `/photos`, and `./library-dev` is mounted at `/library`. Use those paths in the
folder pickers.

## Keyboard shortcuts

| Key | Grid (selection) | Viewer |
| --- | --- | --- |
| `X` / `1`–`4` | toggle mark | toggle mark |
| `Shift`+`0`–`5` | set rating | set rating |
| `F` | toggle favorite | toggle favorite |
| `T` | focus tag input | focus tag input |
| `Del` | trash / restore | trash / restore |
| `Ctrl+A` / `Esc` | select all / clear | — / close |
| `Enter` / double-click | open viewer | — |
| `←` `→` / `I` | — | previous/next / toggle info |

Drag thumbnails onto an album in the sidebar to add them. Double-click an album to rename it.

## License

MIT — see [LICENSE](LICENSE). Release builds bundle libheif and libde265, which are LGPL-licensed.
