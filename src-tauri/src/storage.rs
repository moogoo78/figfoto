//! Where photo files live: `originals/YYYY/MM/DD/`, by the date taken. Albums exist only
//! in the library DB, so a photo can be in any number of them and never moves.
//! Trashed photos keep the same relative path under the trash dir.
//!
//! Libraries from before this layout kept album photos in `albums/<album>/`;
//! [`move_out_of_album_folders`] moves them back to their date folders.

use std::path::Path;

use rusqlite::params;

use crate::error::{AppError, Result};
use crate::import::reserve_dest;
use crate::library::Library;

/// `originals/YYYY/MM/DD` for a `taken_at` timestamp.
fn date_dir(taken_at: &str) -> String {
    format!("originals/{}", taken_at.get(..10).unwrap_or("unknown").replace('-', "/"))
}

/// Moves photos still stored under `albums/` (live or trashed) to their date folders and
/// removes the emptied album folders. Keeps going on errors and reports them together.
pub fn move_out_of_album_folders(lib: &Library) -> Result<usize> {
    let photos: Vec<(i64, String, String, String, bool)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT id, rel_path, orig_name, taken_at, trashed_at IS NOT NULL
             FROM photos WHERE rel_path LIKE 'albums/%'",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut errors = Vec::new();
    for (id, rel, name, taken_at, trashed) in &photos {
        if let Err(e) = relocate(lib, *id, rel, name, &date_dir(taken_at), *trashed) {
            errors.push(format!("{name}: {e}"));
        }
    }
    for base in [lib.root.clone(), lib.trash_dir()] {
        let _ = std::fs::remove_dir(base.join("albums"));
    }
    match errors.len() {
        0 => Ok(photos.len()),
        n => Err(AppError::msg(format!("{n} photo(s) could not be moved: {}", errors.join("; ")))),
    }
}

fn relocate(lib: &Library, id: i64, rel: &str, orig_name: &str, dir: &str, trashed: bool) -> Result<()> {
    let base = if trashed { lib.trash_dir() } else { lib.root.clone() };
    let from = base.join(rel);
    if !from.is_file() {
        return Err(AppError::msg(format!("{} is missing", from.display())));
    }
    let dest_dir = base.join(dir);
    std::fs::create_dir_all(&dest_dir)?;
    // Reserve a free name, then move the photo over the empty placeholder.
    let name = if orig_name.is_empty() { rel.rsplit('/').next().unwrap_or("photo") } else { orig_name };
    let (dest, _) = reserve_dest(&dest_dir, name)?;
    if let Err(e) = std::fs::rename(&from, &dest) {
        let _ = std::fs::remove_file(&dest);
        return Err(e.into());
    }
    let new_rel = format!("{dir}/{}", dest.file_name().unwrap().to_string_lossy());
    if let Err(e) = lib.conn().execute("UPDATE photos SET rel_path = ?1 WHERE id = ?2", params![new_rel, id]) {
        let _ = std::fs::rename(&dest, &from);
        return Err(e.into());
    }
    prune_empty_dirs(&base, from.parent());
    Ok(())
}

/// Removes now-empty folders above a moved file, stopping at the top-level folders.
fn prune_empty_dirs(base: &Path, mut dir: Option<&Path>) {
    let stops = [base.to_path_buf(), base.join("originals"), base.join("albums")];
    while let Some(d) = dir {
        if stops.iter().any(|s| s == d) || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}
