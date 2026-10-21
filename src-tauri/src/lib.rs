pub mod backup;
pub mod commands;
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
            rename_album,
            list_topics,
            create_topic,
            rename_topic,
            delete_topic,
            set_album_topic,
            list_sections,
            create_section,
            rename_section,
            delete_section,
            move_section,
            set_photo_section,
            split_by_day,
            delete_album,
            add_to_album,
            remove_from_album,
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
            add_lens,
            remove_lens,
            rename_lens,
            set_note,
            list_settings_notes,
            list_backup_targets,
            add_backup_target,
            set_backup_target_path,
            rename_backup_target,
            remove_backup_target,
            run_backup,
            verify_backup,
            cancel_backup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running frutyfoto");
}
