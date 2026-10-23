use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{self, BufReader, Read, Write};
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Local, NaiveDateTime};
use rayon::prelude::*;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{AppError, Result};
use crate::library::Library;
use crate::{exif, queries, thumbs};

pub const SUPPORTED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "tif", "tiff", "gif", "heic", "heif", "hif"];

/// Formats the webview (WebKitGTK) can't display; these get a full-size JPEG preview for the viewer.
const PREVIEW_EXTENSIONS: &[&str] = &["heic", "heif", "hif", "tif", "tiff"];

const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Progress events carry only the most recent errors, so a flood of failures stays cheap to send.
pub const MAX_SENT_ERRORS: usize = 50;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub import_id: i64,
    pub total: usize,
    pub done: usize,
    pub added: usize,
    pub skipped: usize,
    /// Skipped because they were deleted from the library before (trash emptied).
    pub skipped_deleted: usize,
    pub failed: usize,
    pub current: String,
    /// Still walking the source folder; `total` is the count found so far.
    pub scanning: bool,
    pub finished: bool,
    pub cancelled: bool,
    /// Albums photos were added to.
    pub albums: usize,
    pub errors: Vec<ImportFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Default)]
pub struct ImportOptions<'a> {
    /// Labels where the photos came from (e.g. "LINE").
    pub source: Option<&'a str>,
    /// Folders become albums (an existing album of the same name is reused); photos already in
    /// the library are added to them too. Without a `plan`, every folder that directly holds
    /// photos, the source folder included, becomes an album named after it.
    pub folder_albums: bool,
    /// With `folder_albums`: what each folder becomes, keyed by its path inside the source
    /// folder ("" for the source folder itself, "Kyoto/Day 2" below it). Folders not listed
    /// are [`FolderRole::Parent`].
    pub plan: Option<&'a HashMap<String, FolderRole>>,
    /// Import photos even if they were deleted from the library before.
    pub include_deleted: bool,
    /// One album per day: photos going to no album land in a `YYMMDD` album for their day,
    /// and a folder album spanning several days becomes a category of `YYMMDD_<folder>` albums.
    pub day_albums: bool,
}

/// What a folder becomes when adding organized folders with a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderRole {
    /// A category named after the folder, inside the category of the folder above (if any).
    /// Its own photos go into an album per day inside it with day albums, else into it.
    Category,
    /// An album named after the folder, inside the category of the folder above (if any).
    Album,
    /// A section named after the folder, inside the album (or section) of the folder above.
    /// Without an album above, it is an album.
    Section,
    /// Its photos go where the folder above's go.
    Parent,
    /// Not added, nor anything inside it.
    Skip,
}

/// A folder holding photos (directly or below), for planning an organized-folders import.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInfo {
    /// Path inside the scanned folder, "/"-separated; "" for the scanned folder itself.
    pub path: String,
    pub name: String,
    pub depth: usize,
    /// Photos directly in the folder.
    pub photos: usize,
    /// Photos in the folder and every folder below it.
    pub total: usize,
}

/// The folders under `src` that hold photos, each followed by the ones inside it (by name).
pub fn scan_folders(src: &Path) -> Result<Vec<FolderInfo>> {
    if !src.is_dir() {
        return Err(AppError::msg(format!("{} is not a folder", src.display())));
    }
    // Folder (as its path components) -> (photos directly in it, photos in it and below).
    let mut counts: BTreeMap<Vec<String>, (usize, usize)> = BTreeMap::new();
    let walker = WalkDir::new(src).follow_links(false).into_iter().filter_entry(|e| e.depth() == 0 || !is_ignored(e.file_name()));
    for entry in walker.filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() || !is_supported(entry.path()) {
            continue;
        }
        let Some(rel) = entry.path().parent().and_then(|d| d.strip_prefix(src).ok()) else { continue };
        let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
        for depth in 0..=parts.len() {
            let c = counts.entry(parts[..depth].to_vec()).or_default();
            c.1 += 1;
            if depth == parts.len() {
                c.0 += 1;
            }
        }
    }
    let root_name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(counts
        .into_iter()
        .map(|(parts, (photos, total))| FolderInfo {
            path: parts.join("/"),
            name: parts.last().cloned().unwrap_or_else(|| root_name.clone()),
            depth: parts.len(),
            photos,
            total,
        })
        .collect())
}

/// Where a folder's photos go under a plan.
enum Place {
    Skip,
    /// Added, but to no album.
    Loose,
    /// Into the album at this path (names and kinds, "category" or "album", from the top down).
    In(Vec<(String, &'static str)>),
}

/// Follows `plan` from `root` down to `dir`: each category, album or section nests in the one
/// above, a skipped folder skips everything inside it. Anything inside an album is a section
/// of it; a section with no album above is an album.
fn place(root: &Path, dir: &Path, plan: &HashMap<String, FolderRole>) -> Place {
    let Ok(rel) = dir.strip_prefix(root) else { return Place::Loose };
    let names: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    let mut albums = Vec::new();
    let mut in_album = false;
    for depth in 0..=names.len() {
        // A folder without a name (the root of a drive) can't name an album.
        let name = match depth {
            0 => root.file_name().map(|n| n.to_string_lossy().into_owned()),
            _ => Some(names[depth - 1].clone()),
        };
        match (plan.get(&names[..depth].join("/")).copied().unwrap_or(FolderRole::Parent), name) {
            (FolderRole::Skip, _) => return Place::Skip,
            (FolderRole::Category, Some(name)) if !in_album => albums.push((name, "category")),
            (FolderRole::Category | FolderRole::Album | FolderRole::Section, Some(name)) => {
                albums.push((name, "album"));
                in_album = true;
            }
            _ => {}
        }
    }
    if albums.is_empty() { Place::Loose } else { Place::In(albums) }
}

/// The album at `path`, each one created if missing (an existing one takes the kind asked
/// for when it can). An album that gets its first album inside this way orders them by name,
/// as folder names aren't dates.
fn album_at(lib: &Library, path: &[(String, &str)]) -> Result<i64> {
    let mut parent: Option<i64> = None;
    for (name, kind) in path {
        if let Some(p) = parent {
            let empty: bool = lib.conn().query_row("SELECT NOT EXISTS (SELECT 1 FROM albums WHERE parent_id = ?1)", [p], |r| r.get(0))?;
            if empty {
                queries::set_child_order(lib, p, "name")?;
            }
        }
        let id = queries::find_or_create_album(lib, name, parent, kind)?;
        // A category the user turned into an album stays one if it holds categories.
        let _ = queries::set_album_kind(lib, id, kind);
        parent = Some(id);
    }
    parent.ok_or_else(|| AppError::msg("no album to add to"))
}

enum Outcome {
    Added,
    Duplicate,
    /// Deleted from the library before; skipped unless `include_deleted`.
    Deleted,
}

fn extension(path: &Path) -> Option<String> {
    path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase())
}

pub fn is_supported(path: &Path) -> bool {
    extension(path).is_some_and(|e| SUPPORTED_EXTENSIONS.contains(&e.as_str()))
}

pub fn needs_preview(path: &Path) -> bool {
    extension(path).is_some_and(|e| PREVIEW_EXTENSIONS.contains(&e.as_str()))
}

/// Copies every supported image under `src` into the library. Source files are only read.
/// See [`ImportOptions`] for what can be adjusted.
/// `on_progress` is called (throttled) while running and once more when finished; it gets at
/// most the last `MAX_SENT_ERRORS` errors, while the returned progress and the database keep all.
pub fn run(
    lib: &Library,
    src: &Path,
    opts: &ImportOptions,
    cancel: &AtomicBool,
    on_progress: impl Fn(&ImportProgress) + Sync,
) -> Result<ImportProgress> {
    if !src.is_dir() {
        return Err(AppError::msg(format!("{} is not a folder", src.display())));
    }
    // Compare real paths so symlinks, `..` or a second mount point can't sneak the library in.
    let (real_src, real_root) = (std::fs::canonicalize(src)?, std::fs::canonicalize(&lib.root)?);
    if real_src.starts_with(&real_root) || real_root.starts_with(&real_src) {
        return Err(AppError::msg("cannot add photos from inside the library (or a parent of it)"));
    }

    let plan = opts.plan.filter(|_| opts.folder_albums);
    let import_id = {
        let conn = lib.conn();
        conn.execute(
            "INSERT INTO imports (source_dir, started_at, folder_albums, folder_sections, single_album, folder_plan)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                src.to_string_lossy(),
                now(),
                opts.folder_albums,
                false,
                plan.is_some_and(|p| p.values().filter(|r| **r == FolderRole::Album).count() == 1 && p.get("") == Some(&FolderRole::Album)),
                plan.map(|p| serde_json::to_string(p).unwrap()),
            ],
        )?;
        conn.last_insert_rowid()
    };

    let emit = |p: &ImportProgress| {
        if p.errors.len() <= MAX_SENT_ERRORS {
            on_progress(p);
        } else {
            let tail = p.errors[p.errors.len() - MAX_SENT_ERRORS..].to_vec();
            on_progress(&ImportProgress { current: p.current.clone(), errors: tail, ..*p });
        }
    };
    let last_emit = Mutex::new(Instant::now() - PROGRESS_INTERVAL);
    let due = || {
        let mut last = last_emit.lock().unwrap();
        let due = last.elapsed() >= PROGRESS_INTERVAL;
        if due {
            *last = Instant::now();
        }
        due
    };

    // Scan first, reporting how many files were found so far.
    let mut scan = ImportProgress { import_id, scanning: true, ..Default::default() };
    emit(&scan);
    let mut files = Vec::new();
    let walker = WalkDir::new(src).follow_links(false).into_iter().filter_entry(|e| e.depth() == 0 || !is_ignored(e.file_name()));
    for entry in walker {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match entry {
            Ok(e) if e.file_type().is_file() && is_supported(e.path()) => files.push(e.into_path()),
            Ok(_) => {}
            Err(e) => {
                let path = e.path().unwrap_or(src).to_string_lossy().into_owned();
                scan.failed += 1;
                scan.errors.push(ImportFailure { path, error: e.to_string() });
            }
        }
        if due() {
            scan.total = files.len();
            emit(&scan);
        }
    }

    let source = opts.source.map(str::trim).filter(|s| !s.is_empty());
    let album_of = match plan {
        Some(plan) => {
            let mut places = HashMap::new();
            for dir in files.iter().filter_map(|f| f.parent()) {
                places.entry(dir.to_path_buf()).or_insert_with(|| place(src, dir, plan));
            }
            files.retain(|f| !f.parent().is_some_and(|d| matches!(places.get(d), Some(Place::Skip))));
            let mut album_of = HashMap::new();
            for (dir, p) in places {
                if let Place::In(path) = p {
                    album_of.insert(dir, album_at(lib, &path)?);
                }
            }
            album_of
        }
        None if opts.folder_albums => folder_album_ids(lib, &files)?,
        None => HashMap::new(),
    };
    // Photos (by hash) that end up in each album and section, added once the copying is done.
    let album_hashes = Mutex::new(BTreeMap::<i64, Vec<String>>::new());
    // Photos going to no album, for day albums.
    let loose_hashes = Mutex::new(Vec::<String>::new());
    let progress = Mutex::new(ImportProgress { total: files.len(), scanning: false, ..scan });
    // Set when a problem makes every remaining file fail too (disk full, source gone).
    let stop = AtomicBool::new(false);
    let claimed = Mutex::new(HashSet::<String>::new());

    emit(&progress.lock().unwrap());

    files.par_iter().for_each(|path| {
        if cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
            return;
        }
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| import_one(lib, import_id, source, opts.include_deleted, src, path, &claimed)))
            .unwrap_or_else(|panic| Err(AppError::msg(format!("crashed while adding photos: {}", panic_message(&panic)))));
        if let Ok((Outcome::Added | Outcome::Duplicate, hash)) = &result {
            match path.parent().and_then(|d| album_of.get(d)) {
                Some(album) => album_hashes.lock().unwrap().entry(*album).or_default().push(hash.clone()),
                None if opts.day_albums => loose_hashes.lock().unwrap().push(hash.clone()),
                None => {}
            }
        }
        let fatal = result.as_ref().err().and_then(|e| fatal_reason(e, src));
        let mut p = progress.lock().unwrap();
        p.done += 1;
        p.current = path.to_string_lossy().into_owned();
        match result {
            Ok((Outcome::Added, _)) => p.added += 1,
            Ok((Outcome::Duplicate, _)) => p.skipped += 1,
            Ok((Outcome::Deleted, _)) => p.skipped_deleted += 1,
            Err(e) => {
                p.failed += 1;
                let path = p.current.clone();
                p.errors.push(ImportFailure { path, error: e.to_string() });
            }
        }
        if let Some(reason) = fatal {
            if !stop.swap(true, Ordering::Relaxed) {
                p.errors.push(ImportFailure { path: src.to_string_lossy().into_owned(), error: format!("adding photos stopped: {reason}") });
            }
        }
        if due() {
            emit(&p);
        }
    });

    let mut p = progress.into_inner().unwrap();
    let mut albums_done = HashSet::new();
    let fail = |p: &mut ImportProgress, e: AppError| {
        p.failed += 1;
        p.errors.push(ImportFailure { path: src.to_string_lossy().into_owned(), error: format!("adding to album: {e}") });
    };
    let mut targets: Vec<(i64, Vec<String>)> = Vec::new();
    for (album, hashes) in album_hashes.into_inner().unwrap() {
        if !opts.day_albums {
            targets.push((album, hashes));
            continue;
        }
        match category_targets(lib, album, hashes) {
            Ok(t) => targets.extend(t),
            Err(e) => fail(&mut p, e),
        }
    }
    let loose = loose_hashes.into_inner().unwrap();
    if !loose.is_empty() {
        match loose_day_targets(lib, &loose) {
            Ok(t) => targets.extend(t),
            Err(e) => fail(&mut p, e),
        }
    }
    for (album, hashes) in targets {
        match add_by_hash(lib, album, &hashes) {
            Ok(()) => {
                albums_done.insert(album);
            }
            Err(e) => fail(&mut p, e),
        }
    }
    p.albums = albums_done.len();
    p.finished = true;
    p.cancelled = cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed);
    {
        let mut conn = lib.conn();
        let tx = conn.transaction()?;
        tx.execute(
            "UPDATE imports SET finished_at = ?1, added = ?2, skipped_dupes = ?3, failed = ?4, cancelled = ?5,
                skipped_deleted = ?6
             WHERE id = ?7",
            params![now(), p.added, p.skipped, p.failed, p.cancelled, p.skipped_deleted, import_id],
        )?;
        for e in &p.errors {
            tx.execute(
                "INSERT INTO import_errors (import_id, path, error) VALUES (?1, ?2, ?3)",
                params![import_id, e.path, e.error],
            )?;
        }
        tx.commit()?;
    }
    emit(&p);
    Ok(p)
}

/// Folders and files that are never photos to import: hidden entries (macOS `._*` resource
/// forks, `.thumbnails`, another library's `.figfoto`), NAS thumbnail dirs and OS trash.
fn is_ignored(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy();
    name.starts_with('.') || ["@eaDir", "#recycle", "$RECYCLE.BIN", "System Volume Information"].contains(&name.as_ref())
}

/// Why the rest of the import is bound to fail too, if it is.
fn fatal_reason(e: &AppError, src: &Path) -> Option<&'static str> {
    let disk_full = match e {
        AppError::Io(e) => e.kind() == io::ErrorKind::StorageFull,
        AppError::Db(rusqlite::Error::SqliteFailure(e, _)) => e.code == rusqlite::ErrorCode::DiskFull,
        _ => false,
    };
    if disk_full {
        Some("the library disk is full")
    } else if !src.is_dir() {
        Some("the source folder is no longer available")
    } else {
        None
    }
}

fn panic_message(panic: &(dyn std::any::Any + Send)) -> &str {
    panic
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| panic.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("unknown panic")
}

/// Identifies a source file by its path inside the imported folder, size and modified time.
/// The path is relative so the same card matches wherever it is mounted.
struct SeenKey {
    rel_path: String,
    size: i64,
    mtime_ns: i64,
}

impl SeenKey {
    fn of(root: &Path, file: &Path) -> Option<SeenKey> {
        let meta = std::fs::metadata(file).ok()?;
        let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        let rel_path = file.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
        Some(SeenKey { rel_path, size: meta.len() as i64, mtime_ns: i64::try_from(mtime.as_nanos()).ok()? })
    }

    /// The content hash recorded when this file was last imported.
    fn lookup(&self, lib: &Library) -> Result<Option<String>> {
        Ok(lib
            .conn()
            .query_row(
                "SELECT hash FROM import_seen WHERE rel_path = ?1 AND size = ?2 AND mtime_ns = ?3",
                params![self.rel_path, self.size, self.mtime_ns],
                |r| r.get(0),
            )
            .optional()?)
    }

    fn remember(&self, lib: &Library, hash: &str) -> Result<()> {
        lib.conn().execute(
            "INSERT OR REPLACE INTO import_seen (rel_path, size, mtime_ns, hash) VALUES (?1, ?2, ?3, ?4)",
            params![self.rel_path, self.size, self.mtime_ns, hash],
        )?;
        Ok(())
    }
}

/// Whether a photo with this content was deleted from the library before.
fn was_deleted(lib: &Library, hash: &str) -> Result<bool> {
    let found = lib.conn().query_row("SELECT 1 FROM deleted_hashes WHERE hash = ?1", [hash], |_| Ok(())).optional()?;
    Ok(found.is_some())
}

/// A top-level album named after each folder that directly holds one of `files`, keyed by
/// folder.
fn folder_album_ids(lib: &Library, files: &[PathBuf]) -> Result<HashMap<PathBuf, i64>> {
    let mut out = HashMap::new();
    for dir in files.iter().filter_map(|f| f.parent()) {
        if out.contains_key(dir) {
            continue;
        }
        // A folder without a name (the root of a drive) can't name an album.
        if let Some(name) = dir.file_name() {
            out.insert(dir.to_path_buf(), queries::find_or_create_album(lib, &name.to_string_lossy(), None, "album")?);
        }
    }
    Ok(out)
}

/// The photos with these hashes, grouped by the `YYMMDD` day they were taken. `unplaced`
/// leaves out photos already in an album.
fn by_day(lib: &Library, hashes: &[String], unplaced: bool) -> Result<BTreeMap<String, Vec<String>>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let conn = lib.conn();
    let filter = if unplaced { "AND NOT EXISTS (SELECT 1 FROM album_photos ap WHERE ap.photo_id = photos.id)" } else { "" };
    // Chunked to stay under SQLite's limit on query parameters.
    for chunk in hashes.chunks(5000) {
        let marks = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!("SELECT taken_at, hash FROM photos WHERE hash IN ({marks}) {filter}"))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (taken, hash) = row?;
            if let Some(day) = queries::day_prefix(&taken) {
                out.entry(day).or_default().push(hash);
            }
        }
    }
    Ok(out)
}

/// Where the photos of a folder go with day albums: into its album, or for a category into
/// a `YYMMDD_<its name>` album per day inside it.
fn category_targets(lib: &Library, album: i64, hashes: Vec<String>) -> Result<Vec<(i64, Vec<String>)>> {
    let (name, kind): (String, String) =
        lib.conn().query_row("SELECT name, kind FROM albums WHERE id = ?1", [album], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if kind != "category" {
        return Ok(vec![(album, hashes)]);
    }
    by_day(lib, &hashes, false)?
        .into_iter()
        .map(|(day, hashes)| Ok((queries::day_album(lib, &day, Some(album), Some(&name))?, hashes)))
        .collect()
}

/// Day albums for photos going to no album; photos already in one are left alone.
fn loose_day_targets(lib: &Library, hashes: &[String]) -> Result<Vec<(i64, Vec<String>)>> {
    by_day(lib, hashes, true)?.into_iter().map(|(day, hashes)| Ok((queries::day_album(lib, &day, None, None)?, hashes))).collect()
}

/// Adds the photos with these hashes to `album`, in date order. A photo already somewhere in
/// the same top-level album (the user may have moved it) stays where it is.
fn add_by_hash(lib: &Library, album: i64, hashes: &[String]) -> Result<()> {
    let mut photos: Vec<(String, i64)> = Vec::with_capacity(hashes.len());
    {
        let conn = lib.conn();
        // Photos in any album of the tree `album` is in. The id is an integer, so formatting it
        // into the SQL is safe.
        let placed = format!(
            "SELECT ap.photo_id FROM album_photos ap WHERE ap.album_id IN (
                 WITH RECURSIVE up(id, parent) AS (
                     SELECT id, parent_id FROM albums WHERE id = {album}
                   UNION
                     SELECT a.id, a.parent_id FROM albums a JOIN up ON a.id = up.parent),
                 sub(id) AS (
                     SELECT id FROM up WHERE parent IS NULL
                   UNION
                     SELECT a.id FROM albums a JOIN sub ON a.parent_id = sub.id)
                 SELECT id FROM sub)"
        );
        // Chunked to stay under SQLite's limit on query parameters.
        for chunk in hashes.chunks(5000) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut stmt = conn.prepare(&format!("SELECT taken_at, id FROM photos WHERE hash IN ({marks}) AND id NOT IN ({placed})"))?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |r| Ok((r.get(0)?, r.get(1)?)))?;
            for row in rows {
                photos.push(row?);
            }
        }
    }
    photos.sort();
    photos.dedup();
    let ids: Vec<i64> = photos.into_iter().map(|(_, id)| id).collect();
    queries::add_to_album(lib, album, &ids)
}

fn import_one(
    lib: &Library,
    import_id: i64,
    source: Option<&str>,
    include_deleted: bool,
    root: &Path,
    src: &Path,
    claimed: &Mutex<HashSet<String>>,
) -> Result<(Outcome, String)> {
    let seen = SeenKey::of(root, src);
    // Seen in an earlier import and unchanged since: decide without reading the file.
    if let Some(hash) = seen.as_ref().map(|k| k.lookup(lib)).transpose()?.flatten() {
        if lib.conn().query_row("SELECT 1 FROM photos WHERE hash = ?1", [&hash], |_| Ok(())).optional()?.is_some() {
            return Ok((Outcome::Duplicate, hash));
        }
        if !include_deleted && was_deleted(lib, &hash)? {
            return Ok((Outcome::Deleted, hash));
        }
    }

    let hash = hash_file(src)?;
    if let Some(k) = &seen {
        k.remember(lib, &hash)?;
    }

    // Claim the hash so identical files within one import aren't copied twice.
    {
        let mut claimed = claimed.lock().unwrap_or_else(|e| e.into_inner());
        if claimed.contains(&hash) {
            return Ok((Outcome::Duplicate, hash));
        }
        let exists = lib
            .conn()
            .query_row("SELECT 1 FROM photos WHERE hash = ?1", [&hash], |_| Ok(()))
            .optional()?
            .is_some();
        if exists {
            return Ok((Outcome::Duplicate, hash));
        }
        if !include_deleted && was_deleted(lib, &hash)? {
            return Ok((Outcome::Deleted, hash));
        }
        claimed.insert(hash.clone());
    }

    let result = copy_and_index(lib, import_id, source, src, &hash);
    if result.is_err() {
        claimed.lock().unwrap_or_else(|e| e.into_inner()).remove(&hash);
    }
    result.map(|_| (Outcome::Added, hash))
}

fn copy_and_index(lib: &Library, import_id: i64, source: Option<&str>, src: &Path, hash: &str) -> Result<()> {
    let meta = std::fs::metadata(src)?;
    let exif = exif::read(src);
    let (taken_at, date_source) = match exif.taken_at {
        Some(t) => (t, crate::dates::EXIF),
        None => {
            let name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            crate::dates::guess(&name, Some(src), mtime(&meta))
        }
    };

    let dir = lib.originals_dir().join(taken_at.format("%Y/%m/%d").to_string());
    std::fs::create_dir_all(&dir)?;
    let orig_name = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (dest, file) = reserve_dest(&dir, &orig_name)?;
    let thumb = lib.thumb_path(hash);
    let preview = needs_preview(src).then(|| lib.preview_path(hash));

    let cleanup = |e: AppError| {
        let _ = std::fs::remove_file(&dest);
        let _ = std::fs::remove_file(&thumb);
        if let Some(p) = &preview {
            let _ = std::fs::remove_file(p);
        }
        e
    };

    copy_verified(src, file, hash).map_err(cleanup)?;
    let processed = thumbs::process(&dest, &thumb, preview.as_deref()).map_err(cleanup)?;

    let rel_path = dest
        .strip_prefix(&lib.root)
        .expect("dest is inside library")
        .to_string_lossy()
        .replace('\\', "/");

    lib.conn()
        .execute(
            "INSERT INTO photos (hash, phash, rel_path, orig_name, source_path, taken_at, imported_at,
                width, height, file_size, mime, camera_make, camera_model, lens, iso, f_number,
                exposure, focal_len, gps_lat, gps_lon, orientation, import_id, source, date_source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
            params![
                hash,
                processed.phash as i64,
                rel_path,
                orig_name,
                src.to_string_lossy(),
                fmt_dt(taken_at),
                now(),
                processed.width,
                processed.height,
                meta.len() as i64,
                mime_for(src),
                exif.camera_make,
                exif.camera_model,
                exif.lens,
                exif.iso,
                exif.f_number,
                exif.exposure,
                exif.focal_len,
                exif.gps_lat,
                exif.gps_lon,
                exif.orientation,
                import_id,
                source,
                date_source,
            ],
        )
        .map_err(|e| cleanup(e.into()))?;
    // Back in the library, so no longer "previously deleted".
    lib.conn().execute("DELETE FROM deleted_hashes WHERE hash = ?1", [hash])?;
    Ok(())
}

/// Atomically creates `dir/name`, or `dir/stem-N.ext` if taken.
pub fn reserve_dest(dir: &Path, name: &str) -> Result<(PathBuf, File)> {
    let p = Path::new(name);
    let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    for n in 0..10_000 {
        let candidate = if n == 0 { dir.join(name) } else { dir.join(format!("{stem}-{n}{ext}")) };
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(f) => return Ok((candidate, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(AppError::msg(format!("could not find a free file name for {name}")))
}

/// Copies `src` into `dest`, verifying the bytes written match the expected hash.
pub fn copy_verified(src: &Path, mut dest: File, expected_hash: &str) -> Result<()> {
    let mut reader = BufReader::with_capacity(1 << 20, File::open(src)?);
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        dest.write_all(&buf[..n])?;
    }
    dest.sync_all()?;
    if hasher.finalize().to_hex().as_str() != expected_hash {
        return Err(AppError::msg("source file changed while copying"));
    }
    Ok(())
}

/// Copies `src` into `dir` as `file_name` (or a free `-N` variant), verified against `hash`.
/// Returns the destination and whether it was copied: false if an identical file was
/// already there under that name.
pub fn copy_into(src: &Path, hash: &str, dir: &Path, file_name: &str) -> Result<(PathBuf, bool)> {
    let existing = dir.join(file_name);
    if existing.is_file() && hash_file(&existing)? == hash {
        return Ok((existing, false));
    }
    std::fs::create_dir_all(dir)?;
    let (dest, file) = reserve_dest(dir, file_name)?;
    if let Err(e) = copy_verified(src, file, hash) {
        let _ = std::fs::remove_file(&dest);
        return Err(e);
    }
    Ok((dest, true))
}

/// Creates any missing previews, e.g. for photos imported before their format got one.
/// Per-photo failures are logged and skipped. Returns how many previews were made.
pub fn backfill_previews(lib: &Library) -> Result<usize> {
    let rows: Vec<(String, String, bool)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare("SELECT hash, rel_path, trashed_at IS NOT NULL FROM photos")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let made = rows
        .par_iter()
        .filter(|(hash, rel, trashed)| {
            needs_preview(Path::new(rel)) && !lib.preview_path(hash).exists() && lib.photo_path(rel, *trashed).is_file()
        })
        .filter(|(hash, rel, trashed)| {
            let src = lib.photo_path(rel, *trashed);
            match thumbs::decode_upright(&src).and_then(|img| thumbs::write_preview(&img, &lib.preview_path(hash))) {
                Ok(()) => true,
                Err(e) => {
                    eprintln!("preview for {} failed: {e}", src.display());
                    false
                }
            }
        })
        .count();
    Ok(made)
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(File::open(path)?)?;
    Ok(hasher.finalize().to_hex().to_string())
}

fn mtime(meta: &std::fs::Metadata) -> NaiveDateTime {
    meta.modified()
        .map(|t| DateTime::<Local>::from(t).naive_local())
        .unwrap_or_else(|_| Local::now().naive_local())
}

fn mime_for(path: &Path) -> &'static str {
    match extension(path).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("tif" | "tiff") => "image/tiff",
        Some("gif") => "image/gif",
        Some("heic" | "hif") => "image/heic",
        Some("heif") => "image/heif",
        _ => "application/octet-stream",
    }
}

pub fn fmt_dt(dt: NaiveDateTime) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S").to_string()
}

pub fn now() -> String {
    fmt_dt(Local::now().naive_local())
}
