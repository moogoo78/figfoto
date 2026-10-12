use rusqlite::types::ToSql;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::import::now;
use crate::library::Library;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Photo {
    pub id: i64,
    pub hash: String,
    pub rel_path: String,
    pub orig_name: String,
    pub source_path: Option<String>,
    pub taken_at: String,
    pub imported_at: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub file_size: i64,
    pub mime: Option<String>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    /// Set by hand if present, otherwise from EXIF.
    pub lens: Option<String>,
    /// Whether `lens` was set by hand (and can be reset to the EXIF value).
    pub lens_set: bool,
    /// Whether the camera recorded a lens; such photos can't be given one by hand.
    pub exif_lens: bool,
    pub iso: Option<i64>,
    pub f_number: Option<f64>,
    pub exposure: Option<String>,
    pub focal_len: Option<f64>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub rating: i64,
    pub favorite: bool,
    pub trashed_at: Option<String>,
    /// Culling mark: "x" (to delete) or "1".."4" (custom actions).
    pub mark: Option<String>,
    /// Where the photo came from, set by the user (e.g. "LINE", "Facebook").
    pub source: Option<String>,
    /// Free-text description of the photo.
    pub note: Option<String>,
    /// Free-text notes on how it was shot (filter, tripod, bulb time…).
    pub settings_note: Option<String>,
    /// The photographer's own record. Private: never shared, exported or published.
    pub private_note: Option<String>,
    /// "phone", "camera" or "unknown", guessed from EXIF make/model.
    pub device: &'static str,
    /// Absolute paths, filled in from the library root. `display` is what the
    /// viewer shows: the original, or a JPEG preview for formats like HEIC.
    pub path: String,
    pub display: String,
    pub thumb: String,
}

const PHOTO_COLS: &str = "p.id, p.hash, p.rel_path, p.orig_name, p.source_path, p.taken_at, p.imported_at,
    p.width, p.height, p.file_size, p.mime, p.camera_make, p.camera_model, COALESCE(p.lens_set, p.lens), p.iso, p.f_number,
    p.exposure, p.focal_len, p.gps_lat, p.gps_lon, p.rating, p.favorite, p.trashed_at, p.mark, p.source,
    p.lens_set IS NOT NULL, p.note, p.settings_note,
    COALESCE(p.lens, '') != '', p.private_note";

fn photo_from_row(lib: &Library, r: &Row) -> rusqlite::Result<Photo> {
    let hash: String = r.get(1)?;
    let rel_path: String = r.get(2)?;
    let trashed_at: Option<String> = r.get(22)?;
    let path = lib.photo_path(&rel_path, trashed_at.is_some());
    let display = if crate::import::needs_preview(&path) { lib.preview_path(&hash) } else { path.clone() };
    let camera_make: Option<String> = r.get(11)?;
    let camera_model: Option<String> = r.get(12)?;
    Ok(Photo {
        id: r.get(0)?,
        path: path.to_string_lossy().into_owned(),
        display: display.to_string_lossy().into_owned(),
        thumb: lib.thumb_path(&hash).to_string_lossy().into_owned(),
        hash,
        rel_path,
        orig_name: r.get(3)?,
        source_path: r.get(4)?,
        taken_at: r.get(5)?,
        imported_at: r.get(6)?,
        width: r.get(7)?,
        height: r.get(8)?,
        file_size: r.get(9)?,
        mime: r.get(10)?,
        device: crate::db::device_kind(camera_make.as_deref(), camera_model.as_deref()),
        camera_make,
        camera_model,
        lens: r.get(13)?,
        iso: r.get(14)?,
        f_number: r.get(15)?,
        exposure: r.get(16)?,
        focal_len: r.get(17)?,
        gps_lat: r.get(18)?,
        gps_lon: r.get(19)?,
        rating: r.get(20)?,
        favorite: r.get(21)?,
        trashed_at,
        mark: r.get(23)?,
        source: r.get(24)?,
        lens_set: r.get(25)?,
        note: r.get(26)?,
        settings_note: r.get(27)?,
        exif_lens: r.get(28)?,
        private_note: r.get(29)?,
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Filter {
    /// Inclusive `YYYY-MM-DD` bounds on `taken_at`.
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    /// Photo must carry all of these tags.
    pub tag_ids: Vec<i64>,
    pub album_id: Option<i64>,
    pub import_id: Option<i64>,
    pub min_rating: Option<i64>,
    pub favorite: Option<bool>,
    pub camera: Option<String>,
    pub text: Option<String>,
    /// A mark ("x", "1".."4"), or "any" for every marked photo.
    pub mark: Option<String>,
    /// The user-set source if any, else "phone" / "camera" / "unknown".
    pub origin: Option<String>,
    pub trashed: bool,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Sort {
    #[default]
    TakenDesc,
    TakenAsc,
    ImportedDesc,
    RatingDesc,
}

impl Sort {
    fn order_by(self) -> &'static str {
        match self {
            Sort::TakenDesc => "p.taken_at DESC, p.id DESC",
            Sort::TakenAsc => "p.taken_at ASC, p.id ASC",
            Sort::ImportedDesc => "p.imported_at DESC, p.id DESC",
            Sort::RatingDesc => "p.rating DESC, p.taken_at DESC, p.id DESC",
        }
    }

    fn groups_by_day(self) -> bool {
        matches!(self, Sort::TakenDesc | Sort::TakenAsc)
    }
}

/// Builds the WHERE clause (and its parameters) for a filter.
fn where_clause(f: &Filter) -> (String, Vec<Box<dyn ToSql>>) {
    let mut clauses: Vec<String> = vec![if f.trashed {
        "p.trashed_at IS NOT NULL".into()
    } else {
        "p.trashed_at IS NULL".into()
    }];
    let mut args: Vec<Box<dyn ToSql>> = Vec::new();

    if let Some(d) = f.date_from.as_ref().filter(|s| !s.is_empty()) {
        clauses.push("p.taken_at >= ?".into());
        args.push(Box::new(d.clone()));
    }
    if let Some(d) = f.date_to.as_ref().filter(|s| !s.is_empty()) {
        clauses.push("p.taken_at < date(?, '+1 day')".into());
        args.push(Box::new(d.clone()));
    }
    if !f.tag_ids.is_empty() {
        let marks = vec!["?"; f.tag_ids.len()].join(",");
        clauses.push(format!(
            "p.id IN (SELECT photo_id FROM photo_tags WHERE tag_id IN ({marks})
                      GROUP BY photo_id HAVING COUNT(*) = {})",
            f.tag_ids.len()
        ));
        args.extend(f.tag_ids.iter().map(|t| Box::new(*t) as Box<dyn ToSql>));
    }
    if let Some(a) = f.album_id {
        clauses.push("p.id IN (SELECT photo_id FROM album_photos WHERE album_id = ?)".into());
        args.push(Box::new(a));
    }
    if let Some(i) = f.import_id {
        clauses.push("p.import_id = ?".into());
        args.push(Box::new(i));
    }
    if let Some(r) = f.min_rating.filter(|r| *r > 0) {
        clauses.push("p.rating >= ?".into());
        args.push(Box::new(r));
    }
    if let Some(fav) = f.favorite {
        clauses.push("p.favorite = ?".into());
        args.push(Box::new(fav));
    }
    if let Some(c) = f.camera.as_ref().filter(|s| !s.is_empty()) {
        clauses.push("p.camera_model = ?".into());
        args.push(Box::new(c.clone()));
    }
    if let Some(t) = f.text.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        clauses.push(
            "(p.orig_name LIKE ? ESCAPE '\\' OR p.rel_path LIKE ? ESCAPE '\\'
              OR p.note LIKE ? ESCAPE '\\' OR p.settings_note LIKE ? ESCAPE '\\'
              OR p.private_note LIKE ? ESCAPE '\\')"
                .into(),
        );
        let pat = format!("%{}%", t.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
        for _ in 0..5 {
            args.push(Box::new(pat.clone()));
        }
    }
    if let Some(m) = f.mark.as_ref().filter(|s| !s.is_empty()) {
        if m == "any" {
            clauses.push("p.mark IS NOT NULL".into());
        } else {
            clauses.push("p.mark = ?".into());
            args.push(Box::new(m.clone()));
        }
    }
    if let Some(o) = f.origin.as_ref().filter(|s| !s.is_empty()) {
        clauses.push(format!("{ORIGIN} = ?"));
        args.push(Box::new(o.clone()));
    }
    (clauses.join(" AND "), args)
}

const ORIGIN: &str = "COALESCE(p.source, device_kind(p.camera_make, p.camera_model))";

pub fn list_photos(lib: &Library, f: &Filter, sort: Sort, offset: i64, limit: i64) -> Result<Vec<Photo>> {
    let (wh, mut args) = where_clause(f);
    args.push(Box::new(limit));
    args.push(Box::new(offset));
    let sql = format!(
        "SELECT {PHOTO_COLS} FROM photos p WHERE {wh} ORDER BY {} LIMIT ? OFFSET ?",
        sort.order_by()
    );
    let conn = lib.conn();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), |r| photo_from_row(lib, r))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Like `list_photos`, but only the ids (e.g. for select-all on a large library).
pub fn list_ids(lib: &Library, f: &Filter, sort: Sort, offset: i64, limit: i64) -> Result<Vec<i64>> {
    let (wh, mut args) = where_clause(f);
    args.push(Box::new(limit));
    args.push(Box::new(offset));
    let sql = format!("SELECT p.id FROM photos p WHERE {wh} ORDER BY {} LIMIT ? OFFSET ?", sort.order_by());
    let conn = lib.conn();
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// First and last day (`YYYY-MM-DD`) the given photos were taken, or `None` for no photos.
pub fn date_span(lib: &Library, ids: &[i64]) -> Result<Option<(String, String)>> {
    let span = lib.conn().query_row(
        &format!(
            "SELECT MIN(substr(taken_at, 1, 10)), MAX(substr(taken_at, 1, 10)) FROM photos WHERE id IN ({})",
            id_list(ids)
        ),
        [],
        |r| Ok(r.get::<_, Option<String>>(0)?.zip(r.get::<_, Option<String>>(1)?)),
    )?;
    Ok(span)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Bucket {
    /// `YYYY-MM-DD`, or empty when the sort doesn't group by day.
    pub day: String,
    pub count: i64,
}

/// Photo counts per day, in the same order `list_photos` returns them.
pub fn date_buckets(lib: &Library, f: &Filter, sort: Sort) -> Result<Vec<Bucket>> {
    let (wh, args) = where_clause(f);
    let conn = lib.conn();
    if !sort.groups_by_day() {
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM photos p WHERE {wh}"),
            params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        return Ok(if count > 0 { vec![Bucket { day: String::new(), count }] } else { vec![] });
    }
    let dir = if sort == Sort::TakenAsc { "ASC" } else { "DESC" };
    let sql = format!(
        "SELECT substr(p.taken_at, 1, 10) AS day, COUNT(*) FROM photos p WHERE {wh}
         GROUP BY day ORDER BY day {dir}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
        Ok(Bucket { day: r.get(0)?, count: r.get(1)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Album {
    pub id: i64,
    pub name: String,
    pub created_at: String,
    pub count: i64,
    pub cover: Option<String>,
    /// The topic the album is filed under, if any.
    pub topic_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhotoDetail {
    pub photo: Photo,
    pub tags: Vec<Tag>,
    pub albums: Vec<Album>,
}

pub fn get_photo(lib: &Library, id: i64) -> Result<PhotoDetail> {
    let photo = {
        let conn = lib.conn();
        conn.query_row(&format!("SELECT {PHOTO_COLS} FROM photos p WHERE p.id = ?1"), [id], |r| {
            photo_from_row(lib, r)
        })
        .optional()?
        .ok_or_else(|| AppError::msg(format!("photo {id} not found")))?
    };
    let tags = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, 0 FROM tags t JOIN photo_tags pt ON pt.tag_id = t.id
             WHERE pt.photo_id = ?1 ORDER BY t.name",
        )?;
        let rows = stmt.query_map([id], |r| Ok(Tag { id: r.get(0)?, name: r.get(1)?, count: r.get(2)? }))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let album_ids: std::collections::HashSet<i64> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare("SELECT album_id FROM album_photos WHERE photo_id = ?1")?;
        let rows = stmt.query_map([id], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let albums = list_albums(lib)?.into_iter().filter(|a| album_ids.contains(&a.id)).collect();
    Ok(PhotoDetail { photo, tags, albums })
}

fn id_list(ids: &[i64]) -> String {
    ids.iter().map(|i| i.to_string()).collect::<Vec<_>>().join(",")
}

pub fn set_rating(lib: &Library, ids: &[i64], rating: i64) -> Result<()> {
    if !(0..=5).contains(&rating) {
        return Err(AppError::msg("rating must be 0-5"));
    }
    lib.conn().execute(&format!("UPDATE photos SET rating = ?1 WHERE id IN ({})", id_list(ids)), [rating])?;
    Ok(())
}

pub fn set_favorite(lib: &Library, ids: &[i64], favorite: bool) -> Result<()> {
    lib.conn()
        .execute(&format!("UPDATE photos SET favorite = ?1 WHERE id IN ({})", id_list(ids)), [favorite])?;
    Ok(())
}

/// Favorites the photos, or unfavorites them if they all are already. Returns the new state.
pub fn toggle_favorite(lib: &Library, ids: &[i64]) -> Result<bool> {
    let not_fav: i64 = lib.conn().query_row(
        &format!("SELECT COUNT(*) FROM photos WHERE favorite = 0 AND id IN ({})", id_list(ids)),
        [],
        |r| r.get(0),
    )?;
    let favorite = not_fav > 0;
    set_favorite(lib, ids, favorite)?;
    Ok(favorite)
}

// ---- tags ----

pub fn list_tags(lib: &Library) -> Result<Vec<Tag>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, COUNT(p.id) FROM tags t
         LEFT JOIN photo_tags pt ON pt.tag_id = t.id
         LEFT JOIN photos p ON p.id = pt.photo_id AND p.trashed_at IS NULL
         GROUP BY t.id ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| Ok(Tag { id: r.get(0)?, name: r.get(1)?, count: r.get(2)? }))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn add_tags(lib: &Library, ids: &[i64], names: &[String]) -> Result<()> {
    let mut conn = lib.conn();
    let tx = conn.transaction()?;
    for name in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        tx.execute("INSERT OR IGNORE INTO tags (name) VALUES (?1)", [name])?;
        let tag_id: i64 = tx.query_row("SELECT id FROM tags WHERE name = ?1", [name], |r| r.get(0))?;
        for id in ids {
            tx.execute("INSERT OR IGNORE INTO photo_tags (photo_id, tag_id) VALUES (?1, ?2)", [id, &tag_id])?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub fn remove_tag(lib: &Library, ids: &[i64], tag_id: i64) -> Result<()> {
    let conn = lib.conn();
    conn.execute(
        &format!("DELETE FROM photo_tags WHERE tag_id = ?1 AND photo_id IN ({})", id_list(ids)),
        [tag_id],
    )?;
    // Drop tags that are no longer used anywhere.
    conn.execute("DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM photo_tags)", [])?;
    Ok(())
}

// ---- albums ----

pub fn list_albums(lib: &Library) -> Result<Vec<Album>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, a.created_at,
            (SELECT COUNT(*) FROM album_photos ap JOIN photos p ON p.id = ap.photo_id
               WHERE ap.album_id = a.id AND p.trashed_at IS NULL),
            COALESCE(
              (SELECT hash FROM photos WHERE id = a.cover_photo_id AND trashed_at IS NULL),
              (SELECT p.hash FROM album_photos ap JOIN photos p ON p.id = ap.photo_id
                 WHERE ap.album_id = a.id AND p.trashed_at IS NULL
                 ORDER BY ap.position, p.taken_at LIMIT 1)),
            a.topic_id
         FROM albums a ORDER BY a.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        let cover: Option<String> = r.get(4)?;
        Ok(Album {
            id: r.get(0)?,
            name: r.get(1)?,
            created_at: r.get(2)?,
            count: r.get(3)?,
            cover: cover.map(|h| lib.thumb_path(&h).to_string_lossy().into_owned()),
            topic_id: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// A group of albums, like "Travel" or "Family".
#[derive(Debug, Clone, Serialize)]
pub struct Topic {
    pub id: i64,
    pub name: String,
    /// Number of albums in the topic.
    pub albums: i64,
    /// Cover of the first album (by name) that has one.
    pub cover: Option<String>,
}

pub fn list_topics(lib: &Library) -> Result<Vec<Topic>> {
    let topics: Vec<(i64, String)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare("SELECT id, name FROM topics ORDER BY name COLLATE NOCASE")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let albums = list_albums(lib)?;
    Ok(topics
        .into_iter()
        .map(|(id, name)| {
            let mine = albums.iter().filter(|a| a.topic_id == Some(id));
            Topic {
                id,
                name,
                albums: mine.clone().count() as i64,
                cover: mine.filter_map(|a| a.cover.clone()).next(),
            }
        })
        .collect())
}

/// The topic named `name` (ignoring case), created if there is none.
pub fn create_topic(lib: &Library, name: &str) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("topic name is empty"));
    }
    let conn = lib.conn();
    let existing = conn
        .query_row("SELECT id FROM topics WHERE name = ?1 COLLATE NOCASE ORDER BY id LIMIT 1", [name], |r| r.get(0))
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    conn.execute("INSERT INTO topics (name, created_at) VALUES (?1, ?2)", params![name, now()])?;
    Ok(conn.last_insert_rowid())
}

pub fn rename_topic(lib: &Library, id: i64, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("topic name is empty"));
    }
    lib.conn().execute("UPDATE topics SET name = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

/// Deletes a topic; its albums stay and move out of it.
pub fn delete_topic(lib: &Library, id: i64) -> Result<()> {
    lib.conn().execute("DELETE FROM topics WHERE id = ?1", [id])?;
    Ok(())
}

/// Files albums under a topic, or takes them out of their topic with `None`.
pub fn set_album_topic(lib: &Library, album_ids: &[i64], topic_id: Option<i64>) -> Result<()> {
    lib.conn().execute(
        &format!("UPDATE albums SET topic_id = ?1 WHERE id IN ({})", id_list(album_ids)),
        [topic_id],
    )?;
    Ok(())
}

pub fn create_album(lib: &Library, name: &str) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("album name is empty"));
    }
    let conn = lib.conn();
    conn.execute("INSERT INTO albums (name, created_at) VALUES (?1, ?2)", params![name, now()])?;
    Ok(conn.last_insert_rowid())
}

/// The album named `name` (ignoring case), created if there is none.
pub fn find_or_create_album(lib: &Library, name: &str) -> Result<i64> {
    let existing = lib
        .conn()
        .query_row(
            "SELECT id FROM albums WHERE name = ?1 COLLATE NOCASE ORDER BY id LIMIT 1",
            [name.trim()],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(id) => Ok(id),
        None => create_album(lib, name),
    }
}

pub fn rename_album(lib: &Library, id: i64, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("album name is empty"));
    }
    lib.conn().execute("UPDATE albums SET name = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

/// Deletes an album; its photos stay in the library.
pub fn delete_album(lib: &Library, id: i64) -> Result<()> {
    lib.conn().execute("DELETE FROM albums WHERE id = ?1", [id])?;
    Ok(())
}

pub fn add_to_album(lib: &Library, album_id: i64, ids: &[i64]) -> Result<()> {
    let mut conn = lib.conn();
    let tx = conn.transaction()?;
    let mut pos: i64 = tx.query_row(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM album_photos WHERE album_id = ?1",
        [album_id],
        |r| r.get(0),
    )?;
    for id in ids {
        pos += tx.execute(
            "INSERT OR IGNORE INTO album_photos (album_id, photo_id, position) VALUES (?1, ?2, ?3)",
            [album_id, *id, pos],
        )? as i64;
    }
    tx.commit()?;
    Ok(())
}

pub fn remove_from_album(lib: &Library, album_id: i64, ids: &[i64]) -> Result<()> {
    lib.conn().execute(
        &format!("DELETE FROM album_photos WHERE album_id = ?1 AND photo_id IN ({})", id_list(ids)),
        [album_id],
    )?;
    Ok(())
}

pub fn set_album_cover(lib: &Library, album_id: i64, photo_id: i64) -> Result<()> {
    lib.conn().execute("UPDATE albums SET cover_photo_id = ?1 WHERE id = ?2", [photo_id, album_id])?;
    Ok(())
}

/// Sets the lens by hand (for manual lenses that record none); empty clears it. Photos whose
/// camera recorded a lens are skipped. A new lens name is added to the saved lens list.
/// Returns how many photos were changed.
pub fn set_lens(lib: &Library, ids: &[i64], lens: &str) -> Result<usize> {
    let lens = Some(lens.trim()).filter(|s| !s.is_empty());
    let conn = lib.conn();
    let only_manual = if lens.is_some() { "AND COALESCE(lens, '') = ''" } else { "" };
    let n = conn.execute(
        &format!("UPDATE photos SET lens_set = ?1 WHERE id IN ({}) {only_manual}", id_list(ids)),
        [lens],
    )?;
    if let Some(name) = lens {
        conn.execute("INSERT OR IGNORE INTO lenses (name) VALUES (?1)", [name])?;
    }
    Ok(n)
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedLens {
    pub name: String,
    pub photos: i64,
}

/// Adds a lens to the saved list, ahead of using it.
pub fn add_lens(lib: &Library, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("lens name is empty"));
    }
    lib.conn().execute("INSERT OR IGNORE INTO lenses (name) VALUES (?1)", [name])?;
    Ok(())
}

/// Removes a lens from the saved list. Photos set to it keep it.
pub fn remove_lens(lib: &Library, name: &str) -> Result<()> {
    lib.conn().execute("DELETE FROM lenses WHERE name = ?1", [name])?;
    Ok(())
}

/// Renames a saved lens and every photo set to it (e.g. to fix a typo). Renaming onto
/// another saved lens merges the two.
pub fn rename_lens(lib: &Library, old: &str, new: &str) -> Result<()> {
    let new = new.trim();
    if new.is_empty() {
        return Err(AppError::msg("lens name is empty"));
    }
    let mut conn = lib.conn();
    let tx = conn.transaction()?;
    tx.execute("UPDATE photos SET lens_set = ?2 WHERE lens_set = ?1 COLLATE NOCASE", [old, new])?;
    tx.execute("DELETE FROM lenses WHERE name = ?1", [old])?;
    tx.execute("INSERT OR IGNORE INTO lenses (name) VALUES (?1)", [new])?;
    tx.commit()?;
    Ok(())
}

/// Sets a free-text note: `kind` is "note" (description), "settings" (camera settings) or
/// "private" (the photographer's private notes).
/// Empty clears it.
pub fn set_note(lib: &Library, ids: &[i64], kind: &str, text: &str) -> Result<()> {
    let col = match kind {
        "note" => "note",
        "settings" => "settings_note",
        "private" => "private_note",
        _ => return Err(AppError::msg(format!("unknown note kind: {kind}"))),
    };
    let text = Some(text.trim()).filter(|s| !s.is_empty());
    lib.conn().execute(&format!("UPDATE photos SET {col} = ?1 WHERE id IN ({})", id_list(ids)), [text])?;
    Ok(())
}

/// Distinct camera-settings notes, most used first, for suggestions.
pub fn list_settings_notes(lib: &Library) -> Result<Vec<String>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT settings_note FROM photos WHERE settings_note IS NOT NULL AND trashed_at IS NULL
         GROUP BY settings_note ORDER BY COUNT(*) DESC, settings_note LIMIT 50",
    )?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The saved lens list with how many photos use each. EXIF lenses aren't in it: manual
/// lenses are usually old ones the camera knows nothing about.
pub fn list_lenses(lib: &Library) -> Result<Vec<SavedLens>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT l.name, (SELECT COUNT(*) FROM photos p WHERE p.lens_set = l.name COLLATE NOCASE
                                                   AND p.trashed_at IS NULL)
         FROM lenses l ORDER BY l.name",
    )?;
    let rows = stmt.query_map([], |r| Ok(SavedLens { name: r.get(0)?, photos: r.get(1)? }))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn list_cameras(lib: &Library) -> Result<Vec<String>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT camera_model FROM photos
         WHERE camera_model IS NOT NULL AND trashed_at IS NULL ORDER BY camera_model",
    )?;
    let rows = stmt.query_map([], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

#[derive(Debug, Clone, Serialize)]
pub struct Counts {
    pub all: i64,
    pub favorites: i64,
    pub trash: i64,
    pub marked: i64,
}

pub fn counts(lib: &Library) -> Result<Counts> {
    Ok(lib.conn().query_row(
        "SELECT
            COALESCE(SUM(trashed_at IS NULL), 0),
            COALESCE(SUM(trashed_at IS NULL AND favorite = 1), 0),
            COALESCE(SUM(trashed_at IS NOT NULL), 0),
            COALESCE(SUM(trashed_at IS NULL AND mark IS NOT NULL), 0)
         FROM photos",
        [],
        |r| Ok(Counts { all: r.get(0)?, favorites: r.get(1)?, trash: r.get(2)?, marked: r.get(3)? }),
    )?)
}

// ---- import history ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRecord {
    pub id: i64,
    pub source_dir: String,
    pub started_at: String,
    /// None if the app quit before the import finished.
    pub finished_at: Option<String>,
    pub added: i64,
    pub skipped_dupes: i64,
    /// Skipped because they had been deleted from the library before.
    pub skipped_deleted: i64,
    pub failed: i64,
    pub cancelled: bool,
    /// Folders were turned into albums.
    pub folder_albums: bool,
    /// Photos from this import still in the library (not trashed).
    pub photo_count: i64,
    pub cover: Option<String>,
}

pub fn list_imports(lib: &Library) -> Result<Vec<ImportRecord>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT i.id, i.source_dir, i.started_at, i.finished_at, i.added, i.skipped_dupes, i.failed, i.cancelled, i.folder_albums, i.skipped_deleted,
            (SELECT COUNT(*) FROM photos p WHERE p.import_id = i.id AND p.trashed_at IS NULL),
            (SELECT p.hash FROM photos p WHERE p.import_id = i.id AND p.trashed_at IS NULL
               ORDER BY p.taken_at LIMIT 1)
         FROM imports i ORDER BY i.id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        let cover: Option<String> = r.get(11)?;
        Ok(ImportRecord {
            id: r.get(0)?,
            source_dir: r.get(1)?,
            started_at: r.get(2)?,
            finished_at: r.get(3)?,
            added: r.get(4)?,
            skipped_dupes: r.get(5)?,
            failed: r.get(6)?,
            cancelled: r.get(7)?,
            folder_albums: r.get(8)?,
            skipped_deleted: r.get(9)?,
            photo_count: r.get(10)?,
            cover: cover.map(|h| lib.thumb_path(&h).to_string_lossy().into_owned()),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn import_errors(lib: &Library, import_id: i64) -> Result<Vec<crate::import::ImportFailure>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare("SELECT path, error FROM import_errors WHERE import_id = ?1 ORDER BY path")?;
    let rows = stmt.query_map([import_id], |r| Ok(crate::import::ImportFailure { path: r.get(0)?, error: r.get(1)? }))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

// ---- duplicates & trash ----

pub fn find_duplicates(lib: &Library, threshold: u32) -> Result<Vec<Vec<Photo>>> {
    let items: Vec<(i64, u64)> = {
        let conn = lib.conn();
        let mut stmt =
            conn.prepare("SELECT id, phash FROM photos WHERE phash IS NOT NULL AND trashed_at IS NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)? as u64)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let groups = crate::dupes::group(&items, threshold);
    groups.into_iter().map(|ids| photos_by_ids(lib, &ids)).collect()
}

fn photos_by_ids(lib: &Library, ids: &[i64]) -> Result<Vec<Photo>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(&format!(
        "SELECT {PHOTO_COLS} FROM photos p WHERE p.id IN ({}) ORDER BY p.width * p.height DESC, p.file_size DESC",
        id_list(ids)
    ))?;
    let rows = stmt.query_map([], |r| photo_from_row(lib, r))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn rel_paths(conn: &Connection, ids: &[i64], trashed: bool) -> Result<Vec<(i64, String)>> {
    let cond = if trashed { "IS NOT NULL" } else { "IS NULL" };
    let mut stmt = conn.prepare(&format!(
        "SELECT id, rel_path FROM photos WHERE trashed_at {cond} AND id IN ({})",
        id_list(ids)
    ))?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn move_file(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
    if to.exists() {
        return Err(AppError::msg(format!("{} already exists", to.display())));
    }
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::rename(from, to)?;
    Ok(())
}

/// Moves photos into the library's trash folder (recoverable).
pub fn trash_photos(lib: &Library, ids: &[i64]) -> Result<()> {
    let items = rel_paths(&lib.conn(), ids, false)?;
    for (id, rel) in items {
        move_file(&lib.photo_path(&rel, false), &lib.photo_path(&rel, true))?;
        lib.conn().execute("UPDATE photos SET trashed_at = ?1 WHERE id = ?2", params![now(), id])?;
    }
    Ok(())
}

pub fn restore_photos(lib: &Library, ids: &[i64]) -> Result<()> {
    let items = rel_paths(&lib.conn(), ids, true)?;
    for (id, rel) in items {
        move_file(&lib.photo_path(&rel, true), &lib.photo_path(&rel, false))?;
        lib.conn().execute("UPDATE photos SET trashed_at = NULL WHERE id = ?1", [id])?;
    }
    Ok(())
}

/// Removes every trashed photo from the library. Files go to the system Trash (so they can
/// still be recovered from the desktop); thumbnails, previews and database rows are deleted.
pub fn empty_trash(lib: &Library) -> Result<usize> {
    empty_trash_with(lib, |path| trash::delete(path).map_err(|e| AppError::msg(format!("{}: {e}", path.display()))))
}

/// `empty_trash` with a custom way of disposing of each file (tests avoid the real Trash).
pub fn empty_trash_with(lib: &Library, discard: impl Fn(&std::path::Path) -> Result<()>) -> Result<usize> {
    let items: Vec<(i64, String, String, String)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare("SELECT id, rel_path, hash, orig_name FROM photos WHERE trashed_at IS NOT NULL")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for (id, rel, hash, name) in &items {
        let path = lib.photo_path(rel, true);
        if path.exists() {
            discard(&path)?;
        }
        let _ = std::fs::remove_file(lib.thumb_path(hash));
        let _ = std::fs::remove_file(lib.preview_path(hash));
        // Remember the content so importing the same file again (e.g. from the SD card) skips it.
        let mut conn = lib.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR REPLACE INTO deleted_hashes (hash, orig_name, deleted_at) VALUES (?1, ?2, ?3)",
            params![hash, name, now()],
        )?;
        tx.execute("DELETE FROM photos WHERE id = ?1", [id])?;
        tx.commit()?;
    }
    for dir in ["originals", "albums"] {
        prune_tree(&lib.trash_dir().join(dir));
    }
    Ok(items.len())
}

/// Removes empty folders below `dir` (not `dir` itself).
fn prune_tree(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            prune_tree(&e.path());
            let _ = std::fs::remove_dir(e.path());
        }
    }
}
