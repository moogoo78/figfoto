use std::path::Path;

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, OptionalExtension};

use crate::error::{AppError, Result};

/// Each entry upgrades the schema by one version (tracked in PRAGMA user_version).
pub const MIGRATIONS: &[&str] = &[
    r#"
CREATE TABLE photos (
    id           INTEGER PRIMARY KEY,
    hash         TEXT NOT NULL UNIQUE,
    phash        INTEGER,
    rel_path     TEXT NOT NULL,
    orig_name    TEXT NOT NULL,
    source_path  TEXT,
    taken_at     TEXT NOT NULL,
    imported_at  TEXT NOT NULL,
    width        INTEGER,
    height       INTEGER,
    file_size    INTEGER NOT NULL,
    mime         TEXT,
    camera_make  TEXT,
    camera_model TEXT,
    lens         TEXT,
    iso          INTEGER,
    f_number     REAL,
    exposure     TEXT,
    focal_len    REAL,
    gps_lat      REAL,
    gps_lon      REAL,
    orientation  INTEGER,
    rating       INTEGER NOT NULL DEFAULT 0,
    favorite     INTEGER NOT NULL DEFAULT 0,
    trashed_at   TEXT
);
CREATE INDEX idx_photos_taken ON photos(taken_at);
CREATE INDEX idx_photos_rating ON photos(rating);
CREATE INDEX idx_photos_phash ON photos(phash);

CREATE TABLE tags (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE photo_tags (
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (photo_id, tag_id)
);
CREATE INDEX idx_photo_tags_tag ON photo_tags(tag_id);

CREATE TABLE albums (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    cover_photo_id INTEGER REFERENCES photos(id) ON DELETE SET NULL
);
CREATE TABLE album_photos (
    album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (album_id, photo_id)
);
CREATE INDEX idx_album_photos_photo ON album_photos(photo_id);

CREATE TABLE imports (
    id            INTEGER PRIMARY KEY,
    source_dir    TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    finished_at   TEXT,
    added         INTEGER NOT NULL DEFAULT 0,
    skipped_dupes INTEGER NOT NULL DEFAULT 0,
    failed        INTEGER NOT NULL DEFAULT 0
);
"#,
    // v2: link photos to the import that added them; keep per-file import errors.
    r#"
ALTER TABLE photos ADD COLUMN import_id INTEGER REFERENCES imports(id);
ALTER TABLE imports ADD COLUMN cancelled INTEGER NOT NULL DEFAULT 0;
CREATE INDEX idx_photos_import ON photos(import_id);

CREATE TABLE import_errors (
    import_id INTEGER NOT NULL REFERENCES imports(id) ON DELETE CASCADE,
    path      TEXT NOT NULL,
    error     TEXT NOT NULL
);
CREATE INDEX idx_import_errors_import ON import_errors(import_id);

-- Existing photos: attribute each to the import whose time window contains it.
UPDATE photos SET import_id = (
    SELECT i.id FROM imports i
    WHERE photos.imported_at >= i.started_at
      AND photos.imported_at <= COALESCE(i.finished_at, '9999')
    ORDER BY i.id DESC LIMIT 1
);
"#,
    // v3: culling marks, photo source annotation, album folders.
    r#"
ALTER TABLE photos ADD COLUMN mark TEXT;
ALTER TABLE photos ADD COLUMN source TEXT;
ALTER TABLE photos ADD COLUMN home_album_id INTEGER REFERENCES albums(id) ON DELETE SET NULL;
CREATE INDEX idx_photos_mark ON photos(mark);
CREATE INDEX idx_photos_home ON photos(home_album_id);

-- Folder name under albums/; assigned when the album first holds a photo.
ALTER TABLE albums ADD COLUMN dir TEXT;
CREATE UNIQUE INDEX idx_albums_dir ON albums(dir COLLATE NOCASE);

-- What marks 1-4 do when run ('x' always moves to trash).
CREATE TABLE mark_actions (
    slot   INTEGER PRIMARY KEY CHECK (slot BETWEEN 1 AND 4),
    label  TEXT NOT NULL DEFAULT '',
    kind   TEXT NOT NULL CHECK (kind IN ('copy', 'album', 'tag')),
    target TEXT NOT NULL DEFAULT ''
);
"#,
    // v4: indexes matching the photo grid's filter + sort, so large libraries page quickly.
    r#"
CREATE INDEX idx_photos_live_taken ON photos(trashed_at, taken_at, id);
CREATE INDEX idx_photos_live_imported ON photos(trashed_at, imported_at, id);
"#,
    // v5: albums are no longer folders on disk (files stay in originals/YYYY/MM/DD), so
    // photos.home_album_id and albums.dir are unused. Kept as columns: SQLite can't drop
    // a column with a foreign key.
    r#"
UPDATE photos SET home_album_id = NULL;
UPDATE albums SET dir = NULL;
"#,
    // v6: imports that turned each source folder into an album.
    r#"
ALTER TABLE imports ADD COLUMN folder_albums INTEGER NOT NULL DEFAULT 0;
"#,
    // v7: content hashes of photos removed by emptying the trash, so importing the same
    // files again skips them.
    r#"
CREATE TABLE deleted_hashes (
    hash       TEXT PRIMARY KEY,
    orig_name  TEXT NOT NULL,
    deleted_at TEXT NOT NULL
);
ALTER TABLE imports ADD COLUMN skipped_deleted INTEGER NOT NULL DEFAULT 0;
"#,
    // v8: files seen by earlier imports, keyed by their path inside the chosen folder plus
    // size and modified time, so importing the same card again needn't read them to hash.
    r#"
CREATE TABLE import_seen (
    rel_path TEXT NOT NULL,
    size     INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    hash     TEXT NOT NULL,
    PRIMARY KEY (rel_path, size, mtime_ns)
) WITHOUT ROWID;
"#,
    // v9: lens set by hand (manual lenses record none in EXIF); overrides photos.lens.
    r#"
ALTER TABLE photos ADD COLUMN lens_set TEXT;
"#,
    // v10: free-text notes: a description, and how the photo was shot.
    r#"
ALTER TABLE photos ADD COLUMN note TEXT;
ALTER TABLE photos ADD COLUMN settings_note TEXT;
"#,
    // v11: the user's own lens list (manual lenses), kept even when no photo uses a lens.
    r#"
CREATE TABLE lenses (name TEXT PRIMARY KEY COLLATE NOCASE) WITHOUT ROWID;
INSERT OR IGNORE INTO lenses (name) SELECT DISTINCT lens_set FROM photos WHERE lens_set IS NOT NULL;
"#,
    // v12: the photographer's private notes, never shared or published.
    r#"
ALTER TABLE photos ADD COLUMN private_note TEXT;
"#,
    // v13: topics group albums (Travel, Family…); an album is in at most one topic.
    r#"
CREATE TABLE topics (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL
);
ALTER TABLE albums ADD COLUMN topic_id INTEGER REFERENCES topics(id) ON DELETE SET NULL;
"#,
    // v14: sections split an album into named parts (Kyoto, Nara…); a photo is in at most
    // one section of each album.
    r#"
CREATE TABLE album_sections (
    id       INTEGER PRIMARY KEY,
    album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    position INTEGER NOT NULL
);
CREATE INDEX idx_album_sections_album ON album_sections(album_id);
ALTER TABLE album_photos ADD COLUMN section_id INTEGER REFERENCES album_sections(id) ON DELETE SET NULL;
ALTER TABLE imports ADD COLUMN folder_sections INTEGER NOT NULL DEFAULT 0;
"#,
    // v15: backups to other disks. Each target records which photos it holds, where, and
    // the hash the copy was checked against. Removing a photo or target only drops the
    // record; files on the backup disk are never deleted.
    r#"
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
CREATE TABLE backup_targets (
    id               INTEGER PRIMARY KEY,
    name             TEXT NOT NULL,
    path             TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    last_run_at      TEXT,
    last_verified_at TEXT
);
CREATE TABLE backup_copies (
    target_id   INTEGER NOT NULL REFERENCES backup_targets(id) ON DELETE CASCADE,
    photo_id    INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    rel_path    TEXT NOT NULL,
    hash        TEXT NOT NULL,
    verified_at TEXT NOT NULL,
    PRIMARY KEY (target_id, photo_id)
) WITHOUT ROWID;
CREATE INDEX idx_backup_copies_photo ON backup_copies(photo_id);
"#,
    // v16: backups to cloud storage through rclone; `path` is then `remote:folder`.
    r#"
ALTER TABLE backup_targets ADD COLUMN kind TEXT NOT NULL DEFAULT 'disk';
"#,
    // v17: an album's sections are either by date (ordered by their first photo) or by
    // topic (ordered by name); album_sections.position is no longer used.
    r#"
ALTER TABLE albums ADD COLUMN section_kind TEXT NOT NULL DEFAULT 'date';
"#,
    // v18: sections nest (Kyoto › Day 2 › Morning); photos only go in sections without
    // sub-sections. Deleting a section deletes the ones inside it.
    r#"
ALTER TABLE album_sections ADD COLUMN parent_id INTEGER REFERENCES album_sections(id) ON DELETE CASCADE;
CREATE INDEX idx_album_sections_parent ON album_sections(parent_id);
"#,
    // v19: an organized-folders import can make the chosen folder itself the one album.
    r#"
ALTER TABLE imports ADD COLUMN single_album INTEGER NOT NULL DEFAULT 0;
"#,
    // v20: albums and topics pinned to the sidebar, in the order they were pinned.
    r#"
CREATE TABLE shortcuts (
    id       INTEGER PRIMARY KEY,
    album_id INTEGER UNIQUE REFERENCES albums(id) ON DELETE CASCADE,
    topic_id INTEGER UNIQUE REFERENCES topics(id) ON DELETE CASCADE,
    CHECK ((album_id IS NULL) != (topic_id IS NULL))
);
"#,
    // v21: what each folder became in an organized-folders import (JSON: path -> role), so
    // adding the folder again can start from the same choices.
    r#"
ALTER TABLE imports ADD COLUMN folder_plan TEXT;
"#,
    // v22: topics become categories, which nest (Travel › Japan); a category holds albums and
    // other categories. Sections ordered by topic are now said to be ordered by name.
    r#"
ALTER TABLE topics RENAME TO categories;
ALTER TABLE categories ADD COLUMN parent_id INTEGER REFERENCES categories(id) ON DELETE SET NULL;
CREATE INDEX idx_categories_parent ON categories(parent_id);
ALTER TABLE albums RENAME COLUMN topic_id TO category_id;
ALTER TABLE shortcuts RENAME COLUMN topic_id TO category_id;
UPDATE albums SET section_kind = 'name' WHERE section_kind = 'topic';
"#,
    // v23: albums nest (Japan › Kyoto › Day 2) and replace both categories and sections. An
    // album holds photos, other albums, or both; `display` says whether its page shows the
    // albums inside as tiles or their photos inline under a heading each, and `child_order`
    // orders them ("date": oldest first, "newest", "name"). Categories become albums shown as
    // tiles; sections become albums inside their album (or section), taking its photos, except
    // "Other" sections, whose photos go back to the section above. Rebuilds the album tables
    // to drop the old columns (see `REBUILDS`).
    r#"
ALTER TABLE albums ADD COLUMN parent_id INTEGER;
ALTER TABLE albums ADD COLUMN display TEXT NOT NULL DEFAULT 'inline';
ALTER TABLE albums ADD COLUMN from_category INTEGER;
ALTER TABLE albums ADD COLUMN from_section INTEGER;

INSERT INTO albums (name, created_at, section_kind, display, from_category)
    SELECT name, created_at, 'newest', 'tiles', id FROM categories ORDER BY id;
UPDATE albums SET parent_id =
    (SELECT up.id FROM categories c JOIN albums up ON up.from_category = c.parent_id WHERE c.id = albums.from_category)
    WHERE from_category IS NOT NULL;
UPDATE albums SET parent_id = (SELECT up.id FROM albums up WHERE up.from_category = albums.category_id)
    WHERE category_id IS NOT NULL;
UPDATE shortcuts SET album_id = (SELECT a.id FROM albums a WHERE a.from_category = shortcuts.category_id), category_id = NULL
    WHERE category_id IS NOT NULL;

INSERT INTO albums (name, created_at, section_kind, from_section)
    SELECT s.name, a.created_at, a.section_kind, s.id FROM album_sections s JOIN albums a ON a.id = s.album_id ORDER BY s.id;
UPDATE albums SET parent_id = COALESCE(
    (SELECT up.id FROM album_sections s JOIN albums up ON up.from_section = s.parent_id WHERE s.id = albums.from_section),
    (SELECT s.album_id FROM album_sections s WHERE s.id = albums.from_section))
    WHERE from_section IS NOT NULL;
INSERT OR IGNORE INTO album_photos (album_id, photo_id, position)
    SELECT a.id, ap.photo_id, ap.position FROM album_photos ap JOIN albums a ON a.from_section = ap.section_id;
DELETE FROM album_photos WHERE section_id IS NOT NULL;

CREATE TEMP TABLE other_albums AS
    SELECT a.id, a.parent_id FROM albums a JOIN album_sections s ON s.id = a.from_section
    WHERE s.name = 'Other' COLLATE NOCASE AND s.parent_id IS NOT NULL
      AND NOT EXISTS (SELECT 1 FROM album_sections c WHERE c.parent_id = s.id);
INSERT OR IGNORE INTO album_photos (album_id, photo_id, position)
    SELECT o.parent_id, ap.photo_id, ap.position FROM album_photos ap JOIN other_albums o ON o.id = ap.album_id;
DELETE FROM album_photos WHERE album_id IN (SELECT id FROM other_albums);
DELETE FROM albums WHERE id IN (SELECT id FROM other_albums);
DROP TABLE other_albums;

CREATE TABLE albums_new (
    id             INTEGER PRIMARY KEY,
    name           TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    cover_photo_id INTEGER REFERENCES photos(id) ON DELETE SET NULL,
    parent_id      INTEGER REFERENCES albums(id) ON DELETE SET NULL,
    child_order    TEXT NOT NULL DEFAULT 'date',
    display        TEXT NOT NULL DEFAULT 'inline'
);
INSERT INTO albums_new (id, name, created_at, cover_photo_id, parent_id, child_order, display)
    SELECT id, name, created_at, cover_photo_id, parent_id, section_kind, display FROM albums;
CREATE TABLE album_photos_new (
    album_id INTEGER NOT NULL REFERENCES albums(id) ON DELETE CASCADE,
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (album_id, photo_id)
);
INSERT INTO album_photos_new (album_id, photo_id, position) SELECT album_id, photo_id, position FROM album_photos;
CREATE TABLE shortcuts_new (
    id       INTEGER PRIMARY KEY,
    album_id INTEGER NOT NULL UNIQUE REFERENCES albums(id) ON DELETE CASCADE
);
INSERT INTO shortcuts_new (id, album_id) SELECT id, album_id FROM shortcuts WHERE album_id IS NOT NULL;

DROP TABLE shortcuts;
DROP TABLE album_photos;
DROP TABLE album_sections;
DROP TABLE categories;
DROP TABLE albums;
ALTER TABLE albums_new RENAME TO albums;
ALTER TABLE album_photos_new RENAME TO album_photos;
ALTER TABLE shortcuts_new RENAME TO shortcuts;
CREATE INDEX idx_albums_parent ON albums(parent_id);
CREATE INDEX idx_album_photos_photo ON album_photos(photo_id);
"#,
];

/// Migrations (by the version they bring the library to) that rebuild tables. They run with
/// foreign keys off, so dropping an old table doesn't cascade into the rows that point at it;
/// the keys are checked before committing. A copy of the library is kept first.
const REBUILDS: &[usize] = &[23];

pub fn open(path: &Path) -> Result<Connection> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA foreign_keys = ON;
         PRAGMA synchronous = NORMAL;",
    )?;
    register_functions(&conn)?;
    migrate(&mut conn, path)?;
    Ok(conn)
}

/// `device_kind(make, model)` -> 'phone' | 'camera' | 'unknown', usable in queries.
fn register_functions(conn: &Connection) -> Result<()> {
    conn.create_scalar_function(
        "device_kind",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let make: Option<String> = ctx.get(0)?;
            let model: Option<String> = ctx.get(1)?;
            Ok(device_kind(make.as_deref(), model.as_deref()))
        },
    )?;
    Ok(())
}

const PHONE_MAKES: &[&str] = &[
    "apple", "google", "samsung", "xiaomi", "redmi", "huawei", "honor", "oneplus", "oppo", "vivo", "realme",
    "motorola", "nokia", "htc", "asus", "zte", "meizu", "lge", "nothing",
];
const PHONE_MODELS: &[&str] = &["iphone", "ipad", "pixel", "galaxy", "xperia", "sm-"];

/// Guesses whether a photo came from a phone or a dedicated camera from its EXIF make/model.
pub fn device_kind(make: Option<&str>, model: Option<&str>) -> &'static str {
    let make = make.unwrap_or("").trim().to_ascii_lowercase();
    let model = model.unwrap_or("").trim().to_ascii_lowercase();
    if make.is_empty() && model.is_empty() {
        return "unknown";
    }
    let phone = PHONE_MAKES.iter().any(|m| make.starts_with(m)) || PHONE_MODELS.iter().any(|m| model.contains(m));
    if phone { "phone" } else { "camera" }
}

fn migrate(conn: &mut Connection, path: &Path) -> Result<()> {
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let rebuild = REBUILDS.contains(&(i + 1));
        if rebuild {
            // A new library has nothing to lose.
            if version > 0 {
                let backup = path.with_file_name(format!("library.v{i}.bak.db"));
                if !backup.exists() {
                    conn.execute("VACUUM INTO ?1", [backup.to_string_lossy()])?;
                }
            }
            conn.pragma_update(None, "foreign_keys", false)?;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        if rebuild {
            let broken: Option<String> = tx.query_row("PRAGMA foreign_key_check", [], |r| r.get(0)).optional()?;
            if let Some(table) = broken {
                return Err(AppError::msg(format!("updating the library left broken links in {table}")));
            }
        }
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
        if rebuild {
            conn.pragma_update(None, "foreign_keys", true)?;
        }
    }
    Ok(())
}
