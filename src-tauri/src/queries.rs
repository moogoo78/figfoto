use std::collections::HashMap;

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
    /// In an album shown inline: by the album inside it each photo is in (in the order of
    /// `list_albums`; the album's own photos last), then date taken. Elsewhere the same as
    /// `TakenAsc`.
    Children,
}

impl Sort {
    /// The sort actually used for this filter (albums inside only show inside an album).
    fn resolve(self, f: &Filter) -> Sort {
        if self == Sort::Children && f.album_id.is_none() { Sort::TakenAsc } else { self }
    }

    fn order_by(self, lib: &Library, f: &Filter) -> Result<String> {
        Ok(match self.resolve(f) {
            Sort::TakenDesc => "p.taken_at DESC, p.id DESC".into(),
            Sort::TakenAsc => "p.taken_at ASC, p.id ASC".into(),
            Sort::ImportedDesc => "p.imported_at DESC, p.id DESC".into(),
            Sort::RatingDesc => "p.rating DESC, p.taken_at DESC, p.id DESC".into(),
            Sort::Children => format!("{}, p.taken_at ASC, p.id ASC", children_rank(lib, f.album_id.unwrap_or_default())?.0),
        })
    }

    fn groups_by_day(self) -> bool {
        matches!(self, Sort::TakenDesc | Sort::TakenAsc)
    }
}

/// SQL for the place of photo `p` among the albums inside `album_id` (in `list_albums` order;
/// a photo in several is placed by the first), with the album's own photos last; and those
/// albums, in that order. Only an album shown inline has its albums' photos on its page, so
/// for any other there are none. The ids are integers, so formatting them into the SQL is safe.
fn children_rank(lib: &Library, album_id: i64) -> Result<(String, Vec<Album>)> {
    let albums = list_albums(lib)?;
    let inline = albums.iter().any(|a| a.id == album_id && a.display == "inline");
    let inside: Vec<Album> = if inline { albums_inside(&albums, album_id) } else { Vec::new() };
    if inside.is_empty() {
        return Ok(("NULL".into(), inside));
    }
    let whens: String = inside.iter().enumerate().map(|(i, a)| format!(" WHEN {} THEN {i}", a.id)).collect();
    let ids: String = inside.iter().map(|a| format!(",{}", a.id)).collect();
    Ok((
        format!(
            "(SELECT MIN(CASE ap.album_id{whens} ELSE {} END) FROM album_photos ap
              WHERE ap.photo_id = p.id AND ap.album_id IN ({album_id}{ids}))",
            i64::MAX
        ),
        inside,
    ))
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
        // An album shown inline has the photos of the albums inside it too, at any depth.
        clauses.push(
            "p.id IN (SELECT ap.photo_id FROM album_photos ap WHERE ap.album_id IN (
               WITH RECURSIVE sub(id) AS (
                   SELECT id FROM albums WHERE id = ?
                 UNION
                   SELECT a.id FROM albums a JOIN sub ON a.parent_id = sub.id
                   WHERE (SELECT display FROM albums WHERE id = ?) = 'inline')
               SELECT id FROM sub))"
                .into(),
        );
        args.push(Box::new(a));
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
        sort.order_by(lib, f)?
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
    let sql = format!("SELECT p.id FROM photos p WHERE {wh} ORDER BY {} LIMIT ? OFFSET ?", sort.order_by(lib, f)?);
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

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Bucket {
    /// `YYYY-MM-DD`, or empty when the sort doesn't group by day.
    pub day: String,
    pub count: i64,
    /// When sorted by the albums inside: that album's id, or 0 for the album's own photos.
    pub album: Option<i64>,
    /// When sorted by the albums inside: the album's name (empty for the album's own photos).
    pub label: String,
    /// When sorted by the albums inside: how deep it is (0 right inside the album shown).
    pub depth: i64,
    /// When sorted by the albums inside: the album it is in, unless that is the album shown.
    pub parent: Option<i64>,
    /// When sorted by the albums inside: number of albums directly inside it.
    pub children: i64,
    /// When sorted by the albums inside: photos in it and the albums inside it (`count` is
    /// only the ones placed directly in it).
    pub total: i64,
}

/// Photo counts per day (or per album inside), in the same order `list_photos` returns them.
pub fn date_buckets(lib: &Library, f: &Filter, sort: Sort) -> Result<Vec<Bucket>> {
    let sort = sort.resolve(f);
    if sort == Sort::Children {
        return children_buckets(lib, f, f.album_id.unwrap_or_default());
    }
    let (wh, args) = where_clause(f);
    let conn = lib.conn();
    if !sort.groups_by_day() {
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM photos p WHERE {wh}"),
            params_from_iter(args.iter()),
            |r| r.get(0),
        )?;
        return Ok(if count > 0 { vec![Bucket { count, ..Default::default() }] } else { vec![] });
    }
    let dir = if sort == Sort::TakenAsc { "ASC" } else { "DESC" };
    let sql = format!(
        "SELECT substr(p.taken_at, 1, 10) AS day, COUNT(*) FROM photos p WHERE {wh}
         GROUP BY day ORDER BY day {dir}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(args.iter()), |r| {
        Ok(Bucket { day: r.get(0)?, count: r.get(1)?, ..Default::default() })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// One bucket per album inside the album, in `list_albums` order (empty ones too, so they can
/// be managed), then the album's own photos.
fn children_buckets(lib: &Library, f: &Filter, album_id: i64) -> Result<Vec<Bucket>> {
    let (rank, inside) = children_rank(lib, album_id)?;
    let counts: HashMap<i64, i64> = {
        let (wh, args) = where_clause(f);
        let conn = lib.conn();
        let mut stmt = conn.prepare(&format!("SELECT r, COUNT(*) FROM (SELECT {rank} AS r FROM photos p WHERE {wh}) GROUP BY r"))?;
        let rows = stmt.query_map(params_from_iter(args.iter()), |r| Ok((r.get::<_, Option<i64>>(0)?.unwrap_or(i64::MAX), r.get(1)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let base = inside.first().map_or(0, |a| a.depth);
    let mut out: Vec<Bucket> = inside
        .into_iter()
        .enumerate()
        .map(|(i, a)| {
            let count = counts.get(&(i as i64)).copied().unwrap_or(0);
            Bucket {
                count,
                album: Some(a.id),
                label: a.name,
                depth: a.depth - base,
                parent: a.parent_id.filter(|p| *p != album_id),
                children: a.children,
                total: count,
                ..Default::default()
            }
        })
        .collect();
    // Albums inside come right after the one they're in, so adding each total to its
    // parent's from the end up sums whole subtrees.
    let index: HashMap<i64, usize> = out.iter().enumerate().map(|(i, b)| (b.album.unwrap_or_default(), i)).collect();
    for i in (0..out.len()).rev() {
        if let Some(&p) = out[i].parent.and_then(|p| index.get(&p)) {
            out[p].total += out[i].total;
        }
    }
    match counts.get(&i64::MAX) {
        // An album with no albums inside (or shown as tiles) is just one plain group.
        Some(&count) if out.is_empty() => out.push(Bucket { count, total: count, ..Default::default() }),
        Some(&count) => out.push(Bucket { count, total: count, album: Some(0), ..Default::default() }),
        None => {}
    }
    Ok(out)
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
    /// The album it is inside, if any.
    pub parent_id: Option<i64>,
    /// How deep it is (0 at the top).
    pub depth: i64,
    /// Photos directly in it.
    pub count: i64,
    /// Photos in it and every album inside it (each counted once).
    pub total: i64,
    /// Number of albums directly inside it.
    pub children: i64,
    /// Its chosen cover, else its first photo, else the cover of the first album inside it.
    pub cover: Option<String>,
    /// How the albums inside are ordered: "date" (oldest first), "newest" or "name".
    pub child_order: String,
    /// How its page shows the albums inside: "inline" (their photos, under a heading each)
    /// or "tiles".
    pub display: String,
    /// When pinned to the sidebar: its place among the shortcuts (lower first).
    pub shortcut: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhotoDetail {
    pub photo: Photo,
    pub tags: Vec<Tag>,
    pub albums: Vec<Album>,
    /// Each of `albums` with the albums it is inside ("Japan › Kyoto"), keyed by album id.
    pub paths: HashMap<i64, String>,
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
    let in_albums: Vec<i64> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare("SELECT album_id FROM album_photos WHERE photo_id = ?1")?;
        let rows = stmt.query_map([id], |r| r.get(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let all = list_albums(lib)?;
    let paths = in_albums.iter().map(|&a| (a, album_path(&all, a))).collect();
    let albums = all.into_iter().filter(|a| in_albums.contains(&a.id)).collect();
    Ok(PhotoDetail { photo, tags, albums, paths })
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

/// Every album as a tree, each followed by the albums inside it. The top level is newest
/// first (by the latest photo in it or inside it, an empty album by when it was made); the
/// albums inside each album are in its `child_order`.
pub fn list_albums(lib: &Library) -> Result<Vec<Album>> {
    struct Row {
        album: Album,
        own_cover: Option<String>,
        created_at: String,
        children: Vec<usize>,
        photos: std::collections::HashSet<i64>,
        first: Option<String>,
        last: Option<String>,
    }
    let mut rows: Vec<Row> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT a.id, a.name, a.created_at, a.parent_id, a.child_order, a.display,
                (SELECT id FROM shortcuts WHERE album_id = a.id),
                COALESCE(
                  (SELECT hash FROM photos WHERE id = a.cover_photo_id AND trashed_at IS NULL),
                  (SELECT p.hash FROM album_photos ap JOIN photos p ON p.id = ap.photo_id
                     WHERE ap.album_id = a.id AND p.trashed_at IS NULL
                     ORDER BY ap.position, p.taken_at LIMIT 1))
             FROM albums a ORDER BY a.id",
        )?;
        let rows = stmt.query_map([], |r| {
            let created_at: String = r.get(2)?;
            Ok(Row {
                album: Album {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: created_at.clone(),
                    parent_id: r.get(3)?,
                    depth: 0,
                    count: 0,
                    total: 0,
                    children: 0,
                    cover: None,
                    child_order: r.get(4)?,
                    display: r.get(5)?,
                    shortcut: r.get(6)?,
                },
                own_cover: r.get::<_, Option<String>>(7)?.map(|h| lib.thumb_path(&h).to_string_lossy().into_owned()),
                created_at,
                children: Vec::new(),
                photos: Default::default(),
                first: None,
                last: None,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let index: HashMap<i64, usize> = rows.iter().enumerate().map(|(i, r)| (r.album.id, i)).collect();
    {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT ap.album_id, p.id, p.taken_at FROM album_photos ap JOIN photos p ON p.id = ap.photo_id
             WHERE p.trashed_at IS NULL",
        )?;
        let pairs = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, String>(2)?)))?;
        for pair in pairs {
            let (album, photo, taken) = pair?;
            let Some(&i) = index.get(&album) else { continue };
            let row = &mut rows[i];
            row.album.count += 1;
            row.photos.insert(photo);
            if row.first.as_ref().is_none_or(|f| &taken < f) {
                row.first = Some(taken.clone());
            }
            if row.last.as_ref().is_none_or(|l| &taken > l) {
                row.last = Some(taken);
            }
        }
    }
    let mut roots = Vec::new();
    for i in 0..rows.len() {
        // An album whose parent is gone (or that would be inside itself) shows at the top.
        match rows[i].album.parent_id.and_then(|p| index.get(&p)).filter(|&&p| p != i) {
            Some(&p) => rows[p].children.push(i),
            None => roots.push(i),
        }
    }
    // Photos, dates and covers of each subtree, the albums inside before the one they're in.
    // `seen` guards against a parent loop, which `move_album` doesn't allow anyway.
    fn sum(rows: &mut [Row], i: usize, seen: &mut Vec<bool>) {
        seen[i] = true;
        for c in rows[i].children.clone() {
            if seen[c] {
                continue;
            }
            sum(rows, c, seen);
            let (photos, first, last) = (rows[c].photos.clone(), rows[c].first.clone(), rows[c].last.clone());
            let row = &mut rows[i];
            row.photos.extend(photos);
            if first.is_some() && (row.first.is_none() || first < row.first) {
                row.first = first;
            }
            if last > row.last {
                row.last = last;
            }
        }
    }
    let mut seen = vec![false; rows.len()];
    for &r in &roots {
        sum(&mut rows, r, &mut seen);
    }
    let order = |list: &mut Vec<usize>, rows: &[Row], how: &str| {
        let name = |i: usize| (rows[i].album.name.to_lowercase(), rows[i].album.id);
        match how {
            "name" => list.sort_by_cached_key(|&i| name(i)),
            "date" => list.sort_by_cached_key(|&i| (rows[i].first.is_none(), rows[i].first.clone(), name(i))),
            _ => list.sort_by_cached_key(|&i| {
                let latest = rows[i].last.clone().unwrap_or_else(|| rows[i].created_at.clone());
                (std::cmp::Reverse(latest), name(i))
            }),
        }
    };
    fn walk(rows: &mut [Row], list: &[usize], depth: i64, out: &mut Vec<usize>, order: &dyn Fn(&mut Vec<usize>, &[Row], &str)) {
        for &i in list {
            if out.contains(&i) {
                continue;
            }
            rows[i].album.depth = depth;
            out.push(i);
            let mut children = rows[i].children.clone();
            let how = rows[i].album.child_order.clone();
            order(&mut children, rows, &how);
            walk(rows, &children, depth + 1, out, order);
        }
    }
    order(&mut roots, &rows, "newest");
    let mut sequence = Vec::with_capacity(rows.len());
    walk(&mut rows, &roots, 0, &mut sequence, &order);
    // Covers: an album without photos of its own takes the first cover found inside it.
    for &i in sequence.iter().rev() {
        let inside = rows[i].children.clone();
        let mut ordered: Vec<usize> = sequence.iter().copied().filter(|j| inside.contains(j)).collect();
        ordered.retain(|&j| rows[j].album.cover.is_some());
        let row = &rows[i];
        let cover = row.own_cover.clone().or_else(|| ordered.first().and_then(|&j| rows[j].album.cover.clone()));
        let (total, children) = (row.photos.len() as i64, row.children.len() as i64);
        let album = &mut rows[i].album;
        album.cover = cover;
        album.total = total;
        album.children = children;
    }
    Ok(sequence.into_iter().map(|i| rows[i].album.clone()).collect())
}

/// The albums inside `id`, at any depth, in `list_albums` order.
pub fn albums_inside(albums: &[Album], id: i64) -> Vec<Album> {
    let Some(start) = albums.iter().position(|a| a.id == id) else { return Vec::new() };
    let depth = albums[start].depth;
    albums[start + 1..].iter().take_while(|a| a.depth > depth).cloned().collect()
}

/// "Japan › Kyoto": the album's name after the albums it is inside.
pub fn album_path(albums: &[Album], id: i64) -> String {
    let mut names = Vec::new();
    let mut at = Some(id);
    while let Some(a) = at.and_then(|id| albums.iter().find(|a| a.id == id)) {
        if names.len() > albums.len() {
            break;
        }
        names.push(a.name.as_str());
        at = a.parent_id;
    }
    names.reverse();
    names.join(" › ")
}

/// The album and every album inside it, at any depth.
fn album_and_inside(conn: &Connection, id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "WITH RECURSIVE sub(id) AS (SELECT ?1 UNION SELECT a.id FROM albums a JOIN sub ON a.parent_id = sub.id)
         SELECT id FROM sub",
    )?;
    let rows = stmt.query_map([id], |r| r.get(0))?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Pins an album to the sidebar, after the ones already there, or unpins it.
pub fn set_shortcut(lib: &Library, album_id: i64, pinned: bool) -> Result<()> {
    let conn = lib.conn();
    if pinned {
        conn.execute("INSERT OR IGNORE INTO shortcuts (album_id) VALUES (?1)", [album_id])?;
    } else {
        conn.execute("DELETE FROM shortcuts WHERE album_id = ?1", [album_id])?;
    }
    Ok(())
}

/// A new album inside `parent` (at the top level with `None`).
pub fn create_album(lib: &Library, name: &str, parent: Option<i64>) -> Result<i64> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("album name is empty"));
    }
    let conn = lib.conn();
    conn.execute("INSERT INTO albums (name, created_at, parent_id) VALUES (?1, ?2, ?3)", params![name, now(), parent])?;
    Ok(conn.last_insert_rowid())
}

/// The album named `name` (ignoring case) inside `parent` (at the top level with `None`),
/// created if there is none.
pub fn find_or_create_album(lib: &Library, name: &str, parent: Option<i64>) -> Result<i64> {
    let existing = lib
        .conn()
        .query_row(
            "SELECT id FROM albums WHERE name = ?1 COLLATE NOCASE AND parent_id IS ?2 ORDER BY id LIMIT 1",
            params![name.trim(), parent],
            |r| r.get(0),
        )
        .optional()?;
    match existing {
        Some(id) => Ok(id),
        None => create_album(lib, name, parent),
    }
}

/// The album at `path` (names from the top down), each one created if missing.
pub fn find_or_create_album_path(lib: &Library, path: &[String]) -> Result<Option<i64>> {
    let mut parent = None;
    for name in path {
        parent = Some(find_or_create_album(lib, name, parent)?);
    }
    Ok(parent)
}

pub fn rename_album(lib: &Library, id: i64, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("album name is empty"));
    }
    lib.conn().execute("UPDATE albums SET name = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

/// Moves an album inside another, or to the top level with `None`. An album can't go inside
/// itself or one of the albums inside it.
pub fn move_album(lib: &Library, id: i64, parent: Option<i64>) -> Result<()> {
    let conn = lib.conn();
    if parent.is_some_and(|p| album_and_inside(&conn, id).is_ok_and(|ids| ids.contains(&p))) {
        return Err(AppError::msg("an album can't go inside itself"));
    }
    conn.execute("UPDATE albums SET parent_id = ?1 WHERE id = ?2", params![parent, id])?;
    Ok(())
}

/// Deletes an album; its photos stay in the library. The albums inside it, and for an album
/// inside another its photos too, move up to the album above (or the top level).
pub fn delete_album(lib: &Library, id: i64) -> Result<()> {
    let mut conn = lib.conn();
    let tx = conn.transaction()?;
    let parent: Option<i64> = tx.query_row("SELECT parent_id FROM albums WHERE id = ?1", [id], |r| r.get(0)).optional()?.flatten();
    tx.execute("UPDATE albums SET parent_id = ?1 WHERE parent_id = ?2", params![parent, id])?;
    if let Some(parent) = parent {
        tx.execute(
            "INSERT OR IGNORE INTO album_photos (album_id, photo_id, position)
             SELECT ?1, photo_id, position + (SELECT COALESCE(MAX(position), -1) + 1 FROM album_photos WHERE album_id = ?1)
             FROM album_photos WHERE album_id = ?2",
            [parent, id],
        )?;
    }
    tx.execute("DELETE FROM albums WHERE id = ?1", [id])?;
    tx.commit()?;
    Ok(())
}

/// Sets how the albums inside an album are ordered: "date" (oldest first), "newest" or "name".
pub fn set_child_order(lib: &Library, album_id: i64, order: &str) -> Result<()> {
    if !matches!(order, "date" | "newest" | "name") {
        return Err(AppError::msg("unknown album order"));
    }
    lib.conn().execute("UPDATE albums SET child_order = ?1 WHERE id = ?2", params![order, album_id])?;
    Ok(())
}

/// Sets how an album's page shows the albums inside: "inline" or "tiles".
pub fn set_album_display(lib: &Library, album_id: i64, display: &str) -> Result<()> {
    if !matches!(display, "inline" | "tiles") {
        return Err(AppError::msg("unknown album display"));
    }
    lib.conn().execute("UPDATE albums SET display = ?1 WHERE id = ?2", params![display, album_id])?;
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

/// Takes photos out of an album, and with `inside` out of every album inside it too.
pub fn remove_from_album(lib: &Library, album_id: i64, ids: &[i64], inside: bool) -> Result<()> {
    let conn = lib.conn();
    let albums = if inside { album_and_inside(&conn, album_id)? } else { vec![album_id] };
    conn.execute(
        &format!("DELETE FROM album_photos WHERE album_id IN ({}) AND photo_id IN ({})", id_list(&albums), id_list(ids)),
        [],
    )?;
    Ok(())
}

/// Moves photos shown on album `from`'s page (in it or an album inside it) into album `to`.
pub fn move_photos(lib: &Library, from: i64, to: i64, ids: &[i64]) -> Result<()> {
    remove_from_album(lib, from, ids, true)?;
    add_to_album(lib, to, ids)
}

pub fn set_album_cover(lib: &Library, album_id: i64, photo_id: i64) -> Result<()> {
    lib.conn().execute("UPDATE albums SET cover_photo_id = ?1 WHERE id = ?2", [photo_id, album_id])?;
    Ok(())
}

/// Moves the album's own photos (not those in albums inside it) into one album inside it per
/// day taken (`YYYY-MM-DD`, reusing an album of that name). Returns how many days there were.
pub fn split_by_day(lib: &Library, album_id: i64) -> Result<usize> {
    let mut days: Vec<(String, Vec<i64>)> = Vec::new();
    {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT substr(p.taken_at, 1, 10), p.id FROM album_photos ap JOIN photos p ON p.id = ap.photo_id
             WHERE ap.album_id = ?1 ORDER BY p.taken_at, p.id",
        )?;
        let rows = stmt.query_map([album_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (day, id) = row?;
            match days.last_mut() {
                Some((d, ids)) if *d == day => ids.push(id),
                _ => days.push((day, vec![id])),
            }
        }
    }
    for (day, ids) in &days {
        let inside = find_or_create_album(lib, day, Some(album_id))?;
        remove_from_album(lib, album_id, ids, false)?;
        add_to_album(lib, inside, ids)?;
    }
    Ok(days.len())
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
    /// Live photos on no backup disk; 0 while no backup is set up.
    pub unbacked: i64,
}

pub fn counts(lib: &Library) -> Result<Counts> {
    Ok(lib.conn().query_row(
        "SELECT
            COALESCE(SUM(trashed_at IS NULL), 0),
            COALESCE(SUM(trashed_at IS NULL AND favorite = 1), 0),
            COALESCE(SUM(trashed_at IS NOT NULL), 0),
            COALESCE(SUM(trashed_at IS NULL AND mark IS NOT NULL), 0),
            CASE WHEN EXISTS (SELECT 1 FROM backup_targets) THEN COALESCE(SUM(trashed_at IS NULL
                AND NOT EXISTS (SELECT 1 FROM backup_copies c WHERE c.photo_id = photos.id)), 0) ELSE 0 END
         FROM photos",
        [],
        |r| Ok(Counts { all: r.get(0)?, favorites: r.get(1)?, trash: r.get(2)?, marked: r.get(3)?, unbacked: r.get(4)? }),
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
    /// Subfolders were turned into sections of those albums.
    pub folder_sections: bool,
    /// The source folder itself was the one album, its subfolders sections.
    pub single_album: bool,
    /// What each folder became (path inside the source folder -> role), for adding it again.
    pub folder_plan: Option<HashMap<String, crate::import::FolderRole>>,
    /// Photos from this import still in the library (not trashed).
    pub photo_count: i64,
    pub cover: Option<String>,
}

pub fn list_imports(lib: &Library) -> Result<Vec<ImportRecord>> {
    let conn = lib.conn();
    let mut stmt = conn.prepare(
        "SELECT i.id, i.source_dir, i.started_at, i.finished_at, i.added, i.skipped_dupes, i.failed, i.cancelled, i.folder_albums, i.skipped_deleted, i.folder_sections,
            i.single_album, i.folder_plan,
            (SELECT COUNT(*) FROM photos p WHERE p.import_id = i.id AND p.trashed_at IS NULL),
            (SELECT p.hash FROM photos p WHERE p.import_id = i.id AND p.trashed_at IS NULL
               ORDER BY p.taken_at LIMIT 1)
         FROM imports i ORDER BY i.id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        let cover: Option<String> = r.get(14)?;
        let plan: Option<String> = r.get(12)?;
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
            folder_sections: r.get(10)?,
            single_album: r.get(11)?,
            folder_plan: plan.and_then(|p| serde_json::from_str(&p).ok()),
            photo_count: r.get(13)?,
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
