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
use serde::Serialize;
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
    /// Albums photos were added to (organized-folder imports).
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
    /// Every folder below the source that holds photos becomes an album named after it
    /// (an existing album of that name is reused); photos already in the library are added
    /// to it too. Photos directly in the source folder go in no album.
    pub folder_albums: bool,
    /// Import photos even if they were deleted from the library before.
    pub include_deleted: bool,
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
        return Err(AppError::msg("cannot import from inside the library (or a parent of it)"));
    }

    let import_id = {
        let conn = lib.conn();
        conn.execute(
            "INSERT INTO imports (source_dir, started_at, folder_albums) VALUES (?1, ?2, ?3)",
            params![src.to_string_lossy(), now(), opts.folder_albums],
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
    let album_of = if opts.folder_albums { folder_album_ids(lib, src, &files)? } else { HashMap::new() };
    // Photos (by hash) that end up in each album, added once the copying is done.
    let album_hashes = Mutex::new(BTreeMap::<i64, Vec<String>>::new());
    let progress = Mutex::new(ImportProgress { total: files.len(), scanning: false, ..scan });
    // Set when a problem makes every remaining file fail too (disk full, source gone).
    let stop = AtomicBool::new(false);
    let claimed = Mutex::new(HashSet::<String>::new());

    emit(&progress.lock().unwrap());

    files.par_iter().for_each(|path| {
        if cancel.load(Ordering::Relaxed) || stop.load(Ordering::Relaxed) {
            return;
        }
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| import_one(lib, import_id, source, opts.include_deleted, path, &claimed)))
            .unwrap_or_else(|panic| Err(AppError::msg(format!("crashed while importing: {}", panic_message(&panic)))));
        if let (Ok((Outcome::Added | Outcome::Duplicate, hash)), Some(album)) = (&result, path.parent().and_then(|d| album_of.get(d))) {
            album_hashes.lock().unwrap().entry(*album).or_default().push(hash.clone());
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
                p.errors.push(ImportFailure { path: src.to_string_lossy().into_owned(), error: format!("import stopped: {reason}") });
            }
        }
        if due() {
            emit(&p);
        }
    });

    let mut p = progress.into_inner().unwrap();
    for (album, hashes) in album_hashes.into_inner().unwrap() {
        if let Err(e) = add_by_hash(lib, album, &hashes) {
            p.failed += 1;
            p.errors.push(ImportFailure { path: src.to_string_lossy().into_owned(), error: format!("adding to album: {e}") });
        } else {
            p.albums += 1;
        }
    }
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
/// forks, `.thumbnails`, another library's `.frutyfoto`), NAS thumbnail dirs and OS trash.
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

/// Whether a photo with this content was deleted from the library before.
fn was_deleted(lib: &Library, hash: &str) -> Result<bool> {
    let found = lib.conn().query_row("SELECT 1 FROM deleted_hashes WHERE hash = ?1", [hash], |_| Ok(())).optional()?;
    Ok(found.is_some())
}

/// The album for each folder below `src` that directly holds one of `files`, keyed by folder.
fn folder_album_ids(lib: &Library, src: &Path, files: &[PathBuf]) -> Result<HashMap<PathBuf, i64>> {
    let mut out = HashMap::new();
    for dir in files.iter().filter_map(|f| f.parent()) {
        if dir == src || out.contains_key(dir) {
            continue;
        }
        let Some(name) = dir.file_name() else { continue };
        out.insert(dir.to_path_buf(), queries::find_or_create_album(lib, &name.to_string_lossy())?);
    }
    Ok(out)
}

/// Adds the photos with these hashes to `album`, in date order.
fn add_by_hash(lib: &Library, album: i64, hashes: &[String]) -> Result<()> {
    let mut photos: Vec<(String, i64)> = Vec::with_capacity(hashes.len());
    {
        let conn = lib.conn();
        // Chunked to stay under SQLite's limit on query parameters.
        for chunk in hashes.chunks(5000) {
            let marks = vec!["?"; chunk.len()].join(",");
            let mut stmt = conn.prepare(&format!("SELECT taken_at, id FROM photos WHERE hash IN ({marks})"))?;
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
    src: &Path,
    claimed: &Mutex<HashSet<String>>,
) -> Result<(Outcome, String)> {
    let hash = hash_file(src)?;

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
    let taken_at = exif.taken_at.unwrap_or_else(|| mtime(&meta));

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
                exposure, focal_len, gps_lat, gps_lon, orientation, import_id, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)",
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
        return Err(AppError::msg("source file changed during import"));
    }
    Ok(())
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
