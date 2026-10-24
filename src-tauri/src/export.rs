//! A CSV list of every photo in the library: where it was added from, where it is now, its
//! dates, albums and tags. Trashed photos and private notes are left out.

use std::io::Write;
use std::path::Path;

use crate::error::Result;
use crate::library::Library;

const HEADER: [&str; 15] = [
    "original_path", "library_path", "datetime", "adjusted_datetime", "date_source", "albums", "tags",
    "camera_make", "camera_model", "lens", "iso", "aperture", "exposure_time", "focal_length", "file_name",
];

/// Writes the CSV to `dest` and returns how many photos it lists. Albums are full paths
/// (`Travel / Japan / 190503_Japan`); a photo in several albums, or with several tags, has them
/// separated by `; `. `datetime` is the date the photo came with (EXIF, or its file's modified
/// time); `adjusted_datetime` is set only when that was changed. `lens` is the one set by hand
/// for a manual lens, else the one the camera recorded. `aperture` is the f-number (2.8) and
/// `focal_length` is in mm, as the camera recorded them; `exposure_time` is written as in the app
/// (1/250, 2s).
pub fn photos_csv(lib: &Library, dest: &Path) -> Result<usize> {
    let rows: Vec<[String; 15]> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "WITH RECURSIVE album_path(id, path) AS (
                SELECT id, name FROM albums WHERE parent_id IS NULL
                UNION ALL
                SELECT a.id, ap.path || ' / ' || a.name FROM albums a JOIN album_path ap ON a.parent_id = ap.id
             )
             SELECT p.source_path, p.rel_path, p.taken_at, p.original_taken_at, p.date_source,
                (SELECT group_concat(path, '; ') FROM (SELECT ap.path FROM album_photos x
                    JOIN album_path ap ON ap.id = x.album_id WHERE x.photo_id = p.id ORDER BY ap.path)),
                (SELECT group_concat(name, '; ') FROM (SELECT t.name FROM photo_tags pt
                    JOIN tags t ON t.id = pt.tag_id WHERE pt.photo_id = p.id ORDER BY t.name)),
                p.camera_make, p.camera_model, COALESCE(NULLIF(p.lens_set, ''), p.lens),
                p.iso, p.f_number, p.exposure, p.focal_len, p.orig_name
             FROM photos p WHERE p.trashed_at IS NULL ORDER BY p.taken_at, p.id",
        )?;
        let rows = stmt.query_map([], |r| {
            let text = |i| r.get::<_, Option<String>>(i).map(Option::unwrap_or_default);
            // To one decimal, without a trailing ".0": 2.8, 50.
            let number = |i| r.get::<_, Option<f64>>(i).map(|v| v.map(|v| ((v * 10.0).round() / 10.0).to_string()).unwrap_or_default());
            let (taken, original) = (text(2)?, r.get::<_, Option<String>>(3)?);
            let (datetime, adjusted) = match original {
                Some(o) if o != taken => (o, taken),
                _ => (taken, String::new()),
            };
            let rel: String = r.get(1)?;
            Ok([
                text(0)?,
                lib.photo_path(&rel, false).to_string_lossy().into_owned(),
                datetime.replace('T', " "),
                adjusted.replace('T', " "),
                text(4)?,
                text(5)?,
                text(6)?,
                text(7)?,
                text(8)?,
                text(9)?,
                number(10)?,
                number(11)?,
                text(12)?,
                number(13)?,
                text(14)?,
            ])
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut out = std::io::BufWriter::new(std::fs::File::create(dest)?);
    // A byte-order mark, so spreadsheet apps read the names as UTF-8.
    out.write_all("\u{feff}".as_bytes())?;
    writeln!(out, "{}", HEADER.join(","))?;
    for row in &rows {
        writeln!(out, "{}", row.iter().map(|f| field(f)).collect::<Vec<_>>().join(","))?;
    }
    out.flush()?;
    Ok(rows.len())
}

/// A CSV field, quoted when it holds a comma, quote or line break.
fn field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
