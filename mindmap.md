# frutyfoto

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

            - option: subfolders → sections (2019 Japan/Kyoto)

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

         - topic (Travel, Family…; one level, one per album)

            - sidebar: collapsible groups

            - file an album: drag onto a topic, or pick on the album page

            - topic page: album cover tiles

            - album pickers grouped by topic

         - \*topic from added folders (my-fotos/Travel/2019 Kyoto)

         - section (Kyoto, Nara…; nest to any depth: Kyoto › Day 2 › Morning; a photo in one per album)

            - make: by hand / split by day / added subfolders (nested like the folders)

            - photos only in sections without sub-sections; a section's own photos move to an "Other" sub-section when it gets its first

            - two kinds per album, at every level: by date (sections in the order their photos were taken) or by topic (by name); added subfolders → by topic

            - album page: headings in that order, "No section" last

            - move photos: selection bar or drag onto a heading

            - collapse / expand a section (caret on the heading; remembered per album)

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

         - readable layout: frutyfoto-backup/originals/YYYY/MM/DD

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

      - online copy: web-size images + albums / topics / photo info

         - without private notes, without originals

      - mark album / topic "Online", then Sync (only what changed goes up)

      - organize online while the computer is off

         - albums, topics, covers, ratings, descriptions

         - phone upload → online Inbox

         - online keeps a list of edits

      - sync back

         - replay online edits on the library

         - Inbox photos added like any batch (hash dedupe)

         - same field changed on both sides: latest edit wins

         - online trash → desktop trash (restorable)

         - real clash → "Needs review" list

      - needs: stable ids (photo hash, album / topic UUID) + updated_at

      - hosting: own Docker server or Cloudflare (static site = read-only)
