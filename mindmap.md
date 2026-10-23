# FigFoto

- input

   - add photos (folder)

      - from

         - hard drive

         - SD card

         - phone (USB / synced folder)

         - \*cloud drive (via rclone or sync client, then add photos from the folder)

            - google drive

            - proton drive

      - folder type

         - camera / SD card (date only)

         - organized folders

            - each folder holding photos → album named after it

            - reuse album with the same name

            - photo already in library → added to album, no second copy

            - folder tree before adding: each folder → category / album / section (inside an album only) / into folder above / skip, with where its photos will go

               - presets: one album · an album per folder inside · … with sections · categories like the folders, albums at the bottom

               - adding again: a photo moved elsewhere in the same top-level album stays put

               - Add again reuses the choices

      - skip rules

         - already in library (content hash)

         - previously deleted (hash kept when trash is emptied)

            - "include previously deleted photos" checkbox

         - fast re-add: known path + size + mtime → no re-read

      - add history

         - add again (keeps folder type)

   - quick add

      - line/facebook

      - source label

   - date source

      - EXIF

      - file name (IMG_20210704…)

      - file mtime

      - manual fix

   - \*video / live photo

- process (library)

   - store (physical disk view)

      - year and date subfolder

      - all metadata in library DB

      - \*DB snapshot / XMP sidecar

   - curate

      - marks (x, 1–4)

      - duplicates

      - rating / favorite

      - trash

   - describe (photo info)

      - description

      - camera settings

         - set on many photos from the selection bar

      - private notes (never shared or published)

      - manual lens

         - only when EXIF has no lens

         - saved lens list (whole library)

   - organize (user view)

      - all photos

         - group by date

         - sort by

            - newest / oldest

            - recently added

            - highest rated

      - album

         - Albums page (below All photos): album / category tiles (stacked when albums are inside); new / rename / delete; drag an album onto another to put it inside

         - sidebar shortcuts: albums pinned with ☆, in the order pinned; drop photos on a shortcut album

         - category › album › section (Travel › Japan › 240503_Kyoto › Morning); one albums table, nested

            - album: a day's photos, named YYMMDD_topic; category: groups albums, nests; section: anything inside an album

            - Albums page: Daily (every album by date, by year and month, wherever filed; only unfiled) / Categories / Tree

            - day albums on adding photos (One album per day): YYMMDD for days with no album yet; a category folder's own photos → YYMMDD_<folder> per day

            - make it a category / an album; a category can't go inside an album


            - make inside: by hand (from the selection too) / split by day / added folders (nested like the folders)

            - move: drag onto another album, or Move to…; not inside itself

            - delete: the albums inside move up a level; for an album inside another, its photos too

            - album page: path back up; albums inside shown inline (their photos under a heading each, the album's own last) or as tiles

               - order: oldest first / newest first / by name; added folders → by name

               - a photo in two albums inside shows once, under the first

               - move photos: selection bar or drag onto a heading / tile

               - collapse / expand an album inside (caret on the heading), or all at once; remembered per album

            - album pickers: the tree, indented; photo info: album paths

            - from older libraries: categories → albums shown as tiles, sections → albums inside ("Other" folded back); a copy of the library kept first

      - tag

      - find / filter

         - date range

         - camera / phone

         - source

         - favorite

         - file name / notes

- output

   - target folder

      - mark action

      - export album

   - backup

      - second disk

         - Backup view: add a disk (folder picker), "Back up now"

         - only copies what's new; every copy checked by hash

         - readable layout: figfoto-backup/originals/YYYY/MM/DD

         - library DB snapshot each run (newest 5 kept)

         - only adds: photos deleted from the library stay on the backup

         - several disks, each with its own record

         - disk mounted elsewhere → "Change folder…"; another library's backup refused

         - verify: read every copy back; missing / changed → copied again

         - sidebar: count of photos not backed up

      - cloud (through rclone)

         - Google Drive, Backblaze B2, Proton Drive… (any rclone remote)

         - rclone installed by the user (brew / winget / install script); "Locate rclone…" if not on PATH

         - remotes set up with `rclone config`; Backup view picks remote + folder

         - same layout and marker as a disk; DB snapshot uploaded each run

         - only adds: a different file at a photo's path is kept as <name>-replaced-<time>

         - verify: rclone check by the cloud's hash (SHA-1 / MD5)

         - \*bundle rclone with the installers (Tauri sidecar)

      - \*offload originals (save local disk)

         - only for photos verified in the backup (hash match)

         - keep locally (user picks the level)

            - thumbnail (400px)

            - screen size (thumbnail + ~1600px JPEG)

         - set per album; a photo in several albums is offloaded only when all of them are

         - need a larger copy

            - original → download from the backup

            - web size → download from the published album

            - "Bring back" → restore the original to the library

         - grid / viewer show an "offloaded" badge

   - \*publish album (online)

      - desktop is the master (originals + full library)

      - online copy: web-size images + albums (nested) / photo info

         - without private notes, without originals

      - mark an album "Online" (with the albums inside), then Sync (only what changed goes up)

      - organize online while the computer is off

         - albums, covers, ratings, descriptions

         - phone upload → online Inbox

         - online keeps a list of edits

      - sync back

         - replay online edits on the library

         - Inbox photos added like any batch (hash dedupe)

         - same field changed on both sides: latest edit wins

         - online trash → desktop trash (restorable)

         - real clash → "Needs review" list

      - needs: stable ids (photo hash, album UUID) + updated_at

      - hosting: own Docker server or Cloudflare (static site = read-only)
