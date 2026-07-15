// Modules
pub mod agent_worker;
mod ai_service;
pub mod app_config;
pub mod assistant;
pub mod backend_client;
pub mod commands;
pub mod data_backup;
pub mod document_editor;
pub mod feature_gate;
pub mod ffmpeg;
pub mod ktv_export;
pub mod legacy_import;
pub mod logging;
pub mod moonshot;
pub mod packaged_backend;
pub mod pdf_sidecar;
pub mod platform;
pub mod source_locator;
pub mod storage;
mod subtitle_extraction;
pub mod subtitle_import;
pub mod types;
pub mod video_server;
mod youtube;

// Re-exports
use agent_worker::{
    mark_running_tasks_interrupted_in_dir, recover_worker_checkpoints_from_backend,
    AgentWorkerManager,
};
use ai_service::AIServiceCache;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AIServiceCache::default())
        .manage(AgentWorkerManager::default())
        .manage(packaged_backend::PackagedBackendManager::default())
        .invoke_handler(tauri::generate_handler![
            // App initialization
            app_config::commands::init_app,
            // Configuration
            app_config::commands::get_config,
            app_config::commands::save_config_cmd,
            app_config::commands::backend_check_session_cmd,
            app_config::commands::backend_health_cmd,
            app_config::commands::backend_login_cmd,
            app_config::commands::backend_register_cmd,
            app_config::commands::backend_logout_cmd,
            packaged_backend::packaged_backend_status_cmd,
            legacy_import::run_legacy_import_cmd,
            legacy_import::get_legacy_import_cmd,
            app_config::commands::set_api_key,
            app_config::commands::save_model_config,
            app_config::commands::delete_model_config,
            app_config::commands::set_active_model_config,
            app_config::commands::get_active_model_config,
            // Articles
            commands::create_article,
            commands::resegment_article,
            commands::get_article,
            commands::list_articles_cmd,
            commands::material_library_list_cmd,
            commands::material_library_list_tags_cmd,
            commands::material_library_create_tag_cmd,
            commands::material_library_patch_tag_cmd,
            commands::material_library_delete_tag_cmd,
            commands::material_library_merge_tag_cmd,
            commands::material_library_get_tags_cmd,
            commands::material_library_set_tags_cmd,
            commands::material_library_bulk_tags_cmd,
            commands::material_library_get_reading_progress_cmd,
            commands::material_library_upsert_reading_progress_cmd,
            commands::material_library_create_import_job_cmd,
            commands::material_library_list_import_jobs_cmd,
            commands::material_library_get_import_job_cmd,
            commands::material_library_patch_import_job_cmd,
            commands::material_library_cancel_import_job_cmd,
            commands::material_library_resume_import_job_cmd,
            commands::material_library_duplicate_check_cmd,
            commands::material_library_bulk_archive_cmd,
            commands::material_library_bulk_unarchive_cmd,
            commands::material_library_bulk_delete_cmd,
            commands::preview_material_import_cmd,
            commands::update_article,
            commands::update_article_segment,
            document_editor::get_material_document_cmd,
            document_editor::preview_material_edit_cmd,
            document_editor::commit_material_edit_cmd,
            document_editor::get_material_draft_cmd,
            document_editor::save_material_draft_cmd,
            document_editor::delete_material_draft_cmd,
            document_editor::list_material_revisions_cmd,
            document_editor::get_material_revision_cmd,
            document_editor::restore_material_revision_cmd,
            document_editor::update_segment_derived_cmd,
            document_editor::create_editable_derivative_cmd,
            commands::delete_article_cmd,
            commands::list_learning_items_cmd,
            commands::get_learning_item_cmd,
            commands::create_learning_item_cmd,
            commands::create_learning_item_from_selection_cmd,
            commands::update_learning_item_cmd,
            commands::accept_learning_item_cmd,
            commands::delete_learning_item_cmd,
            commands::bulk_organize_learning_items_cmd,
            commands::migrate_legacy_learning_items_cmd,
            commands::list_learning_activity_events_cmd,
            commands::get_daily_learning_review_cmd,
            commands::get_learning_activity_heatmap_cmd,
            commands::get_material_learning_review_cmd,
            commands::record_local_preview_cmd,
            commands::list_annotations_cmd,
            commands::create_annotation_cmd,
            commands::update_annotation_cmd,
            commands::delete_annotation_cmd,
            commands::convert_annotation_to_learning_item_cmd,
            commands::fetch_url_content,
            video_server::get_resource_server_info_cmd,
            commands::import_web_material_cmd,
            commands::article_get_overview_cmd,
            commands::article_read_window_cmd,
            commands::article_search_cmd,
            commands::article_get_evidence_cmd,
            commands::task_report_progress_cmd,
            commands::artifact_save_cmd,
            commands::create_mind_map_task_cmd,
            commands::run_agent_turn_cmd,
            commands::get_agent_task_cmd,
            commands::get_artifact_cmd,
            commands::get_agent_worker_status_cmd,
            commands::stop_agent_worker_cmd,
            assistant::commands::assistant_task_list_cmd,
            assistant::commands::assistant_task_detail_cmd,
            assistant::commands::assistant_task_timeline_cmd,
            assistant::commands::assistant_task_cancel_cmd,
            assistant::commands::assistant_task_retry_cmd,
            assistant::commands::assistant_task_artifacts_cmd,
            assistant::commands::assistant_artifact_detail_cmd,
            assistant::commands::assistant_action_execute_cmd,
            assistant::commands::assistant_task_actions_cmd,
            // AI operations
            commands::translate_text,
            commands::analyze_text,
            commands::chat_completion,
            commands::stream_chat_completion,
            commands::translate_article,
            commands::analyze_article,
            commands::segment_translate_explain_cmd,
            // 收藏夹命令
            commands::create_word_pack_cmd,
            commands::update_word_pack_cmd,
            commands::list_word_packs_cmd,
            commands::delete_word_pack_cmd,
            commands::add_favorite_vocabulary_cmd,
            commands::list_favorite_vocabularies_cmd,
            commands::list_favorite_vocabularies_by_pack_cmd,
            commands::set_vocabulary_pack_ids_cmd,
            commands::get_due_vocabulary_queue_cmd,
            commands::review_vocabulary_cmd,
            commands::export_word_pack_cmd,
            commands::import_word_pack_cmd,
            commands::delete_favorite_vocabulary_cmd,
            commands::add_favorite_grammar_cmd,
            commands::list_favorite_grammars_cmd,
            commands::delete_favorite_grammar_cmd,
            // External
            commands::import_youtube_video_cmd,
            commands::import_local_video_cmd,
            commands::import_article_subtitles_cmd,
            commands::import_srt_file_cmd,
            commands::prepare_ktv_segments_cmd,
            commands::export_ktv_video_cmd,
            // 书籍导入
            commands::import_book_cmd,
            commands::import_text_file_cmd,
            // 字幕提取
            commands::extract_subtitles_cmd,
            // 文件操作
            commands::write_text_file,
            commands::write_binary_file,
            // 删除操作
            commands::delete_article_subtitles_cmd,
            commands::delete_article_analysis_cmd,
            // PDF翻译
            commands::translate_pdf_document,
            commands::check_pdf_translation_files,
            commands::export_file_cmd,
            // 全局日志
            logging::get_logs_cmd,
            logging::clear_logs_cmd,
            logging::append_log_cmd,
            logging::get_log_file_path_cmd,
            // 书签管理
            commands::add_bookmark_cmd,
            commands::list_bookmarks_cmd,
            commands::list_bookmarks_for_book_cmd,
            commands::update_bookmark_cmd,
            commands::delete_bookmark_cmd,
        ])
        .setup(|app| {
            // Initialize app on startup
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // Ensure app directories exist
                let _ = app_config::commands::init_app(app_handle.clone()).await;
                if let Ok(app_data_dir) = app_handle.path().app_data_dir() {
                    // Start the persistent log file as early as possible so the
                    // very first PDF translation of a session is captured.
                    logging::LogStore::global().init_file(&app_data_dir);
                    let _ = mark_running_tasks_interrupted_in_dir(&app_data_dir);
                }

                packaged_backend::start_packaged_backend_if_enabled(app_handle.clone()).await;
                if let Ok(app_data_dir) = app_handle.path().app_data_dir() {
                    if let Err(error) =
                        recover_worker_checkpoints_from_backend(&app_handle, &app_data_dir).await
                    {
                        eprintln!("[AgentWorker] Failed to recover Backend task state: {error}");
                    }
                }

                // 启动资源服务器 (视频 + 书籍)
                match app_handle.path().app_data_dir() {
                    Ok(app_data_dir) => {
                        if let Err(e) = video_server::start_resource_server(app_data_dir).await {
                            eprintln!("[ResourceServer] Failed to start: {}", e);
                        }
                    }
                    Err(e) => {
                        eprintln!("[ResourceServer] Failed to resolve app data dir: {}", e);
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
