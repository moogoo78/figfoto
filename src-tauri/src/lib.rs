pub mod backup;
pub mod cloud;
pub mod commands;
pub mod dates;
pub mod db;
pub mod dupes;
pub mod error;
pub mod exif;
pub mod import;
pub mod library;
pub mod marks;
pub mod queries;
pub mod storage;
pub mod thumbs;

use commands::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .setup(|app| {
            restore_last_library(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_library,
            current_library,
            start_import,
            cancel_import,
            scan_folders,
            list_imports,
            import_errors,
            list_photos,
            list_photo_ids,
            photo_date_span,
            photo_date_buckets,
            get_photo,
            counts,
            set_rating,
            set_favorite,
            toggle_favorite,
            list_tags,
            add_tags,
            remove_tag,
            list_cameras,
            list_albums,
            create_album,
            set_album_kind,
            rename_album,
            delete_album,
            move_album,
            set_child_order,
            set_album_display,
            set_shortcut,
            split_by_day,
            add_to_album,
            remove_from_album,
            move_photos,
            set_album_cover,
            find_duplicates,
            trash_photos,
            restore_photos,
            empty_trash,
            set_mark,
            toggle_mark,
            list_marks,
            set_mark_action,
            run_mark,
            list_sources,
            set_source,
            list_lenses,
            set_lens,
            set_taken_at,
            add_lens,
            remove_lens,
            rename_lens,
            set_note,
            list_settings_notes,
            list_backup_targets,
            add_backup_target,
            add_cloud_backup_target,
            set_backup_target_path,
            set_cloud_backup_target_path,
            rclone_status,
            set_rclone_path,
            rename_backup_target,
            remove_backup_target,
            run_backup,
            verify_backup,
            cancel_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running FigFoto");
}
