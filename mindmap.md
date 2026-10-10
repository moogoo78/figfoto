# frutyfoto

- input

   - import (folder)

      - from

         - hard drive

         - SD card

         - phone (USB / synced folder)

         - \*cloud drive (via rclone or sync client, then import the folder)

            - google drive

            - proton drive

      - import type

         - camera / SD card (date only)

         - organized folders

            - each folder holding photos → album named after it

            - reuse album with the same name

            - photo already in library → added to album, no second copy

      - skip rules

         - already in library (content hash)

         - previously deleted (hash kept when trash is emptied)

            - "include previously deleted photos" checkbox

         - fast re-import: known path + size + mtime → no re-read

      - import history

         - import again (keeps import type)

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

   - organize (user view)

      - all fotos

         - group by date

         - sort by

            - newest / oldest

            - recently imported

            - highest rated

      - album

      - tag

      - find / filter

         - date range

         - camera / phone

         - source

         - favorite

         - file name

- output

   - target folder

      - mark action

      - export album

   - \*backup

      - second disk

      - \*backblaze b2

   - \*publish album
