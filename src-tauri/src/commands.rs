use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::backup::{self, BackupResult, BackupTarget, VerifyResult};
use crate::cloud::{self, RcloneStatus};
use crate::error::{AppError, Result};
use crate::import::{self, FolderInfo, FolderRole, ImportFailure, ImportProgress};
use crate::queries::ImportRecord;
use crate::library::Library;
use crate::marks::{self, ActionKind, MarkSlot, RunResult};
use crate::queries::{self, Album, Bucket, Counts, Filter, Photo, PhotoDetail, Sort, Tag};

pub const PROGRESS_EVENT: &str = "import://progress";
pub const MARK_PROGRESS_EVENT: &str = "marks://progress";
pub const BACKUP_PROGRESS_EVENT: &str = "backup://progress";

#[derive(Default)]
pub struct AppState {
    library: Mutex<Option<Arc<Library>>>,
    import_cancel: Arc<AtomicBool>,
    importing: Arc<AtomicBool>,
    backup_cancel: Arc<AtomicBool>,
    backing_up: Arc<AtomicBool>,
}

impl AppState {
    fn lib(&self) -> Result<Arc<Library>> {
        self.library
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| AppError::msg("no library is open"))
    }
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    last_library: Option<PathBuf>,
}

fn config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok().map(|d| d.join("config.json"))
}

/// App ids from before the rename to FigFoto; their config is used until this one has its own.
const LEGACY_IDENTIFIERS: [&str; 1] = ["io.github.moogoo78.frutyfoto"];

fn read_config(app: &AppHandle) -> Option<Config> {
    let path = config_path(app)?;
    let dir = path.parent()?;
    let legacy = LEGACY_IDENTIFIERS.map(|id| dir.with_file_name(id).join("config.json"));
    std::iter::once(path.clone())
        .chain(legacy)
        .find_map(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
}

fn save_config(app: &AppHandle, cfg: &Config) {
    if let Some(path) = config_path(app) {
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let _ = std::fs::write(path, serde_json::to_vec_pretty(cfg).unwrap());
    }
}

/// Re-opens the last used library on startup, if it still exists.
pub fn restore_last_library(app: &AppHandle) {
    let Some(cfg) = read_config(app) else {
        return;
    };
    if let Some(root) = cfg.last_library {
        if let Err(e) = activate(app, &root, false) {
            eprintln!("could not reopen library {}: {e}", root.display());
        }
    }
}

fn activate(app: &AppHandle, root: &Path, create: bool) -> Result<LibraryInfo> {
    let lib = Library::open(root, create)?;
    let scope = app.asset_protocol_scope();
    for dir in lib.served_dirs() {
        scope.allow_directory(&dir, true).map_err(|e| AppError::msg(e.to_string()))?;
    }
    // Libraries from when albums were folders: move those photos back to date folders.
    match crate::storage::move_out_of_album_folders(&lib) {
        Ok(0) => {}
        Ok(n) => eprintln!("moved {n} photo(s) from album folders to date folders"),
        Err(e) => eprintln!("moving photos out of album folders failed: {e}"),
    }
    let info = LibraryInfo { root: lib.root.to_string_lossy().into_owned() };
    let lib = Arc::new(lib);
    *app.state::<AppState>().library.lock().unwrap() = Some(lib.clone());
    std::thread::spawn(move || match import::backfill_previews(&lib) {
        Ok(0) => {}
        Ok(n) => eprintln!("created {n} missing preview(s)"),
        Err(e) => eprintln!("preview backfill failed: {e}"),
    });
    save_config(app, &Config { last_library: Some(root.to_path_buf()) });
    Ok(info)
}

#[derive(Serialize)]
pub struct LibraryInfo {
    root: String,
}

#[tauri::command]
pub async fn open_library(app: AppHandle, state: State<'_, AppState>, path: String, create: bool) -> Result<LibraryInfo> {
    if state.importing.load(Ordering::SeqCst) {
        return Err(AppError::msg("photos are being added"));
    }
    if state.backing_up.load(Ordering::SeqCst) {
        return Err(AppError::msg("a backup is running"));
    }
    activate(&app, Path::new(&path), create)
}

#[tauri::command]
pub async fn current_library(state: State<'_, AppState>) -> Result<Option<LibraryInfo>> {
    Ok(state
        .library
        .lock()
        .unwrap()
        .as_ref()
        .map(|l| LibraryInfo { root: l.root.to_string_lossy().into_owned() }))
}

#[tauri::command]
pub async fn start_import(
    app: AppHandle,
    state: State<'_, AppState>,
    src: String,
    source: Option<String>,
    folder_albums: bool,
    plan: Option<HashMap<String, FolderRole>>,
    include_deleted: bool,
) -> Result<()> {
    let lib = state.lib()?;
    if state.importing.swap(true, Ordering::SeqCst) {
        return Err(AppError::msg("photos are already being added"));
    }
    state.import_cancel.store(false, Ordering::SeqCst);
    let (cancel, importing) = (state.import_cancel.clone(), state.importing.clone());

    std::thread::spawn(move || {
        // Clears the flag however this thread ends, even by panic, so later imports aren't blocked.
        struct Running(Arc<AtomicBool>);
        impl Drop for Running {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _running = Running(importing);
        let emit = |p: &ImportProgress| {
            let _ = app.emit(PROGRESS_EVENT, p);
        };
        let opts = import::ImportOptions { source: source.as_deref(), folder_albums, plan: plan.as_ref(), include_deleted };
        let run = || import::run(&lib, Path::new(&src), &opts, &cancel, emit);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run))
            .unwrap_or_else(|_| Err(AppError::msg("adding photos crashed")));
        if let Err(e) = result {
            emit(&ImportProgress {
                finished: true,
                failed: 1,
                errors: vec![ImportFailure { path: src.clone(), error: e.to_string() }],
                ..Default::default()
            });
        }
    });
    Ok(())
}

/// The folders under `src` that hold photos, for planning an organized-folders import.
#[tauri::command]
pub async fn scan_folders(src: String) -> Result<Vec<FolderInfo>> {
    tauri::async_runtime::spawn_blocking(move || import::scan_folders(Path::new(&src)))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn cancel_import(state: State<'_, AppState>) -> Result<()> {
    state.import_cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
pub async fn list_imports(state: State<'_, AppState>) -> Result<Vec<ImportRecord>> {
    queries::list_imports(&*state.lib()?)
}

#[tauri::command]
pub async fn import_errors(state: State<'_, AppState>, import_id: i64) -> Result<Vec<ImportFailure>> {
    queries::import_errors(&*state.lib()?, import_id)
}

#[tauri::command]
pub async fn list_photos(
    state: State<'_, AppState>,
    filter: Filter,
    sort: Sort,
    offset: i64,
    limit: i64,
) -> Result<Vec<Photo>> {
    queries::list_photos(&*state.lib()?, &filter, sort, offset, limit)
}

#[tauri::command]
pub async fn list_photo_ids(
    state: State<'_, AppState>,
    filter: Filter,
    sort: Sort,
    offset: i64,
    limit: i64,
) -> Result<Vec<i64>> {
    queries::list_ids(&*state.lib()?, &filter, sort, offset, limit)
}

#[tauri::command]
pub async fn photo_date_span(state: State<'_, AppState>, ids: Vec<i64>) -> Result<Option<(String, String)>> {
    queries::date_span(&*state.lib()?, &ids)
}

#[tauri::command]
pub async fn photo_date_buckets(state: State<'_, AppState>, filter: Filter, sort: Sort) -> Result<Vec<Bucket>> {
    queries::date_buckets(&*state.lib()?, &filter, sort)
}

#[tauri::command]
pub async fn get_photo(state: State<'_, AppState>, id: i64) -> Result<PhotoDetail> {
    queries::get_photo(&*state.lib()?, id)
}

#[tauri::command]
pub async fn counts(state: State<'_, AppState>) -> Result<Counts> {
    queries::counts(&*state.lib()?)
}

#[tauri::command]
pub async fn set_rating(state: State<'_, AppState>, ids: Vec<i64>, rating: i64) -> Result<()> {
    queries::set_rating(&*state.lib()?, &ids, rating)
}

#[tauri::command]
pub async fn set_favorite(state: State<'_, AppState>, ids: Vec<i64>, favorite: bool) -> Result<()> {
    queries::set_favorite(&*state.lib()?, &ids, favorite)
}

#[tauri::command]
pub async fn toggle_favorite(state: State<'_, AppState>, ids: Vec<i64>) -> Result<bool> {
    queries::toggle_favorite(&*state.lib()?, &ids)
}

#[tauri::command]
pub async fn list_tags(state: State<'_, AppState>) -> Result<Vec<Tag>> {
    queries::list_tags(&*state.lib()?)
}

#[tauri::command]
pub async fn add_tags(state: State<'_, AppState>, ids: Vec<i64>, names: Vec<String>) -> Result<()> {
    queries::add_tags(&*state.lib()?, &ids, &names)
}

#[tauri::command]
pub async fn remove_tag(state: State<'_, AppState>, ids: Vec<i64>, tag_id: i64) -> Result<()> {
    queries::remove_tag(&*state.lib()?, &ids, tag_id)
}

#[tauri::command]
pub async fn list_cameras(state: State<'_, AppState>) -> Result<Vec<String>> {
    queries::list_cameras(&*state.lib()?)
}

#[tauri::command]
pub async fn list_albums(state: State<'_, AppState>) -> Result<Vec<Album>> {
    queries::list_albums(&*state.lib()?)
}

#[tauri::command]
pub async fn create_album(state: State<'_, AppState>, name: String) -> Result<i64> {
    queries::create_album(&*state.lib()?, &name)
}

#[tauri::command]
pub async fn rename_album(state: State<'_, AppState>, id: i64, name: String) -> Result<()> {
    queries::rename_album(&*state.lib()?, id, &name)
}

#[tauri::command]
pub async fn delete_album(state: State<'_, AppState>, id: i64) -> Result<()> {
    queries::delete_album(&*state.lib()?, id)
}

#[tauri::command]
pub async fn list_categories(state: State<'_, AppState>) -> Result<Vec<queries::Category>> {
    queries::list_categories(&*state.lib()?)
}

#[tauri::command]
pub async fn create_category(state: State<'_, AppState>, name: String, parent_id: Option<i64>) -> Result<i64> {
    queries::create_category(&*state.lib()?, &name, parent_id)
}

#[tauri::command]
pub async fn rename_category(state: State<'_, AppState>, id: i64, name: String) -> Result<()> {
    queries::rename_category(&*state.lib()?, id, &name)
}

#[tauri::command]
pub async fn move_category(state: State<'_, AppState>, id: i64, parent_id: Option<i64>) -> Result<()> {
    queries::move_category(&*state.lib()?, id, parent_id)
}

#[tauri::command]
pub async fn delete_category(state: State<'_, AppState>, id: i64) -> Result<()> {
    queries::delete_category(&*state.lib()?, id)
}

#[tauri::command]
pub async fn set_shortcut(state: State<'_, AppState>, kind: String, id: i64, pinned: bool) -> Result<()> {
    queries::set_shortcut(&*state.lib()?, &kind, id, pinned)
}

#[tauri::command]
pub async fn set_album_category(state: State<'_, AppState>, album_ids: Vec<i64>, category_id: Option<i64>) -> Result<()> {
    queries::set_album_category(&*state.lib()?, &album_ids, category_id)
}

#[tauri::command]
pub async fn list_sections(state: State<'_, AppState>, album_id: i64) -> Result<Vec<queries::Section>> {
    queries::list_sections(&*state.lib()?, album_id)
}

#[tauri::command]
pub async fn create_section(state: State<'_, AppState>, album_id: i64, parent_id: Option<i64>, name: String) -> Result<i64> {
    queries::create_section(&*state.lib()?, album_id, parent_id, &name)
}

#[tauri::command]
pub async fn rename_section(state: State<'_, AppState>, id: i64, name: String) -> Result<()> {
    queries::rename_section(&*state.lib()?, id, &name)
}

#[tauri::command]
pub async fn delete_section(state: State<'_, AppState>, id: i64) -> Result<()> {
    queries::delete_section(&*state.lib()?, id)
}

#[tauri::command]
pub async fn set_section_kind(state: State<'_, AppState>, album_id: i64, kind: String) -> Result<()> {
    queries::set_section_kind(&*state.lib()?, album_id, &kind)
}

#[tauri::command]
pub async fn set_photo_section(state: State<'_, AppState>, album_id: i64, ids: Vec<i64>, section_id: Option<i64>) -> Result<()> {
    queries::set_photo_section(&*state.lib()?, album_id, &ids, section_id)
}

#[tauri::command]
pub async fn split_by_day(state: State<'_, AppState>, album_id: i64) -> Result<usize> {
    queries::split_by_day(&*state.lib()?, album_id)
}

#[tauri::command]
pub async fn add_to_album(state: State<'_, AppState>, album_id: i64, ids: Vec<i64>) -> Result<()> {
    queries::add_to_album(&*state.lib()?, album_id, &ids)
}

#[tauri::command]
pub async fn remove_from_album(state: State<'_, AppState>, album_id: i64, ids: Vec<i64>) -> Result<()> {
    queries::remove_from_album(&*state.lib()?, album_id, &ids)
}

#[tauri::command]
pub async fn set_album_cover(state: State<'_, AppState>, album_id: i64, photo_id: i64) -> Result<()> {
    queries::set_album_cover(&*state.lib()?, album_id, photo_id)
}

#[tauri::command]
pub async fn find_duplicates(state: State<'_, AppState>, threshold: u32) -> Result<Vec<Vec<Photo>>> {
    queries::find_duplicates(&*state.lib()?, threshold.min(32))
}

#[tauri::command]
pub async fn trash_photos(state: State<'_, AppState>, ids: Vec<i64>) -> Result<()> {
    queries::trash_photos(&*state.lib()?, &ids)
}

#[tauri::command]
pub async fn restore_photos(state: State<'_, AppState>, ids: Vec<i64>) -> Result<()> {
    queries::restore_photos(&*state.lib()?, &ids)
}

#[tauri::command]
pub async fn empty_trash(state: State<'_, AppState>) -> Result<usize> {
    queries::empty_trash(&*state.lib()?)
}

#[tauri::command]
pub async fn set_mark(state: State<'_, AppState>, ids: Vec<i64>, mark: Option<String>) -> Result<()> {
    marks::set_mark(&*state.lib()?, &ids, mark.as_deref())
}

#[tauri::command]
pub async fn toggle_mark(state: State<'_, AppState>, ids: Vec<i64>, mark: String) -> Result<()> {
    marks::toggle_mark(&*state.lib()?, &ids, &mark)
}

#[tauri::command]
pub async fn list_marks(state: State<'_, AppState>) -> Result<Vec<MarkSlot>> {
    marks::list_slots(&*state.lib()?)
}

#[tauri::command]
pub async fn set_mark_action(
    state: State<'_, AppState>,
    slot: i64,
    label: String,
    kind: Option<ActionKind>,
    target: String,
) -> Result<()> {
    marks::set_action(&*state.lib()?, slot, &label, kind, &target)
}

#[derive(Clone, Serialize)]
struct MarkProgress {
    mark: String,
    done: usize,
    total: usize,
}

#[tauri::command]
pub async fn run_mark(app: AppHandle, state: State<'_, AppState>, mark: String) -> Result<RunResult> {
    let lib = state.lib()?;
    tauri::async_runtime::spawn_blocking(move || {
        marks::run(&lib, &mark, |done, total| {
            let _ = app.emit(MARK_PROGRESS_EVENT, MarkProgress { mark: mark.clone(), done, total });
        })
    })
    .await
    .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn list_sources(state: State<'_, AppState>) -> Result<Vec<String>> {
    marks::list_sources(&*state.lib()?)
}

#[tauri::command]
pub async fn list_lenses(state: State<'_, AppState>) -> Result<Vec<queries::SavedLens>> {
    queries::list_lenses(&*state.lib()?)
}

#[tauri::command]
pub async fn add_lens(state: State<'_, AppState>, name: String) -> Result<()> {
    queries::add_lens(&*state.lib()?, &name)
}

#[tauri::command]
pub async fn remove_lens(state: State<'_, AppState>, name: String) -> Result<()> {
    queries::remove_lens(&*state.lib()?, &name)
}

#[tauri::command]
pub async fn rename_lens(state: State<'_, AppState>, old: String, new: String) -> Result<()> {
    queries::rename_lens(&*state.lib()?, &old, &new)
}

#[tauri::command]
pub async fn set_lens(state: State<'_, AppState>, ids: Vec<i64>, lens: String) -> Result<usize> {
    queries::set_lens(&*state.lib()?, &ids, &lens)
}

#[tauri::command]
pub async fn list_settings_notes(state: State<'_, AppState>) -> Result<Vec<String>> {
    queries::list_settings_notes(&*state.lib()?)
}

#[tauri::command]
pub async fn set_note(state: State<'_, AppState>, ids: Vec<i64>, kind: String, text: String) -> Result<()> {
    queries::set_note(&*state.lib()?, &ids, &kind, &text)
}

#[tauri::command]
pub async fn set_source(state: State<'_, AppState>, ids: Vec<i64>, source: String) -> Result<()> {
    marks::set_source(&*state.lib()?, &ids, &source)
}

// ---- backup ----

#[tauri::command]
pub async fn list_backup_targets(state: State<'_, AppState>) -> Result<Vec<BackupTarget>> {
    backup::list_targets(&*state.lib()?)
}

#[tauri::command]
pub async fn add_backup_target(state: State<'_, AppState>, path: String) -> Result<i64> {
    backup::add_target(&*state.lib()?, Path::new(&path))
}

#[tauri::command]
pub async fn set_backup_target_path(state: State<'_, AppState>, id: i64, path: String) -> Result<()> {
    backup::set_target_path(&*state.lib()?, id, Path::new(&path))
}

#[tauri::command]
pub async fn add_cloud_backup_target(state: State<'_, AppState>, path: String) -> Result<i64> {
    let lib = state.lib()?;
    tauri::async_runtime::spawn_blocking(move || backup::add_cloud_target(&lib, &path))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn set_cloud_backup_target_path(state: State<'_, AppState>, id: i64, path: String) -> Result<()> {
    let lib = state.lib()?;
    tauri::async_runtime::spawn_blocking(move || backup::set_cloud_target_path(&lib, id, &path))
        .await
        .map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn rclone_status(state: State<'_, AppState>) -> Result<RcloneStatus> {
    let lib = state.lib()?;
    tauri::async_runtime::spawn_blocking(move || cloud::status(&lib)).await.map_err(|e| AppError::msg(e.to_string()))
}

/// `None` goes back to finding rclone on PATH.
#[tauri::command]
pub async fn set_rclone_path(state: State<'_, AppState>, path: Option<String>) -> Result<()> {
    cloud::set_path(&*state.lib()?, path.as_deref())
}

#[tauri::command]
pub async fn rename_backup_target(state: State<'_, AppState>, id: i64, name: String) -> Result<()> {
    backup::rename_target(&*state.lib()?, id, &name)
}

#[tauri::command]
pub async fn remove_backup_target(state: State<'_, AppState>, id: i64) -> Result<()> {
    backup::remove_target(&*state.lib()?, id)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupProgress {
    target_id: i64,
    done: usize,
    total: usize,
}

/// Runs one backup task at a time on a blocking thread, sending progress events.
async fn backup_task<T: Send + 'static>(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    task: fn(&Library, i64, &AtomicBool, &dyn Fn(usize, usize)) -> Result<T>,
) -> Result<T> {
    let lib = state.lib()?;
    if state.backing_up.swap(true, Ordering::SeqCst) {
        return Err(AppError::msg("a backup is already running"));
    }
    state.backup_cancel.store(false, Ordering::SeqCst);
    let (cancel, running) = (state.backup_cancel.clone(), state.backing_up.clone());
    let result = tauri::async_runtime::spawn_blocking(move || {
        task(&lib, id, &cancel, &|done, total| {
            let _ = app.emit(BACKUP_PROGRESS_EVENT, BackupProgress { target_id: id, done, total });
        })
    })
    .await;
    running.store(false, Ordering::SeqCst);
    result.map_err(|e| AppError::msg(e.to_string()))?
}

#[tauri::command]
pub async fn run_backup(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<BackupResult> {
    backup_task(app, state, id, |lib, id, cancel, progress| backup::run(lib, id, cancel, progress)).await
}

#[tauri::command]
pub async fn verify_backup(app: AppHandle, state: State<'_, AppState>, id: i64) -> Result<VerifyResult> {
    backup_task(app, state, id, |lib, id, cancel, progress| backup::verify(lib, id, cancel, progress)).await
}

#[tauri::command]
pub async fn cancel_backup(state: State<'_, AppState>) -> Result<()> {
    state.backup_cancel.store(true, Ordering::SeqCst);
    Ok(())
}
