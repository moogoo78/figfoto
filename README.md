# FigFoto

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
 │ LINE / FB    │        │     → albums, nested       │
 │ *cloud sync  │        │ • source label (LINE, …)   │
 └──────────────┘        │ skip: same hash, deleted,  │
                         │       unchanged path       │
                         └─────────────┬──────────────┘
                                       │ copy (sources never modified)
                                       ▼
                ┌──────────────────────────────────────────┐
                │              LIBRARY (disk)              │
                │ originals/YYYY/MM/DD/                    │
                │ .figfoto/ db · thumbs · previews       │
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
        │ X / 1-4 marks    │ │ description   │ │ albums, nested   │
        │ rating ★, fav ♥  │ │ camera, lens  │ │   (Japan ›       │
        │ duplicates view  │ │ private notes │ │    Kyoto › Day 2)│
        │ Del → trash      │ │ date fix      │ │ shortcuts        │
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
          │ • BACKUP to disk / cloud: copy new, check, verify │
          │     └▶ *offload originals (keep thumbnail/screen) │
          │ • *publish album online ⇄ sync back edits         │
          └───────────────────────────────────────────────────┘
```

## Library layout

```
<library>/
  originals/YYYY/MM/DD/<name>[-N].<ext>   # copied photos (sources are never modified)
  .figfoto/library.db                     # SQLite index (albums live here, not as folders)
  .figfoto/thumbs/<xx>/<hash>.jpg         # 400px thumbnails
  .figfoto/previews/<xx>/<hash>.jpg       # full-size JPEG for HEIC/TIFF (webview can't show them)
  .figfoto/trash/                         # trashed photos (restorable until emptied)
```

Added photos are filed by date under `originals/` and stay there; albums are kept in the library
database, so adding a photo to an album (or to several) never moves the file.

Albums are organized around days:

- an **album** holds the photos of a day, named `YYMMDD_topic` (*240503_Kyoto*). Adding photos with
  **One album per day** on makes a `240503` album for each day that has no album yet; add the topic
  later. To make one by hand, hover the day header in the grid and click **＋ New album** (or select
  photos and click **＋ New album** in the selection bar). The name starts as `240503_`.
- a **category** groups albums and can hold other categories (*Travel › Japan*). Drag an album onto
  a category to file it.
- a **section** is anything inside an album (*240503_Kyoto › Morning*).

The Albums page has three views. **Daily** lists every album by date, under year and month
headings, wherever the album is filed. **Categories** shows the top-level categories. **Tree** shows
everything. **Search albums…** (or `/`) finds albums, categories and sections by any part of a name
or the category above it (`kyoto`, `2405`, `japan 1905`). A page shows what is inside it either as tiles or with the photos inline, under a
heading each, ordered oldest first, newest first or by name.

Adding organized folders lets you choose what each folder becomes: a **category**, an **album**, a
**section** (only inside an album), part of the folder above, or skipped. Presets: one album; an
album per folder inside; the same with deeper folders as sections; or categories like the folders,
with albums at the bottom. With day albums on, a category folder's own photos go into a
`YYMMDD_<folder>` album per day inside it.

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

The date comes from EXIF `DateTimeOriginal` → `DateTimeDigitized` → `DateTime`. For a photo without
one, the date is read from the file name (`IMG_20190503_101500.jpg`, `IMG-20190503-WA0001.jpg`,
`Screenshot_2019-05-03-10-15-00.png`). If the name has none, the nearest folder name with a date is
used (`2019-05-03 Kyoto/`). Only then does the photo take the file's modified time, which is often
wrong. A date without a time keeps the modified time's time of day. Photo info shows where a guessed
date came from, and **Filter → Date from** finds them. **Set date** in the selection bar sets the date
by hand. With only a date, each photo keeps its time; with a time too, the earliest photo gets it and
the others keep their spacing. The files move to the new date folders. Photos added before this are
checked once when the library opens.
Exact duplicates (same BLAKE3 hash) are skipped when adding photos; near-duplicates are found with a
64-bit perceptual hash in the **Duplicates** view.

Supported formats: JPEG, PNG, WebP, TIFF, GIF, HEIC/HEIF. HEIC is decoded with libheif + libde265
(compiled into the app). HEIC and TIFF get a full-size JPEG preview in `.figfoto/previews/`
for the viewer; missing previews are created in the background when a library is opened.

### Backup

The **Backup** view copies the library to other disks. Click **Add backup disk…**, pick a folder
on the disk, then **Back up now**. Each disk gets a `figfoto-backup/` folder that can be browsed
and restored without the app:

```
figfoto-backup/
  originals/YYYY/MM/DD/<name>.<ext>   # every photo, by date
  db/library-<time>.db                # library database snapshot, newest 5 kept
```

Each run copies only what's new, and every copy is checked by hash. Backups only add: photos you
delete from the library stay on the backup. **Verify** reads every copy back; missing or changed
ones are copied again on the next run. You can keep several disks, each with its own record. If a
disk shows up at another path, use **Change folder…**; another library's backup folder is refused.
The sidebar shows how many photos are not backed up yet. Backups made before the app was renamed
to FigFoto stay in their `frutyfoto-backup/` folder and keep working.

**Export CSV…** writes a list of every photo (trash left out): `original_path` (where it was added
from), `library_path`, `datetime` (the date it came with: EXIF, or the file's modified time),
`adjusted_datetime` (filled only when the date was guessed from a name or set by hand),
`date_source`, `albums` (full paths like `Travel / 210715_Japan`, `; ` between several), `tags`,
`camera_make`, `camera_model`, `lens` (set by hand for a manual lens, else from the camera),
`iso`, `aperture` (the f-number, like `2.8`), `exposure_time` (like `1/250` or `2s`),
`focal_length` (mm) and `file_name`. It opens in spreadsheet apps as UTF-8.

**Cloud backups** (Google Drive, Backblaze B2, Proton Drive, or anything else rclone supports) go
through [rclone](https://rclone.org), which you install yourself:

```sh
brew install rclone                         # macOS
winget install Rclone.Rclone                # Windows
curl https://rclone.org/install.sh | sudo bash   # Linux (distro packages may be too old for Proton)
rclone config                               # add a remote, e.g. "gdrive" (Google Drive) or "b2"
```

Then click **Add cloud backup…**, pick the remote and a folder (for B2, the bucket name). The cloud
gets the same `figfoto-backup/` layout as a disk. rclone checks each upload's hash; **Verify**
compares every copy with the library using the cloud's own hash (SHA-1 or MD5). If a different
file is already at a photo's path, it's kept, renamed `<name>-replaced-<time>.<ext>`. If rclone
isn't on PATH (apps started from the macOS Dock don't get the shell's PATH), use **Locate rclone…**.

## Development (Docker)

Everything builds and runs in a container; the window is shown on the host display via Wayland
(X11 fallback). The `Makefile` has shortcuts for the common ones:

```sh
make dev                                  # run the app (docker compose up dev)
make test                                 # Rust tests
make release                              # .deb + AppImage into ./release
```

The full commands:

```sh
docker compose build                      # dev image: Rust, Node 22, pnpm, webkit2gtk
docker compose up dev                     # pnpm install && pnpm tauri dev
docker compose run --rm sh bash -c "cd src-tauri && cargo test"
docker compose run --rm sh pnpm check     # svelte-check / tsc
docker compose run --rm bundle            # .deb + AppImage into ./release
```

**For real use, run the AppImage** (`./release/FigFoto_*.AppImage`). It runs natively,
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
