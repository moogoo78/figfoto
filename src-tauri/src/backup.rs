//! Backups to other disks. A target is a `frutyfoto-backup/` folder holding the photos in
//! the library's own layout (`originals/YYYY/MM/DD/<name>`) plus database snapshots
//! (`db/library-<time>.db`), so it can be browsed and restored without the app.
//!
//! Backups only ever add: photos removed from the library stay on the backup disk. A marker
//! file ties the folder to one library, so a disk mounted at another path can be picked
//! again while another library's backup is refused.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::import::{copy_into, hash_file, now};
use crate::library::Library;

/// Folder created inside the folder the user picks.
pub const BACKUP_DIR: &str = "frutyfoto-backup";
const MARKER: &str = ".frutyfoto-backup.json";
/// Database snapshots kept on each target.
const SNAPSHOTS: usize = 5;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTarget {
    pub id: i64,
    pub name: String,
    /// The `frutyfoto-backup` folder.
    pub path: String,
    /// The disk is connected and the folder belongs to this library.
    pub available: bool,
    pub last_run_at: Option<String>,
    pub last_verified_at: Option<String>,
    /// Live photos with a recorded copy on this target.
    pub backed_up: i64,
    /// Live photos in the library.
    pub total: i64,
    /// Size of the live photos not on this target yet.
    pub bytes_to_copy: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub copied: usize,
    /// Already on the disk (identical file), recorded without copying.
    pub skipped: usize,
    pub errors: Vec<String>,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub checked: usize,
    /// Copies that were missing or changed; they are copied again by the next backup.
    pub problems: Vec<String>,
    pub cancelled: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Marker {
    library_id: String,
    created_at: String,
}

/// This library's id, made on first use. Backups record it to recognise their library.
pub fn library_id(lib: &Library) -> Result<String> {
    let conn = lib.conn();
    let id: Option<String> =
        conn.query_row("SELECT value FROM settings WHERE key = 'library_id'", [], |r| r.get(0)).optional()?;
    if let Some(id) = id {
        return Ok(id);
    }
    let mut hasher = blake3::Hasher::new();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    hasher.update(&nanos.to_le_bytes());
    hasher.update(&std::process::id().to_le_bytes());
    hasher.update(lib.root.to_string_lossy().as_bytes());
    let id = hasher.finalize().to_hex()[..32].to_string();
    conn.execute("INSERT INTO settings (key, value) VALUES ('library_id', ?1)", [&id])?;
    Ok(id)
}

fn read_marker(root: &Path) -> Option<Marker> {
    serde_json::from_slice(&std::fs::read(root.join(MARKER)).ok()?).ok()
}

/// Whether `root` is this library's backup folder on a connected disk.
fn is_ours(lib: &Library, root: &Path) -> Result<bool> {
    let id = library_id(lib)?;
    Ok(read_marker(root).is_some_and(|m| m.library_id == id))
}

/// Resolves a picked folder to its backup folder, creating it (and the marker) if needed.
/// The backup folder itself may be picked too. Refuses another library's backup and
/// folders inside the library.
fn prepare_root(lib: &Library, picked: &Path) -> Result<PathBuf> {
    if !picked.is_dir() {
        return Err(AppError::msg(format!("{} is not available", picked.display())));
    }
    let root = if picked.join(MARKER).is_file() { picked.to_path_buf() } else { picked.join(BACKUP_DIR) };
    let lib_root = lib.root.canonicalize()?;
    if picked.canonicalize()?.starts_with(&lib_root) {
        return Err(AppError::msg("the backup can't go inside the library; pick another disk"));
    }
    let id = library_id(lib)?;
    match read_marker(&root) {
        Some(m) if m.library_id == id => {}
        Some(_) => return Err(AppError::msg(format!("{} is the backup of another library", root.display()))),
        None if root.join(MARKER).exists() => {
            return Err(AppError::msg(format!("{} has an unreadable backup marker", root.display())))
        }
        None => {
            std::fs::create_dir_all(&root)?;
            let marker = Marker { library_id: id, created_at: now() };
            std::fs::write(root.join(MARKER), serde_json::to_vec_pretty(&marker).unwrap())?;
        }
    }
    Ok(root)
}

/// Default target name: the folder the user picked (often the disk's name).
fn default_name(root: &Path) -> String {
    let picked = if root.file_name().is_some_and(|n| n == BACKUP_DIR) { root.parent() } else { Some(root) };
    picked
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Backup".into())
}

fn check_path_free(lib: &Library, root: &Path, except: Option<i64>) -> Result<()> {
    let taken: Option<String> = lib
        .conn()
        .query_row(
            "SELECT name FROM backup_targets WHERE path = ?1 AND id IS NOT ?2",
            params![root.to_string_lossy(), except],
            |r| r.get(0),
        )
        .optional()?;
    match taken {
        Some(name) => Err(AppError::msg(format!("this folder is already the backup “{name}”"))),
        None => Ok(()),
    }
}

/// Adds a backup target in (or at) the picked folder. Returns its id.
pub fn add_target(lib: &Library, picked: &Path) -> Result<i64> {
    let root = prepare_root(lib, picked)?;
    check_path_free(lib, &root, None)?;
    let conn = lib.conn();
    conn.execute(
        "INSERT INTO backup_targets (name, path, created_at) VALUES (?1, ?2, ?3)",
        params![default_name(&root), root.to_string_lossy(), now()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Points a target at another folder, e.g. when its disk is mounted somewhere else.
pub fn set_target_path(lib: &Library, id: i64, picked: &Path) -> Result<()> {
    target_root(lib, id)?;
    let root = prepare_root(lib, picked)?;
    check_path_free(lib, &root, Some(id))?;
    lib.conn().execute("UPDATE backup_targets SET path = ?1 WHERE id = ?2", params![root.to_string_lossy(), id])?;
    Ok(())
}

pub fn rename_target(lib: &Library, id: i64, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::msg("the name can't be empty"));
    }
    lib.conn().execute("UPDATE backup_targets SET name = ?1 WHERE id = ?2", params![name, id])?;
    Ok(())
}

/// Forgets a target. The files on the backup disk are left alone.
pub fn remove_target(lib: &Library, id: i64) -> Result<()> {
    lib.conn().execute("DELETE FROM backup_targets WHERE id = ?1", [id])?;
    Ok(())
}

fn target_root(lib: &Library, id: i64) -> Result<PathBuf> {
    let path: Option<String> =
        lib.conn().query_row("SELECT path FROM backup_targets WHERE id = ?1", [id], |r| r.get(0)).optional()?;
    path.map(PathBuf::from).ok_or_else(|| AppError::msg("no such backup"))
}

/// The target's folder, checked to be connected and this library's.
fn connected_root(lib: &Library, id: i64) -> Result<PathBuf> {
    let root = target_root(lib, id)?;
    if !root.is_dir() {
        return Err(AppError::msg(format!("the backup disk is not connected ({})", root.display())));
    }
    if !is_ours(lib, &root)? {
        return Err(AppError::msg(format!("{} is not this library's backup", root.display())));
    }
    Ok(root)
}

pub fn list_targets(lib: &Library) -> Result<Vec<BackupTarget>> {
    let mut targets: Vec<BackupTarget> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.path, t.last_run_at, t.last_verified_at,
                    (SELECT COUNT(*) FROM photos p WHERE p.trashed_at IS NULL),
                    (SELECT COUNT(*) FROM backup_copies c JOIN photos p ON p.id = c.photo_id
                     WHERE c.target_id = t.id AND p.trashed_at IS NULL),
                    (SELECT COALESCE(SUM(p.file_size), 0) FROM photos p
                     WHERE p.trashed_at IS NULL
                       AND NOT EXISTS (SELECT 1 FROM backup_copies c WHERE c.target_id = t.id AND c.photo_id = p.id))
             FROM backup_targets t ORDER BY t.id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(BackupTarget {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                available: false,
                last_run_at: r.get(3)?,
                last_verified_at: r.get(4)?,
                total: r.get(5)?,
                backed_up: r.get(6)?,
                bytes_to_copy: r.get(7)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    for t in &mut targets {
        t.available = is_ours(lib, Path::new(&t.path))?;
    }
    Ok(targets)
}

/// The path of a backup file relative to the backup folder, with `/` separators.
fn rel_in(root: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}

fn disk_full(e: &AppError) -> bool {
    matches!(e, AppError::Io(io) if io.kind() == std::io::ErrorKind::StorageFull)
}

/// Copies every live photo not yet on the target, then saves a database snapshot. Copies
/// whose file has gone missing or changed size since are copied again. Stops between
/// files when `cancel` is set; what was copied so far is kept.
pub fn run(lib: &Library, id: i64, cancel: &AtomicBool, on_progress: impl Fn(usize, usize)) -> Result<BackupResult> {
    let root = connected_root(lib, id)?;
    let mut result = BackupResult::default();

    // Cheap check of earlier copies: drop records whose file is gone or has another size.
    let recorded: Vec<(i64, String, i64)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT c.photo_id, c.rel_path, p.file_size FROM backup_copies c JOIN photos p ON p.id = c.photo_id
             WHERE c.target_id = ?1",
        )?;
        let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let stale: Vec<i64> = recorded
        .iter()
        .filter(|(_, rel, size)| std::fs::metadata(root.join(rel)).map_or(true, |m| m.len() as i64 != *size))
        .map(|(photo, ..)| *photo)
        .collect();
    for photo in &stale {
        lib.conn().execute("DELETE FROM backup_copies WHERE target_id = ?1 AND photo_id = ?2", params![id, photo])?;
    }

    let pending: Vec<(i64, String, String, String)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT id, rel_path, orig_name, hash FROM photos p
             WHERE trashed_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM backup_copies c WHERE c.target_id = ?1 AND c.photo_id = p.id)
             ORDER BY taken_at, id",
        )?;
        let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    on_progress(0, pending.len());
    for (i, (photo, rel, name, hash)) in pending.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            result.cancelled = true;
            break;
        }
        let src = lib.photo_path(rel, false);
        let dest = root.join(rel);
        let dir = dest.parent().unwrap_or(&root);
        let file_name = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| name.clone());
        match copy_into(&src, hash, dir, &file_name) {
            Ok((dest, copied)) => {
                lib.conn().execute(
                    "INSERT OR REPLACE INTO backup_copies (target_id, photo_id, rel_path, hash, verified_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![id, photo, rel_in(&root, &dest), hash, now()],
                )?;
                if copied {
                    result.copied += 1;
                } else {
                    result.skipped += 1;
                }
            }
            Err(e) if disk_full(&e) => {
                result.errors.push(format!("the backup disk is full ({} photo(s) not copied)", pending.len() - i));
                break;
            }
            Err(e) => result.errors.push(format!("{name}: {e}")),
        }
        on_progress(i + 1, pending.len());
    }

    if !result.cancelled {
        if let Err(e) = snapshot_db(lib, &root) {
            result.errors.push(format!("database snapshot: {e}"));
        }
        lib.conn().execute("UPDATE backup_targets SET last_run_at = ?1 WHERE id = ?2", params![now(), id])?;
    }
    Ok(result)
}

/// Writes a consistent copy of the library database to `db/` and keeps the newest few.
fn snapshot_db(lib: &Library, root: &Path) -> Result<()> {
    let dir = root.join("db");
    std::fs::create_dir_all(&dir)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("library-{stamp}.db"));
    let _ = std::fs::remove_file(&path); // VACUUM INTO refuses an existing file
    if let Err(e) = lib.conn().execute("VACUUM INTO ?1", [path.to_string_lossy()]) {
        let _ = std::fs::remove_file(&path);
        return Err(e.into());
    }
    let mut snapshots: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy())
                .is_some_and(|n| n.starts_with("library-") && n.ends_with(".db"))
        })
        .collect();
    snapshots.sort();
    let extra = snapshots.len().saturating_sub(SNAPSHOTS);
    for old in &snapshots[..extra] {
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}

/// Reads every recorded copy back and checks its hash. Missing or changed copies lose
/// their record, so the next backup copies them again.
pub fn verify(lib: &Library, id: i64, cancel: &AtomicBool, on_progress: impl Fn(usize, usize)) -> Result<VerifyResult> {
    let root = connected_root(lib, id)?;
    let copies: Vec<(i64, String, String, String)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT c.photo_id, c.rel_path, c.hash, p.orig_name FROM backup_copies c JOIN photos p ON p.id = c.photo_id
             WHERE c.target_id = ?1 ORDER BY c.rel_path",
        )?;
        let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut result = VerifyResult::default();
    on_progress(0, copies.len());
    for (i, (photo, rel, hash, name)) in copies.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            result.cancelled = true;
            break;
        }
        let file = root.join(rel);
        let problem = match hash_file(&file) {
            Ok(h) if h == *hash => None,
            Ok(_) => Some("changed"),
            Err(_) if !file.exists() => Some("missing"),
            Err(_) => Some("unreadable"),
        };
        let conn = lib.conn();
        match problem {
            None => {
                conn.execute(
                    "UPDATE backup_copies SET verified_at = ?1 WHERE target_id = ?2 AND photo_id = ?3",
                    params![now(), id, photo],
                )?;
            }
            Some(why) => {
                conn.execute("DELETE FROM backup_copies WHERE target_id = ?1 AND photo_id = ?2", params![id, photo])?;
                result.problems.push(format!("{name} ({rel}): {why}"));
            }
        }
        drop(conn);
        result.checked += 1;
        on_progress(i + 1, copies.len());
    }
    if !result.cancelled {
        lib.conn().execute("UPDATE backup_targets SET last_verified_at = ?1 WHERE id = ?2", params![now(), id])?;
    }
    Ok(result)
}
