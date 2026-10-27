// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

/**
 * Main entry point for the Tauri application.
 * project: Lap
 * author:  julyx10
 * date:    2024-08-08
 */
use tauri::Manager;
use tauri_plugin_aptabase::EventTracker;

mod ai;
mod t_ai;
mod t_ai_png;
mod t_apple_sidecar;
mod t_cluster;
mod t_cmds;
mod t_common;
mod t_config;
mod t_dedup;
mod t_dlna;
mod t_drag;
mod t_embedded_jpeg;
mod t_face;
mod t_heif;
mod t_home;
mod t_http;
mod t_image;
mod t_jpeg;
mod t_jxl;
mod t_lens;
mod t_libraw;
mod t_raw_display;
mod t_network;
mod t_webview_proxy;
mod t_map;
mod t_resources;
mod t_menu;
mod t_migration;
mod t_montage;
mod t_tag_groups;
mod t_motion_photo;
mod t_pasteboard;
mod t_protocol;
mod t_similar;
mod t_smart_album;
mod t_media_recovery;
mod t_media_restore;
mod t_media_jobs;
mod t_media_lifecycle;
mod t_media_traits;
mod t_private_media;
mod t_private_lock;
mod t_sqlite;
mod t_search;
mod t_media_title;
mod t_media_views;
mod t_scan;
mod t_thumbnail_rebuild;
mod t_storage;
mod t_startup;
mod t_utils;
mod t_video;
mod t_window_exit;

/// The main function is the entry point for the Tauri application.
#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("Unhandled panic: {}", panic_info);
    }));

    let startup = t_startup::StartupState::default();
    let builder = tauri::Builder::default();
    let builder = t_protocol::register_protocols(builder);
    let builder = t_private_lock::register_protocol(builder);

    let aptabase_enabled = option_env!("APTABASE_KEY")
        .filter(|k| !k.is_empty())
        .is_some();

    let builder = match option_env!("APTABASE_KEY").filter(|k| !k.is_empty()) {
        Some(key) => builder.plugin(tauri_plugin_aptabase::Builder::new(key).build()),
        None => builder,
    };

    let run_result = builder
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Let the frontend show the window after its first themed render.
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        & !tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(startup)
        .manage(t_video::VideoManager::default())
        .manage(t_ai::AiState(std::sync::Arc::new(std::sync::Mutex::new(t_ai::AiEngine::new()))))
        .manage(t_face::FaceState(std::sync::Arc::new(
            std::sync::Mutex::new(t_face::FaceEngine::new()),
        )))
        .manage(t_cmds::IndexCancellation(std::sync::Arc::new(
            std::sync::Mutex::new(std::collections::HashMap::new()),
        )))
        .manage(t_cmds::ImportCancellation(std::sync::Arc::new(std::sync::Mutex::new(
            t_cmds::ImportState::default(),
        ))))
        .manage(t_face::FaceIndexCancellation(std::sync::Arc::new(
            std::sync::Mutex::new(false),
        )))
        .manage(t_face::FaceIndexingStatus(std::sync::Arc::new(
            std::sync::Mutex::new(false),
        )))
        .manage(t_face::FaceIndexProgressState(std::sync::Arc::new(
            std::sync::Mutex::new(t_face::FaceIndexProgress::default()),
        )))
        .manage(t_dedup::DedupState::default())
        .manage(t_similar::SimilarState::default())
        .setup(|_app| {
            t_startup::mark(&_app.handle(), "setup-started");
            t_config::set_app_identifier(&_app.config().identifier);
            // Apply restored configuration before anything caches it or starts
            // background work. A failed apply rolls back and keeps the marker.
            if let Err(error) = t_storage::apply_pending_settings() {
                eprintln!(
                    "Application settings restore failed; previous settings retained: {error}"
                );
            }
            t_private_lock::start_auto_lock(_app.handle().clone());
            if let Ok(config) = t_config::load_app_config() {
                t_webview_proxy::initialize(config.network_proxy_url.clone());
                t_network::initialize(config.network_proxy_url);
            }
            if _app.get_webview_window("main").is_none() {
                let config=_app.config().app.windows.iter().find(|window|window.label=="main").ok_or("Main window configuration missing")?;
                let mut builder=tauri::WebviewWindowBuilder::from_config(_app,config)?;
                #[cfg(not(target_os="macos"))]
                if let Some(proxy)=t_webview_proxy::startup_url(){builder=builder.proxy_url(proxy);}
                builder.build()?;
            }
            t_video::init_ffmpeg_path(&_app.handle());
            t_menu::install_app_menu(&_app.handle())?;

            #[cfg(not(target_os = "macos"))]
            if let Some(window) = _app.get_webview_window("main") {
                if let Err(e) = window.set_decorations(false) {
                    eprintln!("Failed to disable main window decorations: {}", e);
                }
            }

            // tauri.windows.conf.json sets zoomHotkeysEnabled=true so wry sets
            // both IsZoomControlEnabled and IsPinchZoomEnabled to true at WebView
            // creation. That combination is what allows Chromium to synthesize
            // wheel+ctrlKey events for touchpad pinch. Touchscreen pinch is still
            // handled by our pointer-event logic (with `touch-action: none`).

            t_startup::mark(&_app.handle(), "database-started");
            // Create the database on startup
            if let Err(e) = t_sqlite::create_db() {
                eprintln!("Failed to initialize database: {}", e);
                t_startup::mark(&_app.handle(), "database-failed");
            } else {
                t_startup::mark(&_app.handle(), "database-ready");
                if let Err(e) = ai::profiles::ensure_selected() {
                    eprintln!("Failed to initialize AI index profiles: {e}");
                }
            }

            // Initialize video HTTP server for Linux
            #[cfg(target_os = "linux")]
            t_http::init_video_http_server();

            // Cleanup video cache
            t_video::init_video_cache(&_app.handle());

            if !t_sqlite::is_database_corrupted() {
                if let Err(e) = t_utils::restore_album_scopes(&_app.handle()) {
                    eprintln!("Failed to restore asset scopes: {}", e);
                }
            }
            t_startup::mark(&_app.handle(), "setup-ready");

            // Open devtools in development mode
            // #[cfg(debug_assertions)] // only include this block in debug builds
            // {
            //     let window = app.get_webview_window("main").unwrap();
            //     window.open_devtools();
            // }

            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(
                event,
                tauri::WindowEvent::Destroyed | tauri::WindowEvent::CloseRequested { .. }
            ) {
                t_dlna::release_owner(window.label());
            }
            if window.label() != "main" {
                return;
            }

            if let tauri::WindowEvent::Focused(false) = event {
                let app=window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || t_private_lock::window_lock(&app,true));
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Prevent the default close so we can decide what to do per platform.
                api.prevent_close();
                let app=window.app_handle().clone();
                tauri::async_runtime::spawn_blocking(move || t_private_lock::window_lock(&app,false));

                // macOS convention: closing the window hides it; the app stays
                // alive and can be reopened via the Dock icon (handled by
                // RunEvent::Reopen). Real quit keeps using the native Quit menu.
                #[cfg(target_os = "macos")]
                {
                    let _ = window.hide();
                }

                #[cfg(not(target_os = "macos"))]
                {
                    if let Err(error) = t_window_exit::request(window.app_handle()) {
                        eprintln!("Application exit cancelled: {error}");
                    }
                }
            }
        })
        .on_menu_event(|app, event| {
            t_menu::handle_menu_event(app, event);
        })
        .invoke_handler(tauri::generate_handler![
            ai::services::commands::media_ai_configuration,
            ai::services::commands::media_ai_save_configuration,
            ai::services::commands::media_ai_list_models,
            ai::services::commands::media_ai_probe_draft,
            ai::services::commands::media_ai_test_model,
            ai::services::commands::media_ai_save_embedding,
            ai::services::workflow::media_ai_preflight,
            ai::services::workflow::media_ai_jobs,
            ai::services::workflow::media_ai_start_job,
            ai::services::workflow::media_ai_resume_job,
            ai::services::workflow::media_ai_pause_job,
            ai::services::workflow::media_ai_inspect,
            ai::services::workflow::media_ai_adopt,
            ai::services::workflow::media_ai_dismiss,
            ai::services::workflow::media_ai_search,
            ai::services::workflow::media_ai_ask,
            ai::services::chat::media_chat_sessions,
            ai::services::chat::media_chat_create,
            ai::services::chat::media_chat_inspect,
            ai::services::chat::media_chat_rename,
            ai::services::chat::media_chat_delete,
            ai::services::chat::media_chat_media,
            ai::services::chat::media_chat_send,
            ai::services::requests::media_ai_cancel_request,
            t_window_exit::get_pending_app_exit,
            t_window_exit::cancel_app_exit,
            t_window_exit::confirm_app_exit,
            t_window_exit::request_app_restart,
            t_startup::record_startup_stage,
            t_startup::finish_startup,
            t_startup::get_ai_initialization_status,
            t_drag::start_file_drag,
            t_drag::cancel_file_drag,
            // library
            t_cmds::get_app_config,
            t_cmds::get_system_region_locale,
            t_cmds::get_supported_format_extensions,
            t_cmds::get_ffmpeg_backed_image_extensions,
            t_cmds::check_gstreamer_available,
            t_cmds::set_last_selected_item_index,
            t_cmds::get_db_storage_dir,
            t_cmds::is_using_custom_db_storage,
            t_cmds::change_db_storage_dir,
            t_cmds::reset_db_storage_dir,
            t_cmds::add_library,
            t_cmds::edit_library,
            t_cmds::remove_library,
            t_cmds::hide_library,
            t_cmds::reorder_libraries,
            t_cmds::switch_library,
            t_cmds::is_database_corrupted,
            t_cmds::get_library_info,
            t_cmds::save_library_state,
            t_cmds::get_library_state,
            t_cmds::get_current_library_state,
            // album
            t_cmds::get_all_albums,
            t_cmds::get_all_album_folders,
            t_cmds::generate_directory_thumbnails,
            t_cmds::get_album,
            t_cmds::check_album_accessibility,
            t_cmds::recount_album,
            t_cmds::get_album_visible_counts,
            t_cmds::add_album,
            t_cmds::edit_album,
            t_cmds::remove_album,
            t_cmds::reorder_albums,
            t_cmds::set_album_cover,
            t_cmds::index_album,
            t_scan::get_scan_overview,
            t_scan::open_scan_directory,
            t_scan::get_scan_history,
            t_scan::get_scan_details,
            t_scan::retry_scan_errors,
            t_scan::get_scan_folder_states,
            t_cmds::cancel_indexing,
            t_cmds::get_index_recovery_info,
            t_cmds::clear_index_recovery_info,
            // folder
            t_cmds::select_folder,
            t_cmds::fetch_folder,
            t_cmds::count_folder,
            t_cmds::list_album_subfolders,
            t_cmds::create_folder,
            t_cmds::rename_folder,
            t_cmds::move_folder,
            t_cmds::move_folder_outside_library,
            t_cmds::copy_folder,
            t_cmds::delete_folder,
            t_cmds::delete_folder_permanently,
            t_cmds::reveal_path,
            t_cmds::open_external_url,
            t_cmds::set_desktop_wallpaper,
            t_cmds::get_external_app_display_name,
            t_cmds::open_file_with_app,
            t_cmds::open_files_with_app,
            // file query
            t_cmds::get_query_count_and_sum,
            t_cmds::get_query_time_line,
            t_cmds::get_query_files,
            t_cmds::get_grouped_query_rows,
            t_cmds::get_group_file_ids,
            t_cmds::get_grouped_file_position,
            t_cmds::get_query_file_ids,
            t_cmds::get_library_visible_counts,
            t_home::get_home_overview,
            t_home::search_home_entities,
            t_home::get_home_preview_files,
            t_cmds::get_query_file_position,
            t_media_lifecycle::delete_hidden_media,
            t_media_lifecycle::get_private_media_preview,
            t_private_media::read_private_media,
            t_dlna::dlna_discover,
            t_dlna::dlna_connect,
            t_dlna::dlna_cast,
            t_dlna::dlna_control,
            t_dlna::dlna_disconnect,
            t_dlna::dlna_share,
            t_dlna::dlna_activate_share,
            t_dlna::dlna_transport_state,
            t_private_lock::private_lock_status,
            t_private_lock::unlock_private_view,
            t_private_lock::configure_private_lock,
            t_private_lock::lock_private_view,
            t_private_lock::touch_private_view,
            t_private_lock::prepare_private_video,
            t_media_lifecycle::get_media_overview,
            t_media_lifecycle::get_hidden_media,
            t_media_lifecycle::get_deleted_media,
            t_media_lifecycle::set_media_hidden,
            t_media_lifecycle::restore_deleted_media,
            t_media_lifecycle::preview_deleted_media_restore,
            ai::objects_store::get_object_state,
            ai::objects_store::get_photo_objects,
            ai::objects_store::save_object_annotations,
            ai::objects_store::reset_object_annotations,
            ai::objects_jobs::start_object_index,
            ai::objects_jobs::cancel_object_index,
            ai::ocr::get_ocr_status,
            ai::ocr_auto::record_ocr_activity,
            ai::ocr_auto::save_ocr_automation,
            ai::objects_automation::record_object_activity,
            ai::objects_automation::save_object_automation,
            ai::ocr_annotations::save_ocr_text,
            ai::cuda_assets::get_cuda_runtime_status,
            ai::cuda_assets::download_cuda_runtime,
            ai::cuda_assets::cancel_cuda_runtime_download,
            ai::cuda_assets::test_cuda_runtime,
            ai::cuda_assets::open_cuda_runtime_directory,
            ai::ocr_annotations::prepare_ocr_manual,
            ai::ocr_annotations::reset_ocr_text,
            ai::ocr_extract::extract_ocr_region,
            ai::ocr_extract::cancel_ocr_extraction,
            ai::ocr::get_ocr_summaries,
            ai::ocr::start_ocr_index,
            ai::ocr::cancel_ocr_index,
            ai::ocr::get_photo_ocr,
            t_media_jobs::start_media_job,
            t_media_jobs::get_media_jobs,
            t_media_jobs::cancel_media_job,
            t_media_jobs::retry_media_job,
            t_media_jobs::get_media_job_failures,
            t_media_lifecycle::purge_deleted_media,
            // smart album
            t_cmds::get_smart_album_status,
            t_cmds::start_smart_album_index,
            t_cmds::stop_smart_album_index,
            t_cmds::get_smart_query_count_and_sum,
            t_cmds::get_smart_query_time_line,
            t_cmds::get_smart_query_files,
            t_cmds::get_smart_grouped_query_rows,
            t_cmds::get_smart_group_file_ids,
            t_cmds::get_smart_query_file_ids,
            t_cmds::get_smart_query_file_position,
            // collection
            t_cmds::list_collections,
            t_cmds::get_collection_counts,
            t_cmds::create_collection,
            t_cmds::rename_collection,
            t_cmds::delete_collection,
            t_cmds::reorder_collections,
            t_cmds::add_files_to_collection,
            t_cmds::remove_files_from_collection,
            t_cmds::get_collection_selection_counts,
            t_cmds::clear_collection,
            t_cmds::get_collection_file_ids,
            t_cmds::get_file_collections,
            t_cmds::get_collection_count_and_sum,
            t_cmds::get_collection_files,
            t_cmds::get_collection_grouped_query_rows,
            t_cmds::get_collection_group_file_ids,
            t_cmds::get_collection_query_file_ids,
            // folder file query
            t_cmds::get_files_by_ids,
            t_cmds::get_folder_files,
            t_cmds::sync_album_folder_mtimes,
            t_cmds::refresh_album_subfolders,
            t_cmds::is_directory_accessible,
            t_cmds::get_folder_thumb_count,
            // file operations
            t_cmds::edit_image,
            t_cmds::render_montage,
            t_cmds::copy_edited_image,
            t_cmds::copy_images,
            t_cmds::rename_file,
            t_cmds::move_file,
            t_cmds::move_file_outside_library,
            t_cmds::copy_file,
            t_cmds::import_file,
            t_cmds::import_and_organize,
            t_cmds::cancel_import_and_organize,
            t_cmds::import_url,
            t_cmds::import_from_drag,
            t_cmds::get_drag_payload,
            t_cmds::import_file_bytes,
            t_cmds::has_importable_clipboard,
            t_cmds::import_clipboard,
            t_cmds::delete_file,
            t_cmds::delete_file_permanently,
            t_cmds::delete_db_file,
            t_cmds::batch_delete_files,
            // file metadata
            t_cmds::edit_file_comment,
            t_media_title::get_media_title,
            t_media_title::set_media_title,
            t_media_views::record_media_view,
            t_cmds::clean_unused_thumbnail_cache,
            t_cmds::get_file_thumb,
            t_cmds::get_file_thumb_by_id,
            t_cmds::get_file_thumbs,
            t_cmds::regenerate_thumbnails,
            t_cmds::cancel_thumbnail_regeneration,
            t_cmds::get_file_info,
            t_cmds::update_file_info,
            t_cmds::refresh_selected_file_info,
            t_cmds::is_cloud_only_file,
            t_cmds::prepare_motion_photo_video,
            t_cmds::add_file_to_db,
            t_cmds::check_file_exists,
            t_cmds::check_file_accessibility,
            t_cmds::set_file_rotate,
            t_cmds::get_file_has_tags,
            // favorite
            t_cmds::get_favorite_folders,
            t_cmds::get_folder_favorite,
            t_cmds::set_folder_favorite,
            t_cmds::get_folder_search_excluded,
            t_cmds::set_folder_search_excluded,
            t_cmds::set_file_favorite,
            t_cmds::set_file_rating,
            t_cmds::set_file_culling_flag,
            t_cmds::batch_update_file_metadata,
            // tag
            t_cmds::get_tag_group_name,
            t_cmds::get_tag_groups,
            t_cmds::save_tag_group,
            t_cmds::reorder_tag_groups,
            t_cmds::delete_tag_group,
            t_cmds::move_tags_to_group,
            t_cmds::get_all_tags,
            t_cmds::get_tag_counts,
            t_cmds::get_tag_name,
            t_cmds::create_tag,
            t_cmds::rename_tag,
            t_cmds::delete_tag,
            t_cmds::get_tags_for_file,
            t_cmds::add_tag_to_file,
            t_cmds::remove_tag_from_file,
            t_cmds::get_tag_selection_counts,
            t_cmds::apply_tags_to_files,
            // calendar
            t_cmds::get_taken_dates,
            // camera
            t_cmds::get_camera_info,
            t_cmds::get_lens_info,
            // location
            t_cmds::get_location_info,
            t_cmds::get_gps_map_points,
            // settings
            t_cmds::get_package_info,
            t_cmds::get_build_time,
            t_cmds::get_storage_file_info,
            t_cmds::get_network_proxy,
            t_webview_proxy::get_sdk_browser_proxy_state,
            t_cmds::set_network_proxy,
            t_cmds::test_network_proxy,
            t_cmds::get_app_resources_status,
            t_cmds::download_app_resources,
            // ai
            ai::commands::get_ai_configuration,
            ai::commands::save_ai_model,
            ai::commands::activate_ai_model,
            ai::commands::import_ai_model,
            ai::commands::delete_ai_model,
            ai::commands::download_ai_model,
            ai::commands::open_ai_model_directory,
            ai::commands::cancel_ai_model_download,
            ai::commands::test_ai_model,
            ai::commands::refresh_ai_runtime,
            t_cmds::check_ai_status,
            t_cmds::generate_embedding,
            t_cmds::search_similar_images,
            // person (face recognition)
            t_cmds::index_faces,
            t_cmds::cancel_face_index,
            t_cmds::reset_faces,
            t_cmds::is_face_indexing,
            t_cmds::get_face_stats,
            t_cmds::get_persons,
            t_cmds::get_persons_page,
            t_cmds::rename_person,
            t_cmds::edit_face_name,
            t_cmds::confirm_face_detection,
            t_cmds::get_face_review_page,
            t_cmds::review_faces,
            t_cmds::get_face_review_thumbnail,
            t_cmds::get_face_people,
            t_cmds::get_person_merge_preview,
            t_cmds::merge_persons,
            t_cmds::set_person_hidden,
            t_cmds::set_person_cover,
            t_map::commands::get_map_services,
            t_map::commands::save_map_services,
            t_map::commands::get_map_tile,
            t_map::commands::cancel_map_tile,
            t_map::commands::resolve_photo_location,
            t_map::commands::resolve_library_locations,
            t_map::commands::cancel_location_resolution,
            t_cmds::delete_person,
            t_cmds::get_faces_for_file,
            t_cmds::get_person_thumbnail,
            // dedup
            t_cmds::dedup_start_scan,
            t_cmds::dedup_get_scan_status,
            t_cmds::dedup_cancel_scan,
            t_cmds::dedup_list_groups,
            t_cmds::dedup_get_overview,
            t_cmds::dedup_set_keep,
            t_cmds::dedup_delete,
            t_cmds::similar_start_scan,
            t_cmds::similar_get_scan_status,
            t_cmds::similar_cancel_scan,
            t_cmds::similar_get_eligible_count,
            t_cmds::similar_list_groups,
            t_cmds::similar_get_overview,
            t_cmds::similar_get_group,
            t_cmds::similar_set_keep,
            t_cmds::similar_has_scan,
            // video
            t_video::prepare_video,
            t_video::cancel_video_prepare,
            t_video::clear_video_cache,
            // backup / restore
            t_cmds::get_db_storage_info,
            t_cmds::backup_databases,
            t_cmds::parse_backup_file,
            t_cmds::restore_databases,
            t_cmds::get_restored_ui_preferences,
        ])
        .build(tauri::generate_context!());

    match run_result {
        Ok(app) => {
            app.run(move |app_handle, event| match event {
                tauri::RunEvent::ExitRequested { api, .. } if !t_window_exit::allowed() => {
                    api.prevent_exit();
                    if let Err(error) = t_window_exit::request(app_handle) {
                        eprintln!("Application exit cancelled: {error}");
                    }
                }
                tauri::RunEvent::Ready => {
                    if aptabase_enabled {
                        let _ = app_handle.track_event("app_started", None);
                    }
                }
                tauri::RunEvent::Exit { .. } => {
                    if aptabase_enabled {
                        let _ = app_handle.track_event("app_exited", None);
                        app_handle.flush_events_blocking();
                    }
                }

                // macOS: clicking the Dock icon of a running app reopens it.
                // When the main window is hidden (closed-to-hide), show it again.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                _ => {}
            });
        }
        Err(err) => {
            eprintln!("Error while running application: {}", err);
        }
    }
}
