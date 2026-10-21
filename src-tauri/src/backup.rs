//! Backups to other disks and to cloud storage. A target is a `frutyfoto-backup/` folder
//! holding the photos in the library's own layout (`originals/YYYY/MM/DD/<name>`) plus
//! database snapshots (`db/library-<time>.db`), so it can be browsed and restored without
//! the app. Cloud targets are the same folder on an rclone remote (see [`crate::cloud`]).
//!
//! Backups only ever add: photos removed from the library stay on the backup disk. A marker
//! file ties the folder to one library, so a disk mounted at another path can be picked
//! again while another library's backup is refused.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::cloud::{self, Rclone};
use crate::error::{AppError, Result};
use crate::import::{copy_into, hash_file, now};
use crate::library::Library;

/// Folder created inside the folder the user picks.
pub const BACKUP_DIR: &str = "frutyfoto-backup";
const MARKER: &str = ".frutyfoto-backup.json";
/// Database snapshots kept on each target.
const SNAPSHOTS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Disk,
    /// An rclone remote; `path` is `remote:folder/frutyfoto-backup`.
    Cloud,
}

impl TargetKind {
    fn parse(s: &str) -> Self {
        if s == "cloud" { TargetKind::Cloud } else { TargetKind::Disk }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupTarget {
    pub id: i64,
    pub name: String,
    pub kind: TargetKind,
    /// The `frutyfoto-backup` folder.
    pub path: String,
    /// The disk is connected and the folder belongs to this library. For cloud targets:
    /// rclone and the remote are set up (the folder is checked when a backup starts).
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

/// The marker in a cloud folder: `None` when there is none, an error when it's unreadable.
fn read_cloud_marker(rc: &Rclone, root: &str) -> Result<Option<Marker>> {
    let Some(bytes) = rc.read(&cloud::join(root, MARKER))? else { return Ok(None) };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| AppError::msg(format!("{root} has an unreadable backup marker")))
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

/// Resolves `remote:folder` to its backup folder like [`prepare_root`], creating the marker
/// there if needed.
fn prepare_cloud_root(lib: &Library, rc: &Rclone, picked: &str) -> Result<String> {
    let picked = picked.trim().trim_end_matches('/');
    let remote = cloud::remote_name(picked).ok_or_else(|| AppError::msg("pick an rclone remote"))?;
    if !rc.remotes()?.iter().any(|r| r.name == remote) {
        return Err(AppError::msg(format!("rclone has no remote “{remote}”; set it up with rclone config")));
    }
    let root = match read_cloud_marker(rc, picked)? {
        Some(_) => picked.to_string(),
        None => cloud::join(picked, BACKUP_DIR),
    };
    let id = library_id(lib)?;
    match read_cloud_marker(rc, &root)? {
        Some(m) if m.library_id == id => {}
        Some(_) => return Err(AppError::msg(format!("{root} is the backup of another library"))),
        None => {
            let marker = Marker { library_id: id, created_at: now() };
            rc.write(&cloud::join(&root, MARKER), &String::from_utf8(serde_json::to_vec_pretty(&marker).unwrap()).unwrap())?;
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

/// Adds a backup target in (or at) a folder on an rclone remote, `remote:folder`. Returns
/// its id.
pub fn add_cloud_target(lib: &Library, picked: &str) -> Result<i64> {
    let root = prepare_cloud_root(lib, &Rclone::find(lib)?, picked)?;
    check_path_free(lib, Path::new(&root), None)?;
    let name = root.strip_suffix(BACKUP_DIR).unwrap_or(&root).trim_end_matches('/').to_string();
    let conn = lib.conn();
    conn.execute(
        "INSERT INTO backup_targets (name, path, kind, created_at) VALUES (?1, ?2, 'cloud', ?3)",
        params![name, root, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Points a cloud target at another `remote:folder`, e.g. after renaming the remote.
pub fn set_cloud_target_path(lib: &Library, id: i64, picked: &str) -> Result<()> {
    if target(lib, id)?.0 != TargetKind::Cloud {
        return Err(AppError::msg("this backup is on a disk"));
    }
    let root = prepare_cloud_root(lib, &Rclone::find(lib)?, picked)?;
    check_path_free(lib, Path::new(&root), Some(id))?;
    lib.conn().execute("UPDATE backup_targets SET path = ?1 WHERE id = ?2", params![root, id])?;
    Ok(())
}

/// Points a target at another folder, e.g. when its disk is mounted somewhere else.
pub fn set_target_path(lib: &Library, id: i64, picked: &Path) -> Result<()> {
    if target(lib, id)?.0 != TargetKind::Disk {
        return Err(AppError::msg("this backup is in the cloud"));
    }
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

fn target(lib: &Library, id: i64) -> Result<(TargetKind, String)> {
    let row: Option<(String, String)> = lib
        .conn()
        .query_row("SELECT kind, path FROM backup_targets WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
        .optional()?;
    row.map(|(kind, path)| (TargetKind::parse(&kind), path)).ok_or_else(|| AppError::msg("no such backup"))
}

/// The target's folder, checked to be connected and this library's.
fn connected_root(lib: &Library, id: i64) -> Result<PathBuf> {
    let root = PathBuf::from(target(lib, id)?.1);
    if !root.is_dir() {
        return Err(AppError::msg(format!("the backup disk is not connected ({})", root.display())));
    }
    if !is_ours(lib, &root)? {
        return Err(AppError::msg(format!("{} is not this library's backup", root.display())));
    }
    Ok(root)
}

/// The cloud target's folder, checked to exist and be this library's.
fn connected_cloud_root(lib: &Library, rc: &Rclone, root: &str) -> Result<()> {
    match read_cloud_marker(rc, root)? {
        Some(m) if m.library_id == library_id(lib)? => Ok(()),
        Some(_) => Err(AppError::msg(format!("{root} is not this library's backup"))),
        None => Err(AppError::msg(format!("the backup folder {root} is gone"))),
    }
}

pub fn list_targets(lib: &Library) -> Result<Vec<BackupTarget>> {
    let mut targets: Vec<BackupTarget> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.kind, t.path, t.last_run_at, t.last_verified_at,
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
                kind: TargetKind::parse(&r.get::<_, String>(2)?),
                path: r.get(3)?,
                available: false,
                last_run_at: r.get(4)?,
                last_verified_at: r.get(5)?,
                total: r.get(6)?,
                backed_up: r.get(7)?,
                bytes_to_copy: r.get(8)?,
            })
        })?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    // Reading each cloud folder would go over the network; only check rclone knows the remote.
    let mut remotes: Option<Vec<String>> = None;
    for t in &mut targets {
        t.available = match t.kind {
            TargetKind::Disk => is_ours(lib, Path::new(&t.path))?,
            TargetKind::Cloud => {
                let remotes = remotes.get_or_insert_with(|| {
                    let found = Rclone::find(lib).and_then(|rc| rc.remotes()).unwrap_or_default();
                    found.into_iter().map(|r| r.name).collect()
                });
                cloud::remote_name(&t.path).is_some_and(|name| remotes.iter().any(|r| r == name))
            }
        };
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
    if let (TargetKind::Cloud, root) = target(lib, id)? {
        return run_cloud(lib, id, &root, cancel, on_progress);
    }
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
    if let (TargetKind::Cloud, root) = target(lib, id)? {
        return verify_cloud(lib, id, &root, cancel, on_progress);
    }
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

// ---- cloud targets ----

/// [`run`] for a cloud target: one rclone copy of all new photos, then a listing to record
/// what arrived. A file of another photo already at a path is kept under a suffixed name.
fn run_cloud(
    lib: &Library,
    id: i64,
    root: &str,
    cancel: &AtomicBool,
    on_progress: impl Fn(usize, usize),
) -> Result<BackupResult> {
    let rc = Rclone::find(lib)?;
    connected_cloud_root(lib, &rc, root)?;
    let mut result = BackupResult::default();

    // Cheap check of earlier copies, as for disks: one listing of the whole folder.
    let on_cloud = rc.sizes(root, None)?;
    {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT c.photo_id, c.rel_path, p.file_size FROM backup_copies c JOIN photos p ON p.id = c.photo_id
             WHERE c.target_id = ?1",
        )?;
        let rows = stmt.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        let stale: Vec<i64> = rows
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter(|(_, rel, size)| on_cloud.get(rel) != Some(size))
            .map(|(photo, ..)| photo)
            .collect();
        drop(stmt);
        for photo in stale {
            conn.execute("DELETE FROM backup_copies WHERE target_id = ?1 AND photo_id = ?2", params![id, photo])?;
        }
    }

    let pending: Vec<(i64, String, String, String, i64)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT id, rel_path, orig_name, hash, file_size FROM photos p
             WHERE trashed_at IS NULL
               AND NOT EXISTS (SELECT 1 FROM backup_copies c WHERE c.target_id = ?1 AND c.photo_id = p.id)
             ORDER BY taken_at, id",
        )?;
        let rows = stmt.query_map([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    on_progress(0, pending.len());
    if !pending.is_empty() {
        // Identical files already there are skipped by rclone and recorded below.
        let files: Vec<String> = pending.iter().map(|(_, rel, ..)| rel.clone()).collect();
        let suffix = chrono::Local::now().format("-replaced-%Y%m%d-%H%M%S").to_string();
        let mut copied = std::collections::HashSet::new();
        let outcome = rc.copy(&lib.root, root, &files, &suffix, cancel, |rel| {
            copied.insert(rel.to_string());
            on_progress(copied.len(), pending.len());
        })?;
        result.cancelled = outcome.cancelled;
        result.errors = outcome.errors;

        let arrived = rc.sizes(root, Some(&files))?;
        let conn = lib.conn();
        for (photo, rel, _, hash, size) in &pending {
            if arrived.get(rel) != Some(size) {
                continue;
            }
            conn.execute(
                "INSERT OR REPLACE INTO backup_copies (target_id, photo_id, rel_path, hash, verified_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![id, photo, rel, hash, now()],
            )?;
            if copied.contains(rel) {
                result.copied += 1;
            } else {
                result.skipped += 1;
            }
        }
        on_progress(pending.len(), pending.len());
    }

    if !result.cancelled {
        if let Err(e) = snapshot_db_cloud(lib, &rc, root) {
            result.errors.push(format!("database snapshot: {e}"));
        }
        lib.conn().execute("UPDATE backup_targets SET last_run_at = ?1 WHERE id = ?2", params![now(), id])?;
    }
    Ok(result)
}

/// [`snapshot_db`] for a cloud target: written locally, uploaded, then the oldest pruned.
fn snapshot_db_cloud(lib: &Library, rc: &Rclone, root: &str) -> Result<()> {
    let local = lib.root.join(".frutyfoto").join("backup-snapshot.db");
    let _ = std::fs::remove_file(&local);
    let uploaded = lib
        .conn()
        .execute("VACUUM INTO ?1", [local.to_string_lossy()])
        .map_err(AppError::from)
        .and_then(|_| {
            let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
            rc.upload(&local, &cloud::join(root, &format!("db/library-{stamp}.db")))
        });
    let _ = std::fs::remove_file(&local);
    uploaded?;
    let dir = cloud::join(root, "db");
    let mut snapshots: Vec<String> = rc
        .sizes(&dir, None)?
        .into_keys()
        .filter(|n| !n.contains('/') && n.starts_with("library-") && n.ends_with(".db"))
        .collect();
    snapshots.sort();
    let extra = snapshots.len().saturating_sub(SNAPSHOTS);
    for old in &snapshots[..extra] {
        let _ = rc.delete(&cloud::join(&dir, old));
    }
    Ok(())
}

/// [`verify`] for a cloud target: rclone compares every recorded copy with the library's
/// file by hash (whichever hash the cloud keeps, e.g. SHA-1 or MD5).
fn verify_cloud(
    lib: &Library,
    id: i64,
    root: &str,
    cancel: &AtomicBool,
    on_progress: impl Fn(usize, usize),
) -> Result<VerifyResult> {
    let rc = Rclone::find(lib)?;
    connected_cloud_root(lib, &rc, root)?;
    let copies: HashMap<String, (i64, String)> = {
        let conn = lib.conn();
        let mut stmt = conn.prepare(
            "SELECT c.rel_path, c.photo_id, p.orig_name FROM backup_copies c JOIN photos p ON p.id = c.photo_id
             WHERE c.target_id = ?1",
        )?;
        let rows = stmt.query_map([id], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let mut files: Vec<String> = copies.keys().cloned().collect();
    files.sort();
    let mut result = VerifyResult::default();
    on_progress(0, files.len());
    let mut db_error = None;
    let cancelled = rc.check(&lib.root, root, &files, cancel, |sym, rel| {
        let Some((photo, name)) = copies.get(rel) else { return };
        let conn = lib.conn();
        let done = match sym {
            '=' => conn.execute(
                "UPDATE backup_copies SET verified_at = ?1 WHERE target_id = ?2 AND photo_id = ?3",
                params![now(), id, photo],
            ),
            _ => {
                let why = match sym {
                    '*' => "changed",
                    '+' => "missing",
                    _ => "unreadable",
                };
                result.problems.push(format!("{name} ({rel}): {why}"));
                conn.execute("DELETE FROM backup_copies WHERE target_id = ?1 AND photo_id = ?2", params![id, photo])
            }
        };
        if let Err(e) = done {
            db_error.get_or_insert(e);
        }
        result.checked += 1;
        on_progress(result.checked, files.len());
    })?;
    if let Some(e) = db_error {
        return Err(e.into());
    }
    result.cancelled = cancelled;
    if !result.cancelled {
        lib.conn().execute("UPDATE backup_targets SET last_verified_at = ?1 WHERE id = ?2", params![now(), id])?;
    }
    Ok(result)
}
