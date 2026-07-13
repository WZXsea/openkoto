use crate::agent_worker::{
    default_base_url, resolve_runtime_provider_config, AgentWorkerManager,
    AgentWorkerStatusSnapshot,
};
use crate::ai_service::{get_ai_service, get_or_create_ai_service, AIServiceCache};
use crate::backend_client::{
    AcceptLearningItemRequest, AcceptLearningItemResponse, BackendClient, BackendClientError,
    BackendHealthResponse, BackendUser, CreateLearningItemFromSelectionRequest,
    CreateLearningItemRequest, CreateMaterialRequest, LearningItem, ListLearningItemsRequest,
    PatchMaterialRequest, UpdateLearningItemRequest,
};
use crate::feature_gate::require_external_tools_enabled;
use crate::ktv_export::{export_ktv_video, prepare_ktv_segments, KtvExportConfig, KtvExportResult};
use crate::moonshot::is_moonshot_provider;
use crate::platform::safe_file_io;
use crate::storage::{
    ensure_app_dirs,
    ensure_favorites_dirs,
    list_favorite_vocabularies,
    load_config,
    load_favorite_vocabulary,
    load_word_pack,
    save_config,
    // 收藏夹存储函数
    save_favorite_vocabulary,
    save_word_pack,
};
use crate::subtitle_import::{create_article_from_srt, import_subtitles_into_article};
use crate::types::{
    is_supported_material_import_source_kind, material_import_commit_recovery_strategy,
    material_import_commit_state_from_metadata, material_import_effective_duplicate_policy,
    material_import_metadata_with_commit_state, material_import_recorded_duplicate_policy,
    AgentTask, AgentTaskInput, AgentTaskStatus, AgentTaskType, AnalysisRequest, AnalysisResponse,
    AnalysisType, Article, ArticleEvidenceItem, ArticleEvidenceResult, ArticleOverview,
    ArticleSearchHit, ArticleSearchResult, ArticleSegment, ArticleTextWindow, Artifact,
    ArtifactType, AssistantConversationMessage, Bookmark, BulkMaterialIdsRequest,
    BulkMaterialTagsRequest, BulkOperationResponse, ChatRequest, ChatResponse,
    CreateMaterialImportJobRequest, CreateMaterialTagRequest, DeleteResponse,
    DuplicateCheckRequest, DuplicateCheckResponse, FavoriteGrammar, FavoriteVocabulary,
    ImportJobOptions, ListMaterialImportJobsQuery, ListMaterialsQuery, MaterialImportCommitState,
    MaterialImportJob, MaterialImportRecoveryStrategy, MaterialTag, MergeMaterialTagRequest,
    ModelConfig, PatchMaterialImportJobRequest, PatchMaterialTagRequest, PreviewMaterialFileInfo,
    PreviewMaterialImportRequest, PreviewMaterialImportResponse, ReadingProgress,
    SetMaterialTagsRequest, TimeRange, TranslationRequest, TranslationResponse,
    UpsertReadingProgressRequest, WordPack,
};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

pub type AppState<'a> = State<'a, AIServiceCache>;

pub use crate::types::MaterialSummary;

#[derive(Debug, Clone, Serialize)]
pub struct BackendSessionCheck {
    pub configured: bool,
    pub connected: bool,
    pub authenticated: bool,
    pub backend_url: Option<String>,
    pub user: Option<BackendUser>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendAuthResult {
    pub config: crate::types::AppConfig,
    pub user: BackendUser,
    pub expires_at: String,
}

fn backend_client_for_app(app_handle: &AppHandle) -> Result<BackendClient, String> {
    let config = load_config(app_handle)?.unwrap_or_default();
    BackendClient::from_app_config(&config).map_err(backend_error_to_string)
}

fn backend_error_to_string(error: BackendClientError) -> String {
    match error {
        BackendClientError::NotConfigured => {
            "Backend is required. Configure backend URL and sign in before using materials."
                .to_string()
        }
        other => other.to_string(),
    }
}

fn is_invalid_backend_token(error: &BackendClientError) -> bool {
    matches!(
        error,
        BackendClientError::Backend {
            status,
            code,
            ..
        } if *status == StatusCode::UNAUTHORIZED && code == "invalid_token"
    )
}

#[cfg(test)]
mod backend_session_tests {
    use super::*;

    #[test]
    fn invalid_backend_token_detection_is_specific() {
        let invalid_token = BackendClientError::Backend {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_token".to_string(),
            message: "invalid bearer token".to_string(),
        };
        let wrong_password = BackendClientError::Backend {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_credentials".to_string(),
            message: "invalid credentials".to_string(),
        };

        assert!(is_invalid_backend_token(&invalid_token));
        assert!(!is_invalid_backend_token(&wrong_password));
    }

    #[test]
    fn network_import_hash_uses_backend_url_normalization() {
        let first =
            normalized_network_source_sha256("https://EXAMPLE.com:443/path?z=2&a=1#ignored")
                .unwrap();
        let second = normalized_network_source_sha256("https://example.com/path?a=1&z=2").unwrap();

        assert_eq!(first, second);
        assert!(normalized_network_source_sha256("file:///tmp/private.txt").is_err());
        assert!(normalized_network_source_sha256("https://user@example.com/path").is_err());
    }

    #[test]
    fn content_hash_normalizes_whitespace_and_unicode() {
        assert_eq!(content_sha256("a   b\t c"), content_sha256("a b c"));
        assert_eq!(content_sha256("cafe\u{301}"), content_sha256("caf\u{e9}"));
    }

    #[test]
    fn import_job_metadata_keeps_resume_parameters() {
        let source = MaterialImportSource {
            source_kind: "video".to_string(),
            source_uri: Some("file:///tmp/video.mp4".to_string()),
            content: None,
            file_path: Some(PathBuf::from("/tmp/video.mp4")),
            file_id: None,
            title: Some("Video".to_string()),
            metadata: serde_json::json!({
                "source": "desktop_import",
                "subtitle_path": "/tmp/video.srt",
            }),
        };

        let metadata = import_job_metadata(&source);
        assert_eq!(metadata["resume_payload"]["source_kind"], "video");
        assert_eq!(metadata["resume_payload"]["file_path"], "/tmp/video.mp4");
        assert_eq!(
            metadata["resume_payload"]["subtitle_path"],
            "/tmp/video.srt"
        );
    }

    #[test]
    fn subtitle_attachment_hash_detects_repeated_file() {
        let metadata = serde_json::json!({ "subtitle_file": { "sha256": "abc" } });
        assert!(metadata_has_subtitle_hash(&metadata, "abc"));
        assert!(!metadata_has_subtitle_hash(&metadata, "def"));
    }

    #[test]
    fn import_commit_rejects_source_different_from_preview() {
        let source = MaterialImportSource {
            source_kind: "article".to_string(),
            source_uri: Some("https://example.com/source".to_string()),
            content: Some("previewed content".to_string()),
            file_path: None,
            file_id: None,
            title: Some("Previewed title".to_string()),
            metadata: serde_json::json!({ "source": "desktop_create_article" }),
        };
        let input_hash = content_sha256(source.content.as_deref().unwrap()).unwrap();
        let job = MaterialImportJob {
            id: Uuid::new_v4().to_string(),
            source_kind: source.source_kind.clone(),
            source_uri: source.source_uri.clone(),
            normalized_source_url: source.source_uri.clone(),
            file_id: None,
            input_hash: Some(input_hash.clone()),
            file_sha256: None,
            content_sha256: Some(input_hash.clone()),
            status: "preview_ready".to_string(),
            progress: 0.75,
            error_code: None,
            error_message: None,
            result_material_id: None,
            preview: serde_json::json!({}),
            metadata: import_job_metadata(&source),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            started_at: None,
            finished_at: None,
        };

        assert!(validate_import_job_source(&job, &source, &input_hash).is_ok());
        let mut changed = source.clone();
        changed.title = Some("Changed after preview".to_string());
        assert!(validate_import_job_source(&job, &changed, &input_hash).is_err());
        assert!(validate_import_job_source(&job, &source, &"f".repeat(64)).is_err());
    }
}

const MATERIAL_LIBRARY_MAX_BULK_IDS: usize = 100;

fn validate_uuid(value: &str, field: &str) -> Result<(), String> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| format!("{field} must be a UUID"))
}

fn validate_bulk_ids(ids: &[String], field: &str, allow_empty: bool) -> Result<(), String> {
    if (!allow_empty && ids.is_empty()) || ids.len() > MATERIAL_LIBRARY_MAX_BULK_IDS {
        return Err(format!(
            "{field} must contain between {} and {MATERIAL_LIBRARY_MAX_BULK_IDS} values",
            if allow_empty { 0 } else { 1 }
        ));
    }
    let mut seen = HashSet::new();
    for id in ids {
        validate_uuid(id, field)?;
        if !seen.insert(id) {
            return Err(format!("{field} must not contain duplicate values"));
        }
    }
    Ok(())
}

fn validate_import_job_query(query: &ListMaterialImportJobsQuery) -> Result<(), String> {
    if let Some(status) = query.status.as_deref() {
        if !matches!(
            status,
            "queued"
                | "validating"
                | "parsing"
                | "preview_ready"
                | "committing"
                | "succeeded"
                | "failed_retryable"
                | "failed_terminal"
                | "cancelled"
        ) {
            return Err("unsupported material import status".to_string());
        }
    }
    if let Some(limit) = query.limit.as_deref() {
        let value = limit
            .parse::<u64>()
            .map_err(|_| "limit must be an integer".to_string())?;
        if !(1..=200).contains(&value) {
            return Err("limit must be between 1 and 200".to_string());
        }
    }
    if let Some(offset) = query.offset.as_deref() {
        offset
            .parse::<u64>()
            .map_err(|_| "offset must be a non-negative integer".to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn material_library_list_cmd(
    app_handle: AppHandle,
    query: Option<ListMaterialsQuery>,
) -> Result<Vec<Article>, String> {
    backend_client_for_app(&app_handle)?
        .list_materials_with_query(query.as_ref())
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_list_tags_cmd(
    app_handle: AppHandle,
) -> Result<Vec<MaterialTag>, String> {
    backend_client_for_app(&app_handle)?
        .list_material_tags()
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_create_tag_cmd(
    app_handle: AppHandle,
    request: CreateMaterialTagRequest,
) -> Result<MaterialTag, String> {
    if request.name.trim().is_empty() {
        return Err("tag name is required".to_string());
    }
    backend_client_for_app(&app_handle)?
        .create_material_tag(&request)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_patch_tag_cmd(
    app_handle: AppHandle,
    id: String,
    request: PatchMaterialTagRequest,
) -> Result<MaterialTag, String> {
    validate_uuid(&id, "id")?;
    if request.name.is_none() && request.color.is_none() {
        return Err("at least one tag field is required".to_string());
    }
    if request
        .name
        .as_deref()
        .is_some_and(|name| name.trim().is_empty())
    {
        return Err("tag name is required".to_string());
    }
    backend_client_for_app(&app_handle)?
        .patch_material_tag(&id, &request)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_delete_tag_cmd(
    app_handle: AppHandle,
    id: String,
) -> Result<DeleteResponse, String> {
    validate_uuid(&id, "id")?;
    backend_client_for_app(&app_handle)?
        .delete_material_tag(&id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_merge_tag_cmd(
    app_handle: AppHandle,
    source_tag_id: String,
    request: MergeMaterialTagRequest,
) -> Result<MaterialTag, String> {
    validate_uuid(&source_tag_id, "source_tag_id")?;
    validate_uuid(&request.target_tag_id, "target_tag_id")?;
    if source_tag_id == request.target_tag_id {
        return Err("source and target tags must differ".to_string());
    }
    backend_client_for_app(&app_handle)?
        .merge_material_tag(&source_tag_id, &request)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_get_tags_cmd(
    app_handle: AppHandle,
    material_id: String,
) -> Result<Vec<MaterialTag>, String> {
    validate_uuid(&material_id, "material_id")?;
    backend_client_for_app(&app_handle)?
        .get_material_tags(&material_id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_set_tags_cmd(
    app_handle: AppHandle,
    material_id: String,
    request: SetMaterialTagsRequest,
) -> Result<Vec<MaterialTag>, String> {
    validate_uuid(&material_id, "material_id")?;
    validate_bulk_ids(&request.tag_ids, "tag_ids", true)?;
    backend_client_for_app(&app_handle)?
        .set_material_tags(&material_id, &request)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_bulk_tags_cmd(
    app_handle: AppHandle,
    request: BulkMaterialTagsRequest,
) -> Result<BulkOperationResponse, String> {
    if !matches!(request.mode.as_str(), "add" | "remove" | "replace") {
        return Err("mode must be add, remove, or replace".to_string());
    }
    validate_bulk_ids(&request.ids, "ids", false)?;
    validate_bulk_ids(&request.tag_ids, "tag_ids", request.mode == "replace")?;
    backend_client_for_app(&app_handle)?
        .bulk_material_tags(&request)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_get_reading_progress_cmd(
    app_handle: AppHandle,
    material_id: String,
) -> Result<Option<ReadingProgress>, String> {
    validate_uuid(&material_id, "material_id")?;
    backend_client_for_app(&app_handle)?
        .get_reading_progress(&material_id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_upsert_reading_progress_cmd(
    app_handle: AppHandle,
    material_id: String,
    request: UpsertReadingProgressRequest,
) -> Result<ReadingProgress, String> {
    validate_uuid(&material_id, "material_id")?;
    if !request.progress_ratio.is_finite() || !(0.0..=1.0).contains(&request.progress_ratio) {
        return Err("progress_ratio must be between 0 and 1".to_string());
    }
    if !matches!(
        request.reader_kind.as_str(),
        "article" | "pdf" | "epub" | "txt" | "media"
    ) {
        return Err("unsupported reader_kind".to_string());
    }
    if !request.locator.is_object() {
        return Err("locator must be a JSON object".to_string());
    }
    backend_client_for_app(&app_handle)?
        .upsert_reading_progress(&material_id, &request)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_create_import_job_cmd(
    app_handle: AppHandle,
    request: CreateMaterialImportJobRequest,
) -> Result<MaterialImportJob, String> {
    backend_client_for_app(&app_handle)?
        .create_material_import_job(&request)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_list_import_jobs_cmd(
    app_handle: AppHandle,
    query: Option<ListMaterialImportJobsQuery>,
) -> Result<Vec<MaterialImportJob>, String> {
    if let Some(query) = query.as_ref() {
        validate_import_job_query(query)?;
    }
    backend_client_for_app(&app_handle)?
        .list_material_import_jobs(query.as_ref())
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_get_import_job_cmd(
    app_handle: AppHandle,
    id: String,
) -> Result<MaterialImportJob, String> {
    validate_uuid(&id, "id")?;
    backend_client_for_app(&app_handle)?
        .get_material_import_job(&id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_patch_import_job_cmd(
    app_handle: AppHandle,
    id: String,
    request: PatchMaterialImportJobRequest,
) -> Result<MaterialImportJob, String> {
    validate_uuid(&id, "id")?;
    if request
        .progress
        .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err("progress must be between 0 and 1".to_string());
    }
    backend_client_for_app(&app_handle)?
        .patch_material_import_job(&id, &request)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_cancel_import_job_cmd(
    app_handle: AppHandle,
    id: String,
) -> Result<MaterialImportJob, String> {
    validate_uuid(&id, "id")?;
    backend_client_for_app(&app_handle)?
        .cancel_material_import_job(&id)
        .await
        .map_err(backend_error_to_string)
}

fn resume_string(payload: &serde_json::Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
}

fn metadata_has_subtitle_hash(metadata: &serde_json::Value, file_hash: &str) -> bool {
    metadata
        .pointer("/subtitle_file/sha256")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|existing_hash| existing_hash == file_hash)
}

#[tauri::command]
pub async fn material_library_resume_import_job_cmd(
    app_handle: AppHandle,
    id: String,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    validate_uuid(&id, "id")?;
    let client = backend_client_for_app(&app_handle)?;
    let mut job = client
        .get_material_import_job(&id)
        .await
        .map_err(backend_error_to_string)?;
    if job.status == "succeeded" {
        let material_id = job
            .result_material_id
            .as_deref()
            .ok_or_else(|| "completed import job is missing result_material_id".to_string())?;
        return client
            .get_material(material_id)
            .await
            .map_err(backend_error_to_string);
    }
    let duplicate_policy = material_import_effective_duplicate_policy(
        duplicate_policy.as_deref(),
        material_import_recorded_duplicate_policy(&job.metadata).as_deref(),
    )?;
    parse_duplicate_policy(duplicate_policy.as_deref())?;
    if let Some(article) = recover_completed_import_side_effect(&client, job.clone()).await? {
        return Ok(article);
    }
    if job.status == "committing" {
        let updated_at = chrono::DateTime::parse_from_rfc3339(&job.updated_at)
            .map_err(|_| "material import job has an invalid updated_at timestamp".to_string())?;
        if chrono::Utc::now().signed_duration_since(updated_at.with_timezone(&chrono::Utc))
            < chrono::Duration::minutes(5)
        {
            return Err("material import is still committing; retry after the current operation has had time to finish".to_string());
        }
        job = client
            .patch_material_import_job(
                &id,
                &PatchMaterialImportJobRequest {
                    status: Some("failed_retryable".to_string()),
                    error_code: Some(Some("interrupted_commit".to_string())),
                    error_message: Some(Some(
                        "the previous commit did not report completion and can be resumed"
                            .to_string(),
                    )),
                    ..Default::default()
                },
            )
            .await
            .map_err(backend_error_to_string)?;
    }
    if !matches!(job.status.as_str(), "failed_retryable" | "preview_ready") {
        return Err(
            "only failed_retryable, preview_ready, or stale committing material imports can be resumed".to_string(),
        );
    }
    let payload = job
        .metadata
        .get("resume_payload")
        .filter(|value| value.is_object())
        .ok_or_else(|| "material import job does not contain resume parameters".to_string())?;
    let source_kind =
        resume_string(payload, "source_kind").unwrap_or_else(|| job.source_kind.clone());
    if !is_supported_material_import_source_kind(&source_kind) {
        return Err(format!(
            "unsupported material import retry source_kind: {source_kind}"
        ));
    }
    let file_path = || {
        resume_string(payload, "file_path")
            .ok_or_else(|| "material import retry requires the original file path".to_string())
    };

    match source_kind.as_str() {
        "article" => {
            create_article(
                app_handle,
                resume_string(payload, "title").unwrap_or_else(|| "Untitled Material".to_string()),
                resume_string(payload, "content")
                    .ok_or_else(|| "article retry requires original content".to_string())?,
                resume_string(payload, "source_uri"),
                Some(id),
                duplicate_policy,
            )
            .await
        }
        "url" => {
            import_web_material_cmd(
                app_handle,
                resume_string(payload, "source_uri")
                    .ok_or_else(|| "URL retry requires source_uri".to_string())?,
                resume_string(payload, "title"),
                resume_string(payload, "content")
                    .ok_or_else(|| "URL retry requires extracted content".to_string())?,
                Some(id),
                duplicate_policy,
            )
            .await
        }
        "text_file" => {
            import_text_file_cmd(
                app_handle,
                file_path()?,
                resume_string(payload, "title"),
                Some(id),
                duplicate_policy,
            )
            .await
        }
        "book" => {
            import_book_cmd(
                app_handle,
                file_path()?,
                resume_string(payload, "title"),
                Some(id),
                duplicate_policy,
            )
            .await
        }
        "audio" | "video" => {
            import_local_video_cmd(
                app_handle,
                file_path()?,
                resume_string(payload, "subtitle_path"),
                Some(id),
                duplicate_policy,
            )
            .await
        }
        "youtube" => {
            import_youtube_video_cmd(
                app_handle,
                resume_string(payload, "source_uri")
                    .ok_or_else(|| "YouTube retry requires source_uri".to_string())?,
                Some(id),
                duplicate_policy,
            )
            .await
        }
        "subtitle" if resume_string(payload, "mode").as_deref() == Some("attach") => {
            import_article_subtitles_cmd(
                app_handle,
                resume_string(payload, "target_material_id")
                    .ok_or_else(|| "subtitle retry requires target_material_id".to_string())?,
                file_path()?,
                Some(id),
                duplicate_policy,
            )
            .await
        }
        "subtitle" => {
            import_srt_file_cmd(
                app_handle,
                file_path()?,
                resume_string(payload, "title"),
                Some(id),
                duplicate_policy,
            )
            .await
        }
        _ => Err(format!(
            "unsupported material import retry source_kind: {source_kind}"
        )),
    }
}

#[tauri::command]
pub async fn material_library_duplicate_check_cmd(
    app_handle: AppHandle,
    request: DuplicateCheckRequest,
) -> Result<DuplicateCheckResponse, String> {
    if request.source_url.as_deref().is_none_or(str::is_empty)
        && request.content.as_deref().is_none_or(str::is_empty)
        && request.content_sha256.as_deref().is_none_or(str::is_empty)
        && request.file_sha256.as_deref().is_none_or(str::is_empty)
    {
        return Err("at least one duplicate check input is required".to_string());
    }
    backend_client_for_app(&app_handle)?
        .check_material_duplicates(&request)
        .await
        .map_err(backend_error_to_string)
}

async fn material_library_bulk_operation(
    app_handle: &AppHandle,
    request: BulkMaterialIdsRequest,
    operation: &str,
) -> Result<BulkOperationResponse, String> {
    validate_bulk_ids(&request.ids, "ids", false)?;
    let client = backend_client_for_app(app_handle)?;
    let result = match operation {
        "archive" => client.bulk_archive_materials(&request).await,
        "unarchive" => client.bulk_unarchive_materials(&request).await,
        "delete" => client.bulk_delete_materials(&request).await,
        _ => unreachable!("unsupported material bulk operation"),
    };
    result.map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn material_library_bulk_archive_cmd(
    app_handle: AppHandle,
    request: BulkMaterialIdsRequest,
) -> Result<BulkOperationResponse, String> {
    material_library_bulk_operation(&app_handle, request, "archive").await
}

#[tauri::command]
pub async fn material_library_bulk_unarchive_cmd(
    app_handle: AppHandle,
    request: BulkMaterialIdsRequest,
) -> Result<BulkOperationResponse, String> {
    material_library_bulk_operation(&app_handle, request, "unarchive").await
}

#[tauri::command]
pub async fn material_library_bulk_delete_cmd(
    app_handle: AppHandle,
    request: BulkMaterialIdsRequest,
) -> Result<BulkOperationResponse, String> {
    material_library_bulk_operation(&app_handle, request, "delete").await
}

fn sha256_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn normalized_network_source_sha256(value: &str) -> Result<String, String> {
    let mut url = url::Url::parse(value)
        .map_err(|_| "network import source must be an absolute URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
    {
        return Err(
            "network import source must use http or https and must not contain credentials"
                .to_string(),
        );
    }
    url.set_fragment(None);
    if (url.scheme() == "http" && url.port() == Some(80))
        || (url.scheme() == "https" && url.port() == Some(443))
    {
        let _ = url.set_port(None);
    }
    let mut pairs = url.query_pairs().into_owned().collect::<Vec<_>>();
    pairs.sort();
    url.set_query(None);
    if !pairs.is_empty() {
        url.query_pairs_mut().extend_pairs(pairs);
    }
    Ok(sha256_bytes(url.as_str().as_bytes()))
}

fn content_sha256(content: &str) -> Option<String> {
    let normalized = content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .nfc()
        .collect::<String>();
    (!normalized.is_empty()).then(|| sha256_bytes(normalized.as_bytes()))
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("Failed to open file for SHA-256: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Failed to read file for SHA-256: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

fn validate_sha256_hex(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!(
            "{field} must be a 64-character hexadecimal SHA-256"
        ));
    }
    Ok(value)
}

fn count_preview_paragraphs(content: Option<&str>) -> usize {
    let Some(content) = content else {
        return 0;
    };
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    normalized
        .split("\n\n")
        .filter(|paragraph| !paragraph.trim().is_empty())
        .count()
}

fn preview_title(request: &PreviewMaterialImportRequest, path: Option<&Path>) -> String {
    request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            path.and_then(Path::file_stem)
                .and_then(|value| value.to_str())
                .map(str::to_string)
        })
        .or_else(|| request.source_uri.clone())
        .unwrap_or_else(|| "Untitled Material".to_string())
}

fn retryable_backend_error(error: &BackendClientError) -> bool {
    match error {
        BackendClientError::Request(_) | BackendClientError::Io(_) => true,
        BackendClientError::Backend { status, .. } => {
            status.is_server_error()
                || *status == StatusCode::REQUEST_TIMEOUT
                || *status == StatusCode::TOO_MANY_REQUESTS
        }
        BackendClientError::NotConfigured | BackendClientError::InvalidFileName => false,
    }
}

async fn mark_import_job_failed(
    client: &BackendClient,
    job_id: &str,
    error_code: &str,
    error_message: &str,
    retryable: bool,
) {
    let _ = client
        .patch_material_import_job(
            job_id,
            &PatchMaterialImportJobRequest {
                status: Some(if retryable {
                    "failed_retryable".to_string()
                } else {
                    "failed_terminal".to_string()
                }),
                error_code: Some(Some(error_code.to_string())),
                error_message: Some(Some(error_message.to_string())),
                ..Default::default()
            },
        )
        .await;
}

async fn patch_import_job(
    client: &BackendClient,
    job_id: &str,
    status: &str,
    progress: f64,
    preview: Option<serde_json::Value>,
    result_material_id: Option<String>,
) -> Result<MaterialImportJob, BackendClientError> {
    client
        .patch_material_import_job(
            job_id,
            &PatchMaterialImportJobRequest {
                status: Some(status.to_string()),
                progress: Some(progress),
                result_material_id: result_material_id.map(Some),
                preview,
                ..Default::default()
            },
        )
        .await
}

fn preview_content_from_request(
    request: &PreviewMaterialImportRequest,
    path: Option<&Path>,
) -> Result<Option<String>, String> {
    if let Some(content) = request.content.as_ref() {
        return Ok(Some(content.clone()));
    }
    let Some(path) = path else {
        return Ok(None);
    };
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if is_text_import_extension(&extension) {
        return read_text_import_content(path, &extension).map(Some);
    }
    if extension == "srt" {
        return std::fs::read_to_string(path)
            .map(Some)
            .map_err(|error| format!("Failed to read subtitle file: {error}"));
    }
    Ok(None)
}

#[tauri::command]
pub async fn preview_material_import_cmd(
    app_handle: AppHandle,
    request: PreviewMaterialImportRequest,
) -> Result<PreviewMaterialImportResponse, String> {
    if !is_supported_material_import_source_kind(&request.source_kind) {
        return Err("unsupported material import source_kind".to_string());
    }

    let path = request.file_path.as_deref().map(Path::new);
    if let Some(path) = path {
        if !path.is_file() {
            return Err(format!("File does not exist: {}", path.display()));
        }
    }
    if let Some(file_id) = request.file_id.as_deref() {
        validate_uuid(file_id, "file_id")?;
    }

    let requested_content = request.content.clone();
    let initial_content_sha = match request.content_sha256.as_deref() {
        Some(value) => Some(validate_sha256_hex(value, "content_sha256")?),
        None => requested_content.as_deref().and_then(content_sha256),
    };
    let computed_file_sha = match request.file_sha256.as_deref() {
        Some(value) => Some(validate_sha256_hex(value, "file_sha256")?),
        None => path.map(file_sha256).transpose()?,
    };
    let input_hash = match request.input_hash.as_deref() {
        Some(value) => validate_sha256_hex(value, "input_hash")?,
        None => initial_content_sha
            .clone()
            .or_else(|| computed_file_sha.clone())
            .map(Ok)
            .unwrap_or_else(|| {
                normalized_network_source_sha256(request.source_uri.as_deref().ok_or_else(
                    || "content, file_path, input_hash, or source_uri is required".to_string(),
                )?)
            })?,
    };

    let client = backend_client_for_app(&app_handle)?;
    let preview_source = MaterialImportSource {
        source_kind: request.source_kind.clone(),
        source_uri: request.source_uri.clone(),
        content: requested_content.clone(),
        file_path: request.file_path.as_deref().map(PathBuf::from),
        file_id: request.file_id.clone(),
        title: request.title.clone(),
        metadata: request
            .metadata
            .clone()
            .unwrap_or_else(|| serde_json::json!({})),
    };
    let job_metadata = import_job_metadata(&preview_source);
    let mut current_job = client
        .create_material_import_job(&CreateMaterialImportJobRequest {
            id: None,
            source_kind: request.source_kind.clone(),
            source_uri: request.source_uri.clone(),
            file_id: request.file_id.clone(),
            input_hash: Some(input_hash),
            file_sha256: computed_file_sha.clone(),
            content: requested_content.clone(),
            content_sha256: initial_content_sha,
            preview: serde_json::json!({}),
            metadata: job_metadata,
        })
        .await
        .map_err(backend_error_to_string)?;
    current_job = patch_import_job(&client, &current_job.id, "validating", 0.15, None, None)
        .await
        .map_err(backend_error_to_string)?;

    let content = match preview_content_from_request(&request, path) {
        Ok(content) => content,
        Err(message) => {
            mark_import_job_failed(
                &client,
                &current_job.id,
                "preview_parse_failed",
                &message,
                retryable_import_message(&message),
            )
            .await;
            return Err(message);
        }
    };
    let computed_content_sha = content.as_deref().and_then(content_sha256);
    current_job = patch_import_job(&client, &current_job.id, "parsing", 0.55, None, None)
        .await
        .map_err(backend_error_to_string)?;

    let source_url = request
        .source_uri
        .as_ref()
        .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
        .cloned();
    let skip_material_duplicate_check = request
        .metadata
        .as_ref()
        .and_then(|value| value.get("skip_material_duplicate_check"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let duplicates = if skip_material_duplicate_check {
        DuplicateCheckResponse {
            duplicate: false,
            normalized_source_url: None,
            content_sha256: computed_content_sha,
            file_sha256: computed_file_sha.clone(),
            matches: Vec::new(),
        }
    } else {
        match client
            .check_material_duplicates(&DuplicateCheckRequest {
                source_url,
                content: content.clone(),
                content_sha256: computed_content_sha,
                file_sha256: computed_file_sha.clone(),
            })
            .await
        {
            Ok(duplicates) => duplicates,
            Err(error) => {
                let message = error.to_string();
                mark_import_job_failed(
                    &client,
                    &current_job.id,
                    "preview_duplicate_check_failed",
                    &message,
                    retryable_backend_error(&error),
                )
                .await;
                return Err(backend_error_to_string(error));
            }
        }
    };

    let file_metadata = path
        .map(std::fs::metadata)
        .transpose()
        .map_err(|error| format!("Failed to read file metadata for import preview: {error}"))?;
    let title = preview_title(&request, path);
    let file_info = PreviewMaterialFileInfo {
        file_path: request.file_path.clone(),
        file_id: request.file_id.clone(),
        file_name: path
            .and_then(Path::file_name)
            .and_then(|value| value.to_str())
            .map(str::to_string),
        byte_size: file_metadata.map(|value| value.len()),
        sha256: computed_file_sha.clone(),
    };
    let paragraph_count = count_preview_paragraphs(content.as_deref());
    let content_snippet = content
        .as_deref()
        .map(|value| value.chars().take(500).collect::<String>());
    let source_uri = request.source_uri.clone();
    let preview = serde_json::json!({
        "title": title,
        "file": file_info,
        "paragraph_count": paragraph_count,
        "content_snippet": content_snippet,
        "source_uri": source_uri,
        "duplicates": duplicates,
    });
    current_job = match patch_import_job(
        &client,
        &current_job.id,
        "preview_ready",
        0.75,
        Some(preview),
        None,
    )
    .await
    {
        Ok(job) => job,
        Err(error) => {
            let message = error.to_string();
            mark_import_job_failed(
                &client,
                &current_job.id,
                "preview_failed",
                &message,
                retryable_backend_error(&error),
            )
            .await;
            return Err(backend_error_to_string(error));
        }
    };

    Ok(PreviewMaterialImportResponse {
        title,
        source_uri,
        file: file_info,
        paragraph_count,
        content_snippet,
        duplicates,
        job: current_job,
    })
}

#[derive(Debug, Clone)]
struct MaterialImportSource {
    source_kind: String,
    source_uri: Option<String>,
    content: Option<String>,
    file_path: Option<PathBuf>,
    file_id: Option<String>,
    title: Option<String>,
    metadata: serde_json::Value,
}

fn import_job_metadata(source: &MaterialImportSource) -> serde_json::Value {
    let mut metadata = source.metadata.clone();
    if !metadata.is_object() {
        metadata = serde_json::json!({});
    }
    let mut resume_payload = serde_json::json!({
        "source_kind": source.source_kind,
        "source_uri": source.source_uri,
        "content": source.content,
        "file_path": source.file_path.as_ref().map(|path| path.to_string_lossy().into_owned()),
        "file_id": source.file_id,
        "title": source.title,
    });
    if let (Some(resume), Some(extra)) =
        (resume_payload.as_object_mut(), source.metadata.as_object())
    {
        for (key, value) in extra {
            resume.entry(key.clone()).or_insert_with(|| value.clone());
        }
    }
    metadata
        .as_object_mut()
        .unwrap()
        .insert("resume_payload".to_string(), resume_payload);
    metadata
}

fn validate_import_job_source(
    job: &MaterialImportJob,
    source: &MaterialImportSource,
    input_hash: &str,
) -> Result<(), String> {
    if job.source_kind != source.source_kind {
        return Err("import_job_id belongs to a different source_kind".to_string());
    }
    if job.input_hash.as_deref() != Some(input_hash) {
        return Err(
            "import source does not match the input hash recorded during preview".to_string(),
        );
    }
    let stored = job
        .metadata
        .get("resume_payload")
        .filter(|value| value.is_object())
        .ok_or_else(|| {
            "material import job is missing its preview source parameters".to_string()
        })?;
    let expected = import_job_metadata(source)
        .get("resume_payload")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    for field in [
        "source_kind",
        "source_uri",
        "content",
        "file_path",
        "file_id",
    ] {
        if stored.get(field) != expected.get(field) {
            return Err(format!(
                "import source field {field} does not match the previewed input"
            ));
        }
    }
    if stored.get("title").is_some_and(|value| !value.is_null())
        && stored.get("title") != expected.get("title")
    {
        return Err("import source field title does not match the previewed input".to_string());
    }
    Ok(())
}

fn metadata_for_import_commit(
    metadata: Option<serde_json::Value>,
    job_id: &str,
) -> serde_json::Value {
    let mut metadata = metadata
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}));
    metadata
        .as_object_mut()
        .unwrap()
        .insert("import_job_id".to_string(), serde_json::json!(job_id));
    metadata
}

fn metadata_for_replaced_material(
    existing_metadata: &serde_json::Value,
    imported_article_metadata: &serde_json::Value,
    imported_metadata: Option<serde_json::Value>,
    job_id: &str,
) -> serde_json::Value {
    let mut metadata = existing_metadata
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);
    for candidate in [Some(imported_article_metadata), imported_metadata.as_ref()] {
        if let Some(values) = candidate.and_then(serde_json::Value::as_object) {
            for (key, value) in values {
                metadata.insert(key.clone(), value.clone());
            }
        }
    }
    metadata_for_import_commit(Some(serde_json::Value::Object(metadata)), job_id)
}

fn material_was_committed_by_job(article: &Article, job_id: &str) -> bool {
    article
        .metadata
        .get("import_job_id")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value == job_id)
}

async fn persist_import_job_commit_state(
    client: &BackendClient,
    job: &MaterialImportJob,
    state: &MaterialImportCommitState,
) -> Result<MaterialImportJob, BackendClientError> {
    client
        .patch_material_import_job_metadata(
            &job.id,
            material_import_metadata_with_commit_state(job.metadata.clone(), state),
        )
        .await
}

async fn finish_import_job(
    client: &BackendClient,
    job_id: &str,
    material_id: &str,
) -> Result<(), BackendClientError> {
    let mut last_error = None;
    for attempt in 0..3 {
        match patch_import_job(
            client,
            job_id,
            "succeeded",
            1.0,
            None,
            Some(material_id.to_string()),
        )
        .await
        {
            Ok(_) => return Ok(()),
            Err(error) => {
                if let Ok(job) = client.get_material_import_job(job_id).await {
                    if job.status == "succeeded"
                        && job.result_material_id.as_deref() == Some(material_id)
                    {
                        return Ok(());
                    }
                }
                last_error = Some(error);
                if attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_millis(150 * (attempt + 1))).await;
                }
            }
        }
    }
    Err(last_error.expect("finish_import_job must record an error"))
}

struct PreparedMaterialImport {
    article: Article,
    metadata: Option<serde_json::Value>,
    file_sha256: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportDuplicatePolicy {
    Cancel,
    OpenExisting,
    Replace,
    KeepCopy,
}

impl ImportDuplicatePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cancel => "cancel",
            Self::OpenExisting => "open_existing",
            Self::Replace => "replace",
            Self::KeepCopy => "keep_copy",
        }
    }
}

fn parse_duplicate_policy(value: Option<&str>) -> Result<Option<ImportDuplicatePolicy>, String> {
    value
        .map(|value| match value {
            "cancel" => Ok(ImportDuplicatePolicy::Cancel),
            "open_existing" => Ok(ImportDuplicatePolicy::OpenExisting),
            "replace" => Ok(ImportDuplicatePolicy::Replace),
            "keep_copy" => Ok(ImportDuplicatePolicy::KeepCopy),
            _ => Err(
                "duplicate_policy must be cancel, open_existing, replace, or keep_copy".to_string(),
            ),
        })
        .transpose()
}

fn import_commit_state(
    policy: Option<ImportDuplicatePolicy>,
    commit_kind: &str,
    target_material_id: impl Into<String>,
) -> MaterialImportCommitState {
    MaterialImportCommitState {
        duplicate_policy: policy
            .map(ImportDuplicatePolicy::as_str)
            .map(str::to_string),
        commit_kind: Some(commit_kind.to_string()),
        target_material_id: Some(target_material_id.into()),
        result_material_id: None,
    }
}

fn retryable_import_message(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "timeout",
        "timed out",
        "connection",
        "temporarily",
        "network",
        "failed to execute",
        "failed to upload",
        "rate limit",
        "429",
        "502",
        "503",
        "504",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn import_source_hashes(
    source: &MaterialImportSource,
) -> Result<(Option<String>, Option<String>, String), String> {
    let content_hash = source.content.as_deref().and_then(content_sha256);
    let file_hash = source.file_path.as_deref().map(file_sha256).transpose()?;
    let input_hash =
        match content_hash.clone().or_else(|| file_hash.clone()) {
            Some(hash) => hash,
            None => normalized_network_source_sha256(source.source_uri.as_deref().ok_or_else(
                || "import source must include content, file_path, or source_uri".to_string(),
            )?)?,
        };
    Ok((content_hash, file_hash, input_hash))
}

fn duplicate_check_for_source(
    source: &MaterialImportSource,
    content: Option<String>,
    content_sha256: Option<String>,
    file_sha256: Option<String>,
) -> DuplicateCheckRequest {
    DuplicateCheckRequest {
        source_url: source
            .source_uri
            .as_ref()
            .filter(|value| value.starts_with("http://") || value.starts_with("https://"))
            .cloned(),
        content,
        content_sha256,
        file_sha256,
    }
}

async fn advance_import_job_to_parsing(
    client: &BackendClient,
    mut job: MaterialImportJob,
) -> Result<MaterialImportJob, BackendClientError> {
    if job.status == "failed_retryable" {
        job = patch_import_job(client, &job.id, "queued", 0.0, None, None).await?;
    }
    if job.status == "queued" {
        job = patch_import_job(client, &job.id, "validating", 0.15, None, None).await?;
    }
    if job.status == "validating" {
        job = patch_import_job(client, &job.id, "parsing", 0.5, None, None).await?;
    }
    if matches!(job.status.as_str(), "parsing" | "preview_ready") {
        Ok(job)
    } else {
        Err(BackendClientError::Backend {
            status: StatusCode::CONFLICT,
            code: "invalid_import_job_state".to_string(),
            message: format!("import job cannot be committed from {}", job.status),
        })
    }
}

async fn begin_import_job_commit(
    client: &BackendClient,
    job: MaterialImportJob,
    state: &MaterialImportCommitState,
) -> Result<MaterialImportJob, BackendClientError> {
    if job.status == "committing" {
        let recorded = material_import_commit_state_from_metadata(&job.metadata);
        if recorded == *state {
            return Ok(job);
        }
        if recorded.commit_kind.is_none() || recorded.target_material_id.is_none() {
            return client
                .patch_material_import_job_metadata(
                    &job.id,
                    material_import_metadata_with_commit_state(job.metadata.clone(), state),
                )
                .await;
        }
        return Err(BackendClientError::Backend {
            status: StatusCode::CONFLICT,
            code: "import_commit_intent_conflict".to_string(),
            message: "material import already has a different commit intent".to_string(),
        });
    }
    let mut job = advance_import_job_to_parsing(client, job).await?;
    if job.status == "parsing" {
        job = patch_import_job(client, &job.id, "preview_ready", 0.75, None, None).await?;
    }
    client
        .patch_material_import_job(
            &job.id,
            &PatchMaterialImportJobRequest {
                status: Some("committing".to_string()),
                progress: Some(0.9),
                metadata: Some(material_import_metadata_with_commit_state(
                    job.metadata.clone(),
                    state,
                )),
                ..Default::default()
            },
        )
        .await
}

async fn recover_completed_import_side_effect(
    client: &BackendClient,
    job: MaterialImportJob,
) -> Result<Option<Article>, String> {
    let mut state = material_import_commit_state_from_metadata(&job.metadata);
    let (material_id, side_effect_is_recorded) =
        match material_import_commit_recovery_strategy(&state) {
            MaterialImportRecoveryStrategy::SettleRecordedMaterial(material_id)
            | MaterialImportRecoveryStrategy::SettleOpenExisting(material_id) => {
                (material_id, true)
            }
            MaterialImportRecoveryStrategy::VerifyTargetMaterialImportMarker(material_id) => {
                (material_id, false)
            }
            MaterialImportRecoveryStrategy::NoRecordedSideEffect => {
                let Some(material_id) = job.result_material_id.clone() else {
                    return Ok(None);
                };
                (material_id, true)
            }
        };
    let article = match client.get_material(&material_id).await {
        Ok(article) => article,
        Err(BackendClientError::Backend { status, .. }) if status == StatusCode::NOT_FOUND => {
            return Ok(None);
        }
        Err(error) => return Err(backend_error_to_string(error)),
    };
    if !side_effect_is_recorded && !material_was_committed_by_job(&article, &job.id) {
        return Ok(None);
    }
    if job.status != "succeeded" {
        if state.commit_kind.is_none() || state.target_material_id.is_none() {
            state = import_commit_state(None, "create", article.id.clone());
        }
        state.result_material_id = Some(article.id.clone());
        let committing = begin_import_job_commit(client, job, &state)
            .await
            .map_err(backend_error_to_string)?;
        finish_import_job(client, &committing.id, &article.id)
            .await
            .map_err(backend_error_to_string)?;
    }
    Ok(Some(article))
}

async fn resolve_duplicate_without_commit(
    client: &BackendClient,
    mut job: MaterialImportJob,
    duplicate: &crate::types::DuplicateMatch,
    policy: ImportDuplicatePolicy,
) -> Result<Option<Article>, String> {
    match policy {
        ImportDuplicatePolicy::Cancel => {
            client
                .cancel_material_import_job(&job.id)
                .await
                .map_err(backend_error_to_string)?;
            Err("Import cancelled because a duplicate material exists".to_string())
        }
        ImportDuplicatePolicy::OpenExisting => {
            let mut commit =
                import_commit_state(Some(policy), "open_existing", duplicate.material_id.clone());
            job = begin_import_job_commit(client, job, &commit)
                .await
                .map_err(backend_error_to_string)?;
            let article = client
                .get_material(&duplicate.material_id)
                .await
                .map_err(backend_error_to_string)?;
            commit.result_material_id = Some(article.id.clone());
            let _ = persist_import_job_commit_state(client, &job, &commit)
                .await
                .map_err(backend_error_to_string)?;
            finish_import_job(client, &job.id, &article.id)
                .await
                .map_err(backend_error_to_string)?;
            Ok(Some(article))
        }
        ImportDuplicatePolicy::Replace | ImportDuplicatePolicy::KeepCopy => Ok(None),
    }
}

async fn run_material_import<F, Fut>(
    app_handle: &AppHandle,
    source: MaterialImportSource,
    options: ImportJobOptions,
    prepare: F,
) -> Result<Article, String>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<PreparedMaterialImport, String>>,
{
    let client = backend_client_for_app(app_handle)?;
    let is_replay = options.import_job_id.is_some();
    let (source_content_hash, source_file_hash, input_hash) = import_source_hashes(&source)?;
    if let Some(file_id) = source.file_id.as_deref() {
        validate_uuid(file_id, "file_id")?;
    }

    let mut job = if let Some(job_id) = options.import_job_id.as_deref() {
        validate_uuid(job_id, "import_job_id")?;
        client
            .get_material_import_job(job_id)
            .await
            .map_err(backend_error_to_string)?
    } else {
        client
            .create_material_import_job(&CreateMaterialImportJobRequest {
                id: None,
                source_kind: source.source_kind.clone(),
                source_uri: source.source_uri.clone(),
                file_id: source.file_id.clone(),
                input_hash: Some(input_hash.clone()),
                file_sha256: source_file_hash.clone(),
                content: source.content.clone(),
                content_sha256: source_content_hash.clone(),
                preview: serde_json::json!({}),
                metadata: import_job_metadata(&source),
            })
            .await
            .map_err(backend_error_to_string)?
    };

    if job.status == "succeeded" {
        let material_id = job
            .result_material_id
            .as_deref()
            .ok_or_else(|| "completed import job is missing result_material_id".to_string())?;
        return client
            .get_material(material_id)
            .await
            .map_err(backend_error_to_string);
    }
    validate_import_job_source(&job, &source, &input_hash)?;
    let effective_duplicate_policy = material_import_effective_duplicate_policy(
        options.duplicate_policy.as_deref(),
        material_import_recorded_duplicate_policy(&job.metadata).as_deref(),
    )?;
    let policy = parse_duplicate_policy(effective_duplicate_policy.as_deref())?;
    if let Some(policy) = policy {
        let mut commit = material_import_commit_state_from_metadata(&job.metadata);
        if commit.duplicate_policy.as_deref() != Some(policy.as_str()) {
            commit.duplicate_policy = Some(policy.as_str().to_string());
            job = persist_import_job_commit_state(&client, &job, &commit)
                .await
                .map_err(backend_error_to_string)?;
        }
    }
    if let Some(article) = recover_completed_import_side_effect(&client, job.clone()).await? {
        return Ok(article);
    }

    if let Ok(existing) = client.get_material(&job.id).await {
        if material_was_committed_by_job(&existing, &job.id) {
            let mut commit = import_commit_state(policy, "create", existing.id.clone());
            commit.result_material_id = Some(existing.id.clone());
            let committing = begin_import_job_commit(&client, job, &commit)
                .await
                .map_err(backend_error_to_string)?;
            finish_import_job(&client, &committing.id, &existing.id)
                .await
                .map_err(backend_error_to_string)?;
            return Ok(existing);
        }
    }

    let initial_duplicates = client
        .check_material_duplicates(&duplicate_check_for_source(
            &source,
            source.content.clone(),
            source_content_hash,
            source_file_hash.clone(),
        ))
        .await
        .map_err(backend_error_to_string)?;
    if let Some(duplicate) = initial_duplicates.matches.first() {
        let Some(policy) = policy else {
            mark_import_job_failed(
                &client,
                &job.id,
                "duplicate_policy_required",
                "duplicate material detected; choose cancel, open_existing, replace, or keep_copy",
                is_replay,
            )
            .await;
            return Err(format!(
                "Duplicate material detected: {}. duplicate_policy is required",
                duplicate.material_id
            ));
        };
        if let Some(article) =
            resolve_duplicate_without_commit(&client, job.clone(), duplicate, policy).await?
        {
            return Ok(article);
        }
    } else if policy == Some(ImportDuplicatePolicy::Cancel) {
        client
            .cancel_material_import_job(&job.id)
            .await
            .map_err(backend_error_to_string)?;
        return Err("Import cancelled".to_string());
    } else if policy == Some(ImportDuplicatePolicy::OpenExisting) {
        mark_import_job_failed(
            &client,
            &job.id,
            "duplicate_not_found",
            "open_existing requires a duplicate material",
            is_replay,
        )
        .await;
        return Err("open_existing requires a duplicate material".to_string());
    }

    job = advance_import_job_to_parsing(&client, job)
        .await
        .map_err(backend_error_to_string)?;

    let mut prepared = match prepare().await {
        Ok(prepared) => prepared,
        Err(message) => {
            mark_import_job_failed(
                &client,
                &job.id,
                "import_prepare_failed",
                &message,
                is_replay || retryable_import_message(&message),
            )
            .await;
            return Err(message);
        }
    };

    let final_file_hash = prepared.file_sha256.or(source_file_hash);
    let final_content_hash = content_sha256(&prepared.article.content);
    let final_duplicates = match client
        .check_material_duplicates(&duplicate_check_for_source(
            &source,
            Some(prepared.article.content.clone()),
            final_content_hash,
            final_file_hash.clone(),
        ))
        .await
    {
        Ok(duplicates) => duplicates,
        Err(error) => {
            let message = backend_error_to_string(error);
            mark_import_job_failed(
                &client,
                &job.id,
                "duplicate_check_failed",
                &message,
                is_replay || retryable_import_message(&message),
            )
            .await;
            return Err(message);
        }
    };
    let duplicate = final_duplicates.matches.first();
    if duplicate.is_some() && policy.is_none() {
        mark_import_job_failed(
            &client,
            &job.id,
            "duplicate_policy_required",
            "duplicate material detected after parsing",
            is_replay,
        )
        .await;
        return Err(
            "Duplicate material detected after parsing; duplicate_policy is required".to_string(),
        );
    }
    if let (Some(duplicate), Some(policy)) = (duplicate, policy) {
        if let Some(article) =
            resolve_duplicate_without_commit(&client, job.clone(), duplicate, policy).await?
        {
            return Ok(article);
        }
    }

    let (commit_kind, target_material_id) =
        if let (Some(duplicate), Some(ImportDuplicatePolicy::Replace)) = (duplicate, policy) {
            ("replace", duplicate.material_id.clone())
        } else {
            ("create", job.id.clone())
        };
    let mut commit = import_commit_state(policy, commit_kind, target_material_id);
    job = begin_import_job_commit(&client, job, &commit)
        .await
        .map_err(backend_error_to_string)?;

    let commit_result =
        if let (Some(duplicate), Some(ImportDuplicatePolicy::Replace)) = (duplicate, policy) {
            match client.get_material(&duplicate.material_id).await {
                Ok(existing) => {
                    prepared.article.id = duplicate.material_id.clone();
                    prepared.article.metadata = metadata_for_replaced_material(
                        &existing.metadata,
                        &prepared.article.metadata,
                        prepared.metadata.take(),
                        &job.id,
                    );
                    for segment in &mut prepared.article.segments {
                        segment.article_id = duplicate.material_id.clone();
                    }
                    client
                        .patch_material_replacing_source_fields_with_file_hash(
                            &duplicate.material_id,
                            &patch_material_payload_from_article(&prepared.article),
                            final_file_hash.as_deref(),
                        )
                        .await
                }
                Err(error) => Err(error),
            }
        } else {
            prepared.article.id = job.id.clone();
            for segment in &mut prepared.article.segments {
                segment.article_id = job.id.clone();
            }
            let metadata = metadata_for_import_commit(prepared.metadata.take(), &job.id);
            client
                .create_material_with_options(
                    &create_material_payload_from_article(&prepared.article, Some(metadata)),
                    final_file_hash.as_deref(),
                    (policy == Some(ImportDuplicatePolicy::KeepCopy)).then_some("keep_copy"),
                )
                .await
        };

    let article = match commit_result {
        Ok(article) => article,
        Err(error) => {
            if let Some(existing) =
                recover_completed_import_side_effect(&client, job.clone()).await?
            {
                return Ok(existing);
            }
            let retryable = is_replay || retryable_backend_error(&error);
            let message = error.to_string();
            mark_import_job_failed(
                &client,
                &job.id,
                "import_commit_failed",
                &message,
                retryable,
            )
            .await;
            return Err(backend_error_to_string(error));
        }
    };
    commit.result_material_id = Some(article.id.clone());
    let _ = persist_import_job_commit_state(&client, &job, &commit)
        .await
        .map_err(backend_error_to_string)?;
    finish_import_job(&client, &job.id, &article.id)
        .await
        .map_err(backend_error_to_string)?;
    Ok(article)
}

async fn persist_agent_task_backend(
    app_handle: &AppHandle,
    task: &AgentTask,
) -> Result<AgentTask, String> {
    backend_client_for_app(app_handle)?
        .save_agent_task(task)
        .await
        .map_err(backend_error_to_string)
}

async fn persist_artifact_backend(
    app_handle: &AppHandle,
    artifact: &Artifact,
) -> Result<Artifact, String> {
    backend_client_for_app(app_handle)?
        .save_artifact(artifact)
        .await
        .map_err(backend_error_to_string)
}

async fn ensure_backend_task_references_artifact(
    app_handle: &AppHandle,
    task_id: &str,
    article_id: &str,
    artifact_id: &str,
) -> Result<(), String> {
    let client = backend_client_for_app(app_handle)?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut task = match client.get_agent_task(task_id).await {
        Ok(task) => task,
        Err(BackendClientError::Backend { status, .. }) if status == StatusCode::NOT_FOUND => {
            AgentTask {
                id: task_id.to_string(),
                task_type: AgentTaskType::MindMapGenerate,
                status: AgentTaskStatus::Succeeded,
                article_id: article_id.to_string(),
                input: AgentTaskInput {
                    article_id: article_id.to_string(),
                    display_language: "zh-CN".to_string(),
                    max_depth: 0,
                    evidence_mode: "manual".to_string(),
                    prefer_structure: "manual".to_string(),
                },
                progress: 1.0,
                stage: Some("manual_artifact".to_string()),
                message: Some("Manual mind map artifact saved".to_string()),
                error: None,
                worker_session_id: None,
                artifact_ids: Vec::new(),
                created_at: now.clone(),
                updated_at: now.clone(),
                started_at: Some(now.clone()),
                finished_at: Some(now.clone()),
            }
        }
        Err(error) => return Err(backend_error_to_string(error)),
    };

    if task.article_id != article_id {
        return Err("Artifact task does not belong to the target article".to_string());
    }
    if !task.artifact_ids.iter().any(|id| id == artifact_id) {
        task.artifact_ids.push(artifact_id.to_string());
    }
    task.updated_at = now.clone();
    if matches!(
        task.status,
        AgentTaskStatus::Queued | AgentTaskStatus::Running
    ) {
        task.status = AgentTaskStatus::Succeeded;
        task.progress = 1.0;
        task.stage = Some("manual_artifact".to_string());
        task.finished_at = Some(now);
    }

    client
        .save_agent_task(&task)
        .await
        .map(|_| ())
        .map_err(backend_error_to_string)
}

fn create_material_payload_from_article(
    article: &Article,
    metadata: Option<serde_json::Value>,
) -> CreateMaterialRequest {
    CreateMaterialRequest {
        id: Some(article.id.clone()),
        title: article.title.clone(),
        content: article.content.clone(),
        source_type: article.source_type.clone(),
        source_url: article.source_url.clone(),
        media_path: article.media_path.clone(),
        book_path: article.book_path.clone(),
        book_type: article.book_type.clone(),
        translated: Some(article.translated),
        active_mind_map_artifact_id: article.active_mind_map_artifact_id.clone(),
        metadata,
        segments: Some(article.segments.clone()),
    }
}

fn patch_material_payload_from_article(article: &Article) -> PatchMaterialRequest {
    PatchMaterialRequest {
        title: Some(article.title.clone()),
        content: Some(article.content.clone()),
        source_type: article.source_type.clone(),
        source_url: article.source_url.clone(),
        media_path: article.media_path.clone(),
        book_path: article.book_path.clone(),
        book_type: article.book_type.clone(),
        translated: Some(article.translated),
        active_mind_map_artifact_id: article.active_mind_map_artifact_id.clone(),
        metadata: Some(article.metadata.clone()),
        segments: Some(article.segments.clone()),
    }
}

async fn replace_backend_article(
    app_handle: &AppHandle,
    article: &Article,
) -> Result<Article, String> {
    let client = backend_client_for_app(app_handle)?;
    client
        .patch_material(&article.id, &patch_material_payload_from_article(article))
        .await
        .map_err(backend_error_to_string)
}

// Helper function to create segments from content
// 按句子分隔内容（使用.或。作为分隔符），并标记是否需要换行
fn create_segments_from_content(article_id: &str, content: &str) -> Vec<ArticleSegment> {
    let mut segments = Vec::new();
    let mut order = 0;

    // 首先按段落分割（双换行或单换行）
    let paragraphs: Vec<&str> = content
        .split('\n')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    for paragraph in paragraphs {
        // 将段落按句子分割（使用 . 或 。 作为分隔符）
        // 使用正则表达式保留分隔符
        let sentences = split_into_sentences(paragraph);

        for (sentence_index, sentence) in sentences.iter().enumerate() {
            let text = sentence.trim();
            if text.is_empty() {
                continue;
            }

            segments.push(ArticleSegment {
                id: Uuid::new_v4().to_string(),
                article_id: article_id.to_string(),
                order,
                text: text.to_string(),
                reading_text: None,
                translation: None,
                explanation: None,
                start_time: None,
                end_time: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                // 段落的第一个句子需要换行显示，后续句子紧跟前一个显示
                is_new_paragraph: sentence_index == 0,
            });
            order += 1;
        }
    }

    segments
}

/// 将段落拆分成句子，保留句末标点
/// 支持英文句号(.)、中文句号(。)、问号(?/？)、感叹号(!/！)
fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = text.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        current.push(c);

        // 检查是否是句子结束符
        let is_sentence_end = c == '。'
            || c == '？'
            || c == '！'
            || (c == '.' && !is_abbreviation(&chars, i))
            || c == '?'
            || c == '!';

        if is_sentence_end {
            // 处理引号闭合情况：如 ... said." 这种情况
            // 向后看，如果下一个字符是引号，把它也加进来
            if i + 1 < chars.len() {
                let next = chars[i + 1];
                if next == '"'
                    || next == '"'
                    || next == '\''
                    || next == '\u{2019}'
                    || next == ')'
                    || next == '）'
                {
                    i += 1;
                    current.push(next);
                }
            }

            let trimmed = current.trim().to_string();
            if !trimmed.is_empty() {
                sentences.push(trimmed);
            }
            current = String::new();
        }

        i += 1;
    }

    // 处理剩余内容（没有句号结尾的情况）
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        sentences.push(trimmed);
    }

    // 如果整个段落没有分割成功（没有找到分隔符），返回整段
    if sentences.is_empty() && !text.trim().is_empty() {
        sentences.push(text.trim().to_string());
    }

    sentences
}

/// 检查句点是否是缩写的一部分（如 Mr. Mrs. Dr. U.S. 等）
/// 简单的启发式规则
fn is_abbreviation(chars: &[char], pos: usize) -> bool {
    // 如果句点后面紧跟字母，可能是缩写 (如 U.S.A)
    if pos + 1 < chars.len() && chars[pos + 1].is_alphabetic() {
        return true;
    }

    // 检查句点前是否是常见缩写
    // 向前查找单词
    let mut word = String::new();
    let mut j = pos as i32 - 1;
    while j >= 0 && chars[j as usize].is_alphabetic() {
        word.insert(0, chars[j as usize]);
        j -= 1;
    }

    let word_lower = word.to_lowercase();
    let abbreviations = [
        "mr", "mrs", "ms", "dr", "jr", "sr", "vs", "etc", "inc", "ltd", "no", "st", "ave", "rd",
    ];

    if abbreviations.contains(&word_lower.as_str()) {
        return true;
    }

    // 单字母后跟句点通常是缩写（如 A. B. C.）
    if word.len() == 1 && word.chars().next().unwrap().is_uppercase() {
        return true;
    }

    false
}

pub fn build_article_overview(article: &Article) -> ArticleOverview {
    ArticleOverview {
        article_id: article.id.clone(),
        title: article.title.clone(),
        source_type: article.source_type.clone(),
        content_length: article.content.chars().count(),
        segment_count: article.segments.len(),
        has_timestamps: article
            .segments
            .iter()
            .any(|segment| segment.start_time.is_some() || segment.end_time.is_some()),
        has_segments: !article.segments.is_empty(),
        language_hint: None,
        book_type: article.book_type.clone(),
    }
}

pub fn material_summary_from_article(article: &Article) -> MaterialSummary {
    let material_type = article
        .book_type
        .clone()
        .or_else(|| match article.source_type.as_deref() {
            Some("youtube") | Some("local_video") => Some("video".to_string()),
            Some(source_type) => Some(source_type.to_string()),
            None => None,
        })
        .unwrap_or_else(|| "article".to_string());

    MaterialSummary {
        id: article.id.clone(),
        title: article.title.clone(),
        material_type,
        created_at: article.created_at.clone(),
        translated: article.translated,
    }
}

pub fn filter_material_summaries(
    items: &[MaterialSummary],
    keyword: Option<&str>,
    material_type: Option<&str>,
    limit: usize,
) -> Vec<MaterialSummary> {
    let normalized_keyword = keyword
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let normalized_type = material_type
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());

    items
        .iter()
        .filter(|item| {
            normalized_keyword
                .as_ref()
                .map(|keyword| item.title.to_lowercase().contains(keyword))
                .unwrap_or(true)
        })
        .filter(|item| {
            normalized_type
                .as_ref()
                .map(|material_type| item.material_type.to_lowercase() == *material_type)
                .unwrap_or(true)
        })
        .take(limit)
        .cloned()
        .collect()
}

fn builtin_agent_turn_payload(
    user_message: &str,
    current_material: &MaterialSummary,
    available_materials: &[MaterialSummary],
) -> Option<(serde_json::Value, Option<String>)> {
    let normalized = user_message.trim().to_lowercase();
    if normalized.is_empty() {
        return None;
    }

    if matches!(
        normalized.as_str(),
        "查看当前素材" | "current material" | "view current material" | "show current material"
    ) {
        let reply = format!(
            "当前素材：{}\n\n- ID: {}\n- 类型: {}\n- 创建时间: {}\n- 翻译状态: {}",
            current_material.title,
            current_material.id,
            current_material.material_type,
            current_material.created_at,
            if current_material.translated {
                "已翻译"
            } else {
                "未完整翻译"
            }
        );
        return Some((
            serde_json::json!({
                "reply": reply,
                "action": { "kind": "get_current_material" }
            }),
            None,
        ));
    }

    if matches!(
        normalized.as_str(),
        "列出素材" | "list materials" | "show materials" | "list material"
    ) {
        let mut lines = vec!["当前素材列表：".to_string()];
        for (index, material) in available_materials.iter().take(20).enumerate() {
            lines.push(format!(
                "{}. {} ({}, {})",
                index + 1,
                material.title,
                material.material_type,
                material.id
            ));
        }
        if available_materials.len() > 20 {
            lines.push(format!(
                "其余 {} 个素材未显示。",
                available_materials.len() - 20
            ));
        }
        return Some((
            serde_json::json!({
                "reply": lines.join("\n"),
                "action": { "kind": "list_materials" }
            }),
            None,
        ));
    }

    let open_prefixes = ["打开素材", "open material"];
    let open_target = open_prefixes
        .iter()
        .find_map(|prefix| normalized.strip_prefix(prefix).map(str::trim));
    if let Some(target) = open_target {
        if target.is_empty() {
            return Some((
                serde_json::json!({
                    "reply": "请提供要打开的素材标题或 ID。你也可以先点击“列出素材”查看可用素材。",
                    "action": null
                }),
                None,
            ));
        }

        let matched = available_materials.iter().find(|material| {
            material.id.to_lowercase() == target || material.title.to_lowercase().contains(target)
        });
        if let Some(material) = matched {
            return Some((
                serde_json::json!({
                    "reply": format!("正在打开素材：{}", material.title),
                    "action": {
                        "kind": "open_material",
                        "material_id": material.id
                    }
                }),
                Some(material.id.clone()),
            ));
        }

        return Some((
            serde_json::json!({
                "reply": format!("没有找到匹配“{}”的素材。请检查标题或 ID。", target),
                "action": null
            }),
            None,
        ));
    }

    None
}

async fn complete_builtin_agent_turn(
    app_handle: &AppHandle,
    mut task: AgentTask,
    payload: serde_json::Value,
    open_material_id: Option<String>,
) -> Result<AgentTask, String> {
    let now = chrono::Utc::now().to_rfc3339();
    task.status = AgentTaskStatus::Succeeded;
    task.progress = 1.0;
    task.stage = Some("done".to_string());
    task.message = Some("Agent turn handled locally".to_string());
    task.error = None;
    task.updated_at = now.clone();
    task.started_at = Some(task.started_at.unwrap_or_else(|| now.clone()));
    task.finished_at = Some(now);
    let task = persist_agent_task_backend(app_handle, &task).await?;

    let _ = app_handle.emit(&format!("assistant-agent-progress://{}", task.id), &task);
    let _ = app_handle.emit(&format!("assistant-agent-result://{}", task.id), &payload);
    let _ = app_handle.emit("agent-task-updated", &task);
    if let Some(material_id) = open_material_id {
        let _ = app_handle.emit(
            "agent://open-material",
            serde_json::json!({ "materialId": material_id }),
        );
    }

    Ok(task)
}

pub fn read_article_window(
    article: &Article,
    cursor: usize,
    max_chars: usize,
) -> ArticleTextWindow {
    let total_chars = article.content.chars().count();
    let safe_cursor = cursor.min(total_chars);
    let requested_end = (safe_cursor + max_chars).min(total_chars);
    let text: String = article
        .content
        .chars()
        .skip(safe_cursor)
        .take(requested_end.saturating_sub(safe_cursor))
        .collect();

    let mut source_segment_ids = Vec::new();
    let mut min_start: Option<f64> = None;
    let mut max_end: Option<f64> = None;
    let mut segment_cursor = 0usize;

    for (index, segment) in article.segments.iter().enumerate() {
        if index > 0 {
            segment_cursor += 1;
        }
        let segment_start = segment_cursor;
        let segment_end = segment_start + segment.text.chars().count();
        segment_cursor = segment_end;

        let overlaps = segment_end > safe_cursor && segment_start < requested_end;
        if overlaps {
            source_segment_ids.push(segment.id.clone());
            if let Some(start) = segment.start_time {
                min_start = Some(min_start.map_or(start, |current| current.min(start)));
            }
            if let Some(end) = segment.end_time {
                max_end = Some(max_end.map_or(end, |current| current.max(end)));
            }
        }
    }

    ArticleTextWindow {
        cursor: safe_cursor,
        next_cursor: requested_end,
        has_more: requested_end < total_chars,
        text,
        start_offset: safe_cursor,
        end_offset: requested_end,
        source_segment_ids,
        time_range: match (min_start, max_end) {
            (Some(start), Some(end)) => Some(TimeRange { start, end }),
            _ => None,
        },
    }
}

pub fn search_article_segments(
    article: &Article,
    query: &str,
    limit: usize,
) -> ArticleSearchResult {
    let normalized_query = query.trim().to_lowercase();
    if normalized_query.is_empty() {
        return ArticleSearchResult {
            results: Vec::new(),
        };
    }

    let mut results = Vec::new();
    for segment in &article.segments {
        let text_lower = segment.text.to_lowercase();
        if text_lower.contains(&normalized_query) {
            let score = normalized_query.len() as f64 / segment.text.len().max(1) as f64;
            results.push(ArticleSearchHit {
                segment_id: segment.id.clone(),
                text: segment.text.clone(),
                score,
                start_time: segment.start_time,
                end_time: segment.end_time,
            });
        }
        if results.len() >= limit {
            break;
        }
    }

    ArticleSearchResult { results }
}

pub fn collect_article_evidence(
    article: &Article,
    segment_ids: &[String],
) -> ArticleEvidenceResult {
    let index: HashMap<&str, &ArticleSegment> = article
        .segments
        .iter()
        .map(|segment| (segment.id.as_str(), segment))
        .collect();

    let items = segment_ids
        .iter()
        .filter_map(|segment_id| index.get(segment_id.as_str()))
        .map(|segment| ArticleEvidenceItem {
            segment_id: segment.id.clone(),
            text: segment.text.clone(),
            start_time: segment.start_time,
            end_time: segment.end_time,
        })
        .collect();

    ArticleEvidenceResult { items }
}

pub fn update_legacy_agent_task_progress_in_dir(
    data_dir: &std::path::Path,
    task_id: &str,
    stage: String,
    progress: f64,
    message: Option<String>,
) -> Result<AgentTask, String> {
    let mut task = crate::storage::load_legacy_agent_task_in_dir(data_dir, task_id)?;
    task.status = AgentTaskStatus::Running;
    task.stage = Some(stage);
    task.progress = progress.clamp(0.0, 1.0);
    task.message = message;
    task.updated_at = chrono::Utc::now().to_rfc3339();
    if task.started_at.is_none() {
        task.started_at = Some(task.updated_at.clone());
    }
    crate::storage::save_legacy_agent_task_in_dir(data_dir, &task)?;
    Ok(task)
}

pub fn save_legacy_mind_map_artifact_in_dir(
    data_dir: &std::path::Path,
    task_id: &str,
    article_id: &str,
    content: serde_json::Value,
) -> Result<Artifact, String> {
    let artifact = Artifact {
        id: Uuid::new_v4().to_string(),
        task_id: task_id.to_string(),
        article_id: article_id.to_string(),
        artifact_type: ArtifactType::MindMap,
        version: "1".to_string(),
        content,
        metadata: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: chrono::Utc::now().to_rfc3339(),
    };
    crate::storage::save_legacy_artifact_in_dir(data_dir, &artifact)?;
    Ok(artifact)
}

#[tauri::command]
pub async fn article_get_overview_cmd(
    app_handle: AppHandle,
    article_id: String,
) -> Result<ArticleOverview, String> {
    let article = get_article(app_handle, article_id).await?;
    Ok(build_article_overview(&article))
}

#[tauri::command]
pub async fn article_read_window_cmd(
    app_handle: AppHandle,
    article_id: String,
    cursor: usize,
    max_chars: usize,
) -> Result<ArticleTextWindow, String> {
    let article = get_article(app_handle, article_id).await?;
    Ok(read_article_window(&article, cursor, max_chars))
}

#[tauri::command]
pub async fn article_search_cmd(
    app_handle: AppHandle,
    article_id: String,
    query: String,
    limit: Option<usize>,
) -> Result<ArticleSearchResult, String> {
    let article = get_article(app_handle, article_id).await?;
    Ok(search_article_segments(
        &article,
        &query,
        limit.unwrap_or(8),
    ))
}

#[tauri::command]
pub async fn article_get_evidence_cmd(
    app_handle: AppHandle,
    article_id: String,
    segment_ids: Vec<String>,
) -> Result<ArticleEvidenceResult, String> {
    let article = get_article(app_handle, article_id).await?;
    Ok(collect_article_evidence(&article, &segment_ids))
}

#[tauri::command]
pub async fn task_report_progress_cmd(
    app_handle: AppHandle,
    task_id: String,
    stage: String,
    progress: f64,
    message: Option<String>,
) -> Result<AgentTask, String> {
    let client = backend_client_for_app(&app_handle)?;
    let mut task = client
        .get_agent_task(&task_id)
        .await
        .map_err(backend_error_to_string)?;
    if matches!(
        task.status,
        AgentTaskStatus::Succeeded
            | AgentTaskStatus::Failed
            | AgentTaskStatus::Cancelled
            | AgentTaskStatus::Interrupted
    ) {
        return Ok(task);
    }
    task.status = AgentTaskStatus::Running;
    task.stage = Some(stage);
    task.progress = progress.clamp(0.0, 1.0);
    task.message = message;
    task.updated_at = chrono::Utc::now().to_rfc3339();
    if task.started_at.is_none() {
        task.started_at = Some(task.updated_at.clone());
    }
    client
        .save_agent_task(&task)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn artifact_save_cmd(
    app_handle: AppHandle,
    task_id: String,
    article_id: String,
    content: serde_json::Value,
) -> Result<Artifact, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let artifact = Artifact {
        id: Uuid::new_v4().to_string(),
        task_id,
        article_id: article_id.clone(),
        artifact_type: ArtifactType::MindMap,
        version: "1".to_string(),
        content,
        metadata: None,
        created_at: now.clone(),
        updated_at: now,
    };
    let artifact = persist_artifact_backend(&app_handle, &artifact).await?;
    ensure_backend_task_references_artifact(
        &app_handle,
        &artifact.task_id,
        &article_id,
        &artifact.id,
    )
    .await?;
    let mut article = get_article(app_handle.clone(), article_id.clone()).await?;
    article.active_mind_map_artifact_id = Some(artifact.id.clone());
    let _ = replace_backend_article(&app_handle, &article).await?;
    Ok(artifact)
}

#[tauri::command]
pub async fn create_mind_map_task_cmd(
    app_handle: AppHandle,
    worker_manager: State<'_, AgentWorkerManager>,
    article_id: String,
    display_language: Option<String>,
    max_depth: Option<i32>,
) -> Result<AgentTask, String> {
    let article = get_article(app_handle.clone(), article_id.clone()).await?;
    let active_model = require_active_agent_model_config(load_config(&app_handle)?)?;
    let provider_config = resolve_runtime_provider_config(&active_model);
    let now = chrono::Utc::now().to_rfc3339();
    let task = AgentTask {
        id: Uuid::new_v4().to_string(),
        task_type: AgentTaskType::MindMapGenerate,
        status: AgentTaskStatus::Queued,
        article_id: article_id.clone(),
        input: AgentTaskInput {
            article_id: article_id.clone(),
            display_language: display_language.unwrap_or_else(|| "zh-CN".to_string()),
            max_depth: max_depth.unwrap_or(3),
            evidence_mode: "strict".to_string(),
            prefer_structure: "topic_tree".to_string(),
        },
        progress: 0.0,
        stage: Some("queued".to_string()),
        message: None,
        error: None,
        worker_session_id: None,
        artifact_ids: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
        started_at: None,
        finished_at: None,
    };
    let task = persist_agent_task_backend(&app_handle, &task).await?;
    if let Err(error) =
        worker_manager.submit_mind_map_task(&app_handle, &task, &article, &provider_config)
    {
        let mut failed_task = task.clone();
        failed_task.status = AgentTaskStatus::Failed;
        failed_task.error = Some(error.clone());
        failed_task.stage = Some("failed_to_start".to_string());
        failed_task.updated_at = chrono::Utc::now().to_rfc3339();
        failed_task.finished_at = Some(failed_task.updated_at.clone());
        persist_agent_task_backend(&app_handle, &failed_task).await?;
        return Err(error);
    }
    backend_client_for_app(&app_handle)?
        .get_agent_task(&task.id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn run_agent_turn_cmd(
    app_handle: AppHandle,
    worker_manager: State<'_, AgentWorkerManager>,
    task_id: String,
    article_id: String,
    user_message: String,
    conversation: Vec<AssistantConversationMessage>,
    display_language: Option<String>,
) -> Result<AgentTask, String> {
    let article = get_article(app_handle.clone(), article_id.clone()).await?;
    let articles = list_articles_cmd(app_handle.clone()).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let task = AgentTask {
        id: task_id,
        task_type: AgentTaskType::AssistantAgentTurn,
        status: AgentTaskStatus::Queued,
        article_id: article_id.clone(),
        input: AgentTaskInput {
            article_id: article_id.clone(),
            display_language: display_language.unwrap_or_else(|| "zh-CN".to_string()),
            max_depth: 0,
            evidence_mode: "none".to_string(),
            prefer_structure: "none".to_string(),
        },
        progress: 0.0,
        stage: Some("queued".to_string()),
        message: None,
        error: None,
        worker_session_id: None,
        artifact_ids: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
        started_at: None,
        finished_at: None,
    };
    let task = persist_agent_task_backend(&app_handle, &task).await?;

    let current_material = material_summary_from_article(&article);
    let available_materials = articles
        .iter()
        .map(material_summary_from_article)
        .collect::<Vec<_>>();

    if let Some((payload, open_material_id)) =
        builtin_agent_turn_payload(&user_message, &current_material, &available_materials)
    {
        return complete_builtin_agent_turn(&app_handle, task, payload, open_material_id).await;
    }

    let active_model = require_active_agent_model_config(load_config(&app_handle)?)?;
    let provider_config = resolve_runtime_provider_config(&active_model);

    if let Err(error) = worker_manager.submit_assistant_turn(
        &app_handle,
        &task,
        user_message,
        conversation,
        current_material,
        available_materials,
        &provider_config,
    ) {
        let mut failed_task = task.clone();
        failed_task.status = AgentTaskStatus::Failed;
        failed_task.error = Some(error.clone());
        failed_task.stage = Some("failed_to_start".to_string());
        failed_task.updated_at = chrono::Utc::now().to_rfc3339();
        failed_task.finished_at = Some(failed_task.updated_at.clone());
        persist_agent_task_backend(&app_handle, &failed_task).await?;
        return Err(error);
    }

    backend_client_for_app(&app_handle)?
        .get_agent_task(&task.id)
        .await
        .map_err(backend_error_to_string)
}

pub fn require_active_agent_model_config(
    config: Option<crate::types::AppConfig>,
) -> Result<ModelConfig, String> {
    let config = config.ok_or_else(|| "未配置 API，请先在设置中配置 AI 模型".to_string())?;
    config
        .get_active_config()
        .cloned()
        .ok_or_else(|| "未设置活动模型配置，请先在设置中配置 AI 模型".to_string())
}

#[tauri::command]
pub async fn get_agent_task_cmd(
    app_handle: AppHandle,
    task_id: String,
) -> Result<AgentTask, String> {
    backend_client_for_app(&app_handle)?
        .get_agent_task(&task_id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn get_artifact_cmd(
    app_handle: AppHandle,
    article_id: String,
    artifact_id: String,
) -> Result<Artifact, String> {
    backend_client_for_app(&app_handle)?
        .get_artifact(&article_id, &artifact_id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn get_agent_worker_status_cmd(
    worker_manager: State<'_, AgentWorkerManager>,
) -> Result<AgentWorkerStatusSnapshot, String> {
    Ok(worker_manager.status_snapshot())
}

#[tauri::command]
pub async fn stop_agent_worker_cmd(
    worker_manager: State<'_, AgentWorkerManager>,
) -> Result<(), String> {
    worker_manager.stop()
}

const DEFAULT_UNGROUPED_PACK_ID: &str = "system-ungrouped";
const DEFAULT_UNGROUPED_PACK_NAME: &str = "未分组";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WordPackExportMeta {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    cover_url: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    language_from: Option<String>,
    #[serde(default)]
    language_to: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WordPackExportEntry {
    word: String,
    meaning: String,
    #[serde(default)]
    usage: Option<String>,
    #[serde(default)]
    example: Option<String>,
    #[serde(default)]
    reading: Option<String>,
    #[serde(default)]
    explanation: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WordPackExportFile {
    schema_version: String,
    pack: WordPackExportMeta,
    entries: Vec<WordPackExportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportWordPackResult {
    pub file_name: String,
    pub json_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportWordPackResult {
    pub created_pack_id: String,
    pub total: usize,
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

fn favorite_to_word_pack_export_entry(fav: FavoriteVocabulary) -> WordPackExportEntry {
    WordPackExportEntry {
        word: fav.word,
        meaning: fav.meaning,
        usage: if fav.usage.trim().is_empty() {
            None
        } else {
            Some(fav.usage)
        },
        example: fav.example,
        reading: fav.reading,
        explanation: fav.explanation,
        tags: Vec::new(),
    }
}

fn build_word_pack_export_result(
    pack_meta: WordPackExportMeta,
    mut entries: Vec<WordPackExportEntry>,
) -> Result<ExportWordPackResult, String> {
    entries.sort_by(|a, b| a.word.cmp(&b.word));

    let export_file = WordPackExportFile {
        schema_version: "openkoto-word-pack-v1".to_string(),
        pack: pack_meta.clone(),
        entries,
    };

    let json_content = serde_json::to_string_pretty(&export_file)
        .map_err(|e| format!("Failed to serialize export file: {}", e))?;
    let file_name = format!("{}.okpack.json", sanitize_file_name(&pack_meta.name));

    Ok(ExportWordPackResult {
        file_name,
        json_content,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrsUpdateResult {
    pub srs_state: String,
    pub repetitions: i32,
    pub interval_days: i32,
    pub ease_factor: f64,
    pub due_date: String,
}

fn normalize_word(word: &str) -> String {
    word.trim().to_lowercase()
}

fn parse_import_word_pack_json(json_content: &str) -> Result<WordPackExportFile, String> {
    let normalized = json_content.trim_start_matches('\u{feff}').trim();
    if normalized.is_empty() {
        return Err("Word pack JSON is empty".to_string());
    }

    let parsed = serde_json::from_str::<WordPackExportFile>(normalized)
        .map_err(|e| format!("Invalid word pack JSON: {}", e))?;

    if parsed.schema_version != "openkoto-word-pack-v1" {
        return Err(format!(
            "Unsupported word pack schema_version: {} (expected openkoto-word-pack-v1)",
            parsed.schema_version
        ));
    }

    Ok(parsed)
}

fn parse_local_date(date_local: &str) -> Result<chrono::NaiveDate, String> {
    chrono::NaiveDate::parse_from_str(date_local, "%Y-%m-%d")
        .map_err(|_| format!("Invalid local date format: {}", date_local))
}

fn today_local_date() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

fn ensure_default_word_pack(app_handle: &AppHandle) -> Result<WordPack, String> {
    ensure_favorites_dirs(app_handle)?;
    let now = chrono::Utc::now().to_rfc3339();
    let default_pack = WordPack {
        id: DEFAULT_UNGROUPED_PACK_ID.to_string(),
        name: DEFAULT_UNGROUPED_PACK_NAME.to_string(),
        description: Some("系统默认合集".to_string()),
        cover_url: None,
        author: Some("OpenKoto".to_string()),
        language_from: None,
        language_to: None,
        tags: vec!["system".to_string()],
        version: Some("1.0.0".to_string()),
        created_at: now.clone(),
        updated_at: now,
        is_system: true,
    };

    let existing = load_word_pack(app_handle, DEFAULT_UNGROUPED_PACK_ID)
        .ok()
        .and_then(|json| serde_json::from_str::<WordPack>(&json).ok());

    if let Some(pack) = existing {
        return Ok(pack);
    }

    let json = serde_json::to_string(&default_pack)
        .map_err(|e| format!("Failed to serialize default pack: {}", e))?;
    save_word_pack(app_handle, &default_pack.id, &json)?;
    Ok(default_pack)
}

fn persist_favorite_vocabulary(
    app_handle: &AppHandle,
    favorite: &FavoriteVocabulary,
) -> Result<(), String> {
    let json = serde_json::to_string(favorite)
        .map_err(|e| format!("Failed to serialize favorite vocabulary: {}", e))?;
    save_favorite_vocabulary(app_handle, &favorite.id, &json)
}

fn default_word_pack_from_packs(packs: &[WordPack]) -> WordPack {
    packs
        .iter()
        .find(|pack| pack.id == DEFAULT_UNGROUPED_PACK_ID)
        .cloned()
        .unwrap_or_else(|| {
            let now = chrono::Utc::now().to_rfc3339();
            WordPack {
                id: DEFAULT_UNGROUPED_PACK_ID.to_string(),
                name: DEFAULT_UNGROUPED_PACK_NAME.to_string(),
                description: Some("系统默认合集".to_string()),
                cover_url: None,
                author: Some("OpenKoto".to_string()),
                language_from: None,
                language_to: None,
                tags: vec!["system".to_string()],
                version: Some("1.0.0".to_string()),
                created_at: now.clone(),
                updated_at: now,
                is_system: true,
            }
        })
}

fn sanitize_pack_ids(pack_ids: Option<Vec<String>>) -> Vec<String> {
    let mut seen = HashSet::new();
    pack_ids
        .unwrap_or_default()
        .into_iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn filter_existing_pack_ids(
    pack_ids: Vec<String>,
    existing_pack_ids: &HashSet<String>,
    default_pack_id: &str,
) -> Vec<String> {
    let mut result: Vec<String> = pack_ids
        .into_iter()
        .filter(|id| existing_pack_ids.contains(id))
        .collect();

    if result.is_empty() {
        result.push(default_pack_id.to_string());
    }

    result
}

fn sort_by_due_then_last_review(
    a: &FavoriteVocabulary,
    b: &FavoriteVocabulary,
) -> std::cmp::Ordering {
    match a.due_date.cmp(&b.due_date) {
        std::cmp::Ordering::Equal => a.last_reviewed_at.cmp(&b.last_reviewed_at),
        ord => ord,
    }
}

fn is_due_on_or_before(due_date: &str, target_date: chrono::NaiveDate) -> bool {
    parse_local_date(due_date)
        .map(|due| due <= target_date)
        .unwrap_or(true)
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | '?' | '%' | '*' | ':' | '|' | '"' | '<' | '>' => '-',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string();

    if sanitized.is_empty() {
        "openkoto_word_pack".to_string()
    } else {
        sanitized
    }
}

pub fn calculate_sm2_update(
    repetitions: i32,
    interval_days: i32,
    ease_factor: f64,
    grade: &str,
    review_date: chrono::NaiveDate,
) -> Result<SrsUpdateResult, String> {
    let q = match grade {
        "unknown" => 2.0,
        "uncertain" => 3.0,
        "known" => 5.0,
        _ => return Err("Invalid grade, expected unknown|uncertain|known".to_string()),
    };

    let mut next_repetitions = repetitions.max(0);
    let mut next_interval_days = interval_days.max(0);
    let mut next_ease_factor = if ease_factor < 1.3 { 2.5 } else { ease_factor };
    let next_state;

    if q < 3.0 {
        next_repetitions = 0;
        next_interval_days = 1;
        next_state = "learning".to_string();
    } else {
        if next_repetitions == 0 {
            next_interval_days = 1;
        } else if next_repetitions == 1 {
            next_interval_days = 6;
        } else {
            next_interval_days = ((next_interval_days as f64) * next_ease_factor).round() as i32;
        }
        next_repetitions += 1;
        next_state = "review".to_string();
    }

    next_ease_factor = (next_ease_factor + (0.1 - (5.0 - q) * (0.08 + (5.0 - q) * 0.02))).max(1.3);
    let due_date = (review_date + chrono::Duration::days(next_interval_days as i64))
        .format("%Y-%m-%d")
        .to_string();

    Ok(SrsUpdateResult {
        srs_state: next_state,
        repetitions: next_repetitions,
        interval_days: next_interval_days,
        ease_factor: next_ease_factor,
        due_date,
    })
}

pub fn build_due_vocabulary_queue(
    mut all: Vec<FavoriteVocabulary>,
    pack_id: &str,
    date_local: &str,
    new_limit: i32,
    review_limit: i32,
) -> Result<Vec<FavoriteVocabulary>, String> {
    let target_date = parse_local_date(date_local)?;
    let new_limit = new_limit.max(0) as usize;
    let review_limit = review_limit.max(0) as usize;

    if pack_id != "all" {
        all.retain(|fav| fav.pack_ids.iter().any(|id| id == pack_id));
    }
    all.retain(|fav| is_due_on_or_before(&fav.due_date, target_date));

    let (mut new_learning, mut review): (Vec<_>, Vec<_>) = all
        .into_iter()
        .partition(|fav| fav.srs_state == "new" || fav.srs_state == "learning");

    new_learning.sort_by(sort_by_due_then_last_review);
    review.sort_by(sort_by_due_then_last_review);

    let mut queue = Vec::new();
    queue.extend(new_learning.into_iter().take(new_limit));
    queue.extend(review.into_iter().take(review_limit));
    Ok(queue)
}

#[allow(dead_code)]
fn migrate_favorite_vocabularies(app_handle: &AppHandle) -> Result<(), String> {
    let default_pack = ensure_default_word_pack(app_handle)?;
    let ids = list_favorite_vocabularies(app_handle)?;
    let today = today_local_date().format("%Y-%m-%d").to_string();

    for id in ids {
        let json = match load_favorite_vocabulary(app_handle, &id) {
            Ok(content) => content,
            Err(_) => continue,
        };

        let mut favorite = match serde_json::from_str::<FavoriteVocabulary>(&json) {
            Ok(item) => item,
            Err(_) => continue,
        };

        let mut changed = false;

        if favorite.pack_ids.is_empty() {
            favorite.pack_ids = vec![default_pack.id.clone()];
            changed = true;
        } else {
            let dedup = sanitize_pack_ids(Some(favorite.pack_ids.clone()));
            if dedup != favorite.pack_ids {
                favorite.pack_ids = dedup;
                changed = true;
            }
        }

        if favorite.srs_state != "new"
            && favorite.srs_state != "learning"
            && favorite.srs_state != "review"
        {
            favorite.srs_state = "new".to_string();
            changed = true;
        }

        if favorite.ease_factor < 1.3 {
            favorite.ease_factor = 2.5;
            changed = true;
        }

        if favorite.due_date.trim().is_empty() {
            favorite.due_date = today.clone();
            changed = true;
        } else if parse_local_date(&favorite.due_date).is_err() {
            favorite.due_date = today.clone();
            changed = true;
        }

        if favorite.interval_days < 0 {
            favorite.interval_days = 0;
            changed = true;
        }

        if favorite.repetitions < 0 {
            favorite.repetitions = 0;
            changed = true;
        }

        if favorite.review_count < 0 {
            favorite.review_count = 0;
            changed = true;
        }

        if changed {
            persist_favorite_vocabulary(app_handle, &favorite)?;
        }
    }

    Ok(())
}

// Initialize the app (ensure directories exist)
#[tauri::command]
pub async fn init_app(app_handle: AppHandle) -> Result<String, String> {
    ensure_app_dirs(&app_handle)?;
    Ok("App initialized successfully".to_string())
}

// Configuration commands
#[tauri::command]
pub async fn get_config(
    app_handle: AppHandle,
    state: AppState<'_>,
) -> Result<Option<crate::types::AppConfig>, String> {
    let config = load_config(&app_handle)?;

    // If we have a config and an active model, ensure AI service is initialized
    if let Some(ref app_config) = config {
        if let Some(active_id) = &app_config.active_model_id {
            if let Some(model_config) = app_config.get_config(active_id) {
                // We don't fail here if init fails, just log it or ignore
                // real errors will bubble up when user tries to use AI features
                let _ = get_or_create_ai_service(
                    &state,
                    model_config.api_key.clone(),
                    model_config.api_provider.clone(),
                    model_config.model.clone(),
                    model_config.base_url.clone(),
                )
                .await;
            }
        }
    }

    Ok(config)
}

#[tauri::command]
pub async fn save_config_cmd(
    app_handle: AppHandle,
    config: crate::types::AppConfig,
) -> Result<String, String> {
    save_config(&app_handle, &config)?;
    Ok("Configuration saved".to_string())
}

#[tauri::command]
pub async fn backend_check_session_cmd(
    app_handle: AppHandle,
) -> Result<BackendSessionCheck, String> {
    let mut config = load_config(&app_handle)?.unwrap_or_default();
    let backend_url = config
        .backend_url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let Some(backend_url) = backend_url else {
        return Ok(BackendSessionCheck {
            configured: false,
            connected: false,
            authenticated: false,
            backend_url: None,
            user: None,
            error: None,
        });
    };

    let health_client =
        BackendClient::for_base_url(&backend_url).map_err(backend_error_to_string)?;
    let health = match health_client.health().await {
        Ok(health) => health,
        Err(error) => {
            return Ok(BackendSessionCheck {
                configured: true,
                connected: false,
                authenticated: false,
                backend_url: Some(backend_url),
                user: None,
                error: Some(backend_error_to_string(error)),
            });
        }
    };

    if !health.auth.configured {
        return Ok(BackendSessionCheck {
            configured: true,
            connected: true,
            authenticated: false,
            backend_url: Some(backend_url),
            user: None,
            error: Some("Backend authentication is not configured. Set OPENKOTO_JWT_SECRET and restart the backend.".to_string()),
        });
    }

    if config
        .auth_token
        .as_ref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        return Ok(BackendSessionCheck {
            configured: true,
            connected: true,
            authenticated: false,
            backend_url: Some(backend_url),
            user: None,
            error: None,
        });
    }

    match BackendClient::from_app_config(&config)
        .map_err(backend_error_to_string)?
        .me()
        .await
    {
        Ok(current) => Ok(BackendSessionCheck {
            configured: true,
            connected: true,
            authenticated: true,
            backend_url: Some(backend_url),
            user: Some(current.user),
            error: None,
        }),
        Err(error) => {
            let message = if is_invalid_backend_token(&error) {
                config.auth_token = None;
                save_config(&app_handle, &config)?;
                "登录已失效，请重新登录。".to_string()
            } else {
                backend_error_to_string(error)
            };

            Ok(BackendSessionCheck {
                configured: true,
                connected: true,
                authenticated: false,
                backend_url: Some(backend_url),
                user: None,
                error: Some(message),
            })
        }
    }
}

#[tauri::command]
pub async fn backend_health_cmd(backend_url: String) -> Result<BackendHealthResponse, String> {
    BackendClient::for_base_url(&backend_url)
        .map_err(backend_error_to_string)?
        .health()
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn backend_login_cmd(
    app_handle: AppHandle,
    backend_url: String,
    email: String,
    password: String,
) -> Result<BackendAuthResult, String> {
    let client = BackendClient::for_base_url(&backend_url).map_err(backend_error_to_string)?;
    let health = client.health().await.map_err(backend_error_to_string)?;
    if !health.auth.configured {
        return Err("Backend authentication is not configured. Set OPENKOTO_JWT_SECRET and restart the backend.".to_string());
    }

    let auth = client
        .login(email.trim(), &password)
        .await
        .map_err(backend_error_to_string)?;
    save_backend_auth_result(&app_handle, backend_url, auth).await
}

#[tauri::command]
pub async fn backend_register_cmd(
    app_handle: AppHandle,
    backend_url: String,
    email: String,
    password: String,
    display_name: Option<String>,
) -> Result<BackendAuthResult, String> {
    let client = BackendClient::for_base_url(&backend_url).map_err(backend_error_to_string)?;
    let health = client.health().await.map_err(backend_error_to_string)?;
    if !health.auth.configured {
        return Err("Backend authentication is not configured. Set OPENKOTO_JWT_SECRET and restart the backend.".to_string());
    }

    let display_name = display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let auth = client
        .register(email.trim(), &password, display_name)
        .await
        .map_err(backend_error_to_string)?;
    save_backend_auth_result(&app_handle, backend_url, auth).await
}

#[tauri::command]
pub async fn backend_logout_cmd(app_handle: AppHandle) -> Result<crate::types::AppConfig, String> {
    let mut config = load_config(&app_handle)?.unwrap_or_default();
    config.auth_token = None;
    save_config(&app_handle, &config)?;
    Ok(config)
}

async fn save_backend_auth_result(
    app_handle: &AppHandle,
    backend_url: String,
    auth: crate::backend_client::BackendAuthResponse,
) -> Result<BackendAuthResult, String> {
    let mut config = load_config(app_handle)?.unwrap_or_default();
    config.backend_url = Some(backend_url.trim().trim_end_matches('/').to_string());
    config.auth_token = Some(auth.token);
    save_config(app_handle, &config)?;

    Ok(BackendAuthResult {
        config,
        user: auth.user,
        expires_at: auth.expires_at,
    })
}

/// Add or update a model configuration
#[tauri::command]
pub async fn save_model_config(
    app_handle: AppHandle,
    state: AppState<'_>,
    config: ModelConfig,
) -> Result<ModelConfig, String> {
    let mut app_config = load_config(&app_handle)?.unwrap_or_default();

    // Check if this is an update or new config
    let existing_index = app_config
        .model_configs
        .iter()
        .position(|c| c.id == config.id);

    if let Some(idx) = existing_index {
        // Update existing config
        app_config.model_configs[idx] = config.clone();
    } else {
        // Add new config
        app_config.model_configs.push(config.clone());
    }

    // Set as active if it's the first one or marked as default
    if app_config.model_configs.len() == 1 || config.is_default {
        app_config.active_model_id = Some(config.id.clone());
        // Unset other defaults
        for c in &mut app_config.model_configs {
            if c.id != config.id {
                c.is_default = false;
            }
        }
    }

    save_config(&app_handle, &app_config)?;

    // Update AI service cache if this is the active config
    if app_config.active_model_id.as_ref() == Some(&config.id) {
        get_or_create_ai_service(
            &state,
            config.api_key.clone(),
            config.api_provider.clone(),
            config.model.clone(),
            config.base_url.clone(),
        )
        .await?;
    }

    Ok(config)
}

/// Delete a model configuration
#[tauri::command]
pub async fn delete_model_config(app_handle: AppHandle, config_id: String) -> Result<(), String> {
    let mut app_config = load_config(&app_handle)?.unwrap_or_default();

    // Remove the config
    let original_len = app_config.model_configs.len();
    app_config.model_configs.retain(|c| c.id != config_id);

    if app_config.model_configs.len() == original_len {
        return Err("Configuration not found".to_string());
    }

    // If we deleted the active config, set a new active one
    if app_config.active_model_id.as_ref() == Some(&config_id) {
        app_config.active_model_id = app_config.model_configs.first().map(|c| c.id.clone());
    }

    save_config(&app_handle, &app_config)?;
    Ok(())
}

/// Set the active model configuration
#[tauri::command]
pub async fn set_active_model_config(
    app_handle: AppHandle,
    state: AppState<'_>,
    config_id: String,
) -> Result<ModelConfig, String> {
    let mut app_config = load_config(&app_handle)?.unwrap_or_default();

    let config = app_config
        .get_config(&config_id)
        .ok_or("Configuration not found")?
        .clone();

    app_config.active_model_id = Some(config_id.clone());

    save_config(&app_handle, &app_config)?;

    // Update AI service cache
    get_or_create_ai_service(
        &state,
        config.api_key.clone(),
        config.api_provider.clone(),
        config.model.clone(),
        config.base_url.clone(),
    )
    .await?;

    Ok(config)
}

/// Get the active model configuration
#[tauri::command]
pub async fn get_active_model_config(app_handle: AppHandle) -> Result<Option<ModelConfig>, String> {
    let app_config = load_config(&app_handle)?.unwrap_or_default();
    Ok(app_config.get_active_config().cloned())
}

/// Legacy command for backward compatibility - redirects to new model config system
#[tauri::command]
pub async fn set_api_key(
    app_handle: AppHandle,
    state: AppState<'_>,
    api_key: String,
    provider: String,
    model: String,
) -> Result<String, String> {
    let mut app_config = load_config(&app_handle)?.unwrap_or_default();

    // Create a default config name
    let config_name = format!("{} - {}", provider, model);

    // Check if a config with same provider/model already exists
    let existing = app_config
        .model_configs
        .iter()
        .find(|c| c.api_provider == provider && c.model == model);

    let config = if let Some(existing) = existing {
        // Update existing
        ModelConfig {
            api_key,
            ..existing.clone()
        }
    } else {
        // Create new
        ModelConfig::new(config_name, api_key, provider, model)
    };

    let config_id = config.id.clone();

    // Add or update
    let existing_index = app_config
        .model_configs
        .iter()
        .position(|c| c.id == config.id);
    if let Some(idx) = existing_index {
        app_config.model_configs[idx] = config.clone();
    } else {
        app_config.model_configs.push(config.clone());
    }

    // Set as active
    app_config.active_model_id = Some(config_id.clone());

    save_config(&app_handle, &app_config)?;

    // Update AI service cache
    get_or_create_ai_service(
        &state,
        config.api_key.clone(),
        config.api_provider.clone(),
        config.model.clone(),
        config.base_url.clone(),
    )
    .await?;

    Ok("API key saved successfully".to_string())
}

// Article commands
#[tauri::command]
pub async fn create_article(
    app_handle: AppHandle,
    title: String,
    content: String,
    source_url: Option<String>,
    import_job_id: Option<String>,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    let source = MaterialImportSource {
        source_kind: "article".to_string(),
        source_uri: source_url.clone(),
        content: Some(content.clone()),
        file_path: None,
        file_id: None,
        title: Some(title.clone()),
        metadata: serde_json::json!({ "source": "desktop_create_article" }),
    };
    run_material_import(
        &app_handle,
        source,
        ImportJobOptions {
            import_job_id,
            duplicate_policy,
        },
        || async move {
            let id = Uuid::new_v4().to_string();
            Ok(PreparedMaterialImport {
                article: Article {
                    id: id.clone(),
                    title,
                    content: content.clone(),
                    source_type: Some("article".to_string()),
                    source_url,
                    media_path: None,
                    book_path: None,
                    book_type: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    translated: false,
                    active_mind_map_artifact_id: None,
                    segments: create_segments_from_content(&id, &content),
                    metadata: serde_json::json!({}),
                    tags: Vec::new(),
                    reading_progress: None,
                    archived_at: None,
                },
                metadata: None,
                file_sha256: None,
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn resegment_article(
    app_handle: AppHandle,
    article_id: String,
) -> Result<Article, String> {
    let mut article = get_article(app_handle.clone(), article_id).await?;

    article.segments = create_segments_from_content(&article.id, &article.content);
    replace_backend_article(&app_handle, &article).await
}

#[tauri::command]
pub async fn get_article(app_handle: AppHandle, id: String) -> Result<Article, String> {
    let client = backend_client_for_app(&app_handle)?;
    client
        .get_material(&id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn list_articles_cmd(app_handle: AppHandle) -> Result<Vec<Article>, String> {
    let client = backend_client_for_app(&app_handle)?;
    client
        .list_materials()
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn update_article(
    app_handle: AppHandle,
    id: String,
    title: Option<String>,
    content: Option<String>,
    source_url: Option<String>,
    translated: Option<bool>,
) -> Result<Article, String> {
    let mut article = get_article(app_handle.clone(), id.clone()).await?;

    if let Some(t) = title {
        article.title = t;
    }
    if let Some(c) = content {
        article.content = c;
    }
    if let Some(s) = source_url {
        article.source_url = Some(s);
    }
    if let Some(t) = translated {
        article.translated = t;
    }

    replace_backend_article(&app_handle, &article).await
}

#[tauri::command]
pub async fn delete_article_cmd(app_handle: AppHandle, id: String) -> Result<(), String> {
    let client = backend_client_for_app(&app_handle)?;
    client
        .delete_material(&id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn list_learning_items_cmd(
    app_handle: AppHandle,
    query: Option<ListLearningItemsRequest>,
) -> Result<Vec<LearningItem>, String> {
    backend_client_for_app(&app_handle)?
        .list_learning_items(query.as_ref())
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn create_learning_item_cmd(
    app_handle: AppHandle,
    payload: CreateLearningItemRequest,
) -> Result<LearningItem, String> {
    backend_client_for_app(&app_handle)?
        .create_learning_item(&payload)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn create_learning_item_from_selection_cmd(
    app_handle: AppHandle,
    payload: CreateLearningItemFromSelectionRequest,
) -> Result<LearningItem, String> {
    backend_client_for_app(&app_handle)?
        .create_learning_item_from_selection(&payload)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn update_learning_item_cmd(
    app_handle: AppHandle,
    id: String,
    payload: UpdateLearningItemRequest,
) -> Result<LearningItem, String> {
    backend_client_for_app(&app_handle)?
        .patch_learning_item(&id, &payload)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn accept_learning_item_cmd(
    app_handle: AppHandle,
    id: String,
    payload: AcceptLearningItemRequest,
) -> Result<AcceptLearningItemResponse, String> {
    validate_uuid(&id, "id")?;
    backend_client_for_app(&app_handle)?
        .accept_learning_item(&id, &payload)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn delete_learning_item_cmd(app_handle: AppHandle, id: String) -> Result<(), String> {
    backend_client_for_app(&app_handle)?
        .delete_learning_item(&id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn update_article_segment(
    app_handle: AppHandle,
    article_id: String,
    segment_id: String,
    explanation: Option<crate::types::SegmentExplanation>,
    reading: Option<String>,
    translation: Option<String>,
) -> Result<Article, String> {
    let mut article = get_article(app_handle.clone(), article_id.clone()).await?;

    if let Some(segment) = article.segments.iter_mut().find(|s| s.id == segment_id) {
        if let Some(exp) = explanation {
            segment.explanation = Some(exp);
        }
        if let Some(read) = reading {
            segment.reading_text = Some(read);
        }
        if let Some(trans) = translation {
            segment.translation = Some(trans);
        }
    } else {
        return Err("Segment not found".to_string());
    }

    replace_backend_article(&app_handle, &article).await
}

// AI commands
#[tauri::command]
pub async fn translate_text(
    state: AppState<'_>,
    request: TranslationRequest,
) -> Result<TranslationResponse, String> {
    let ai_service = get_ai_service(&state).await?;
    ai_service.translate(request).await
}

#[tauri::command]
pub async fn analyze_text(
    state: AppState<'_>,
    request: AnalysisRequest,
) -> Result<AnalysisResponse, String> {
    let ai_service = get_ai_service(&state).await?;
    ai_service.analyze(request).await
}

#[tauri::command]
pub async fn chat_completion(
    state: AppState<'_>,
    request: ChatRequest,
) -> Result<ChatResponse, String> {
    let ai_service = get_ai_service(&state).await?;
    ai_service.chat(request).await
}

#[tauri::command]
pub async fn stream_chat_completion(
    app_handle: AppHandle,
    state: AppState<'_>,
    request: ChatRequest,
    event_id: String,
) -> Result<String, String> {
    let ai_service = get_ai_service(&state).await?;

    // Create a callback that emits events to the frontend
    let app_handle_clone = app_handle.clone();
    let event_name = format!("chat-stream://{}", event_id);

    ai_service
        .stream_chat(request, move |chunk| {
            // Emit the chunk to the frontend
            // We ignore errors here as we can't do much if emission fails
            let _ = app_handle_clone.emit(&event_name, chunk);
        })
        .await
}

#[tauri::command]
pub async fn segment_translate_explain_cmd(
    state: AppState<'_>,
    text: String,
    target_language: String,
) -> Result<crate::types::SegmentExplanation, String> {
    let ai_service = get_ai_service(&state).await?;
    ai_service
        .segment_translate_explain(text, target_language)
        .await
}

#[tauri::command]
pub async fn translate_article(
    app_handle: AppHandle,
    state: AppState<'_>,
    article_id: String,
    target_language: String,
) -> Result<Article, String> {
    let mut article = get_article(app_handle.clone(), article_id.clone()).await?;

    // Ensure segments exist
    if article.segments.is_empty() {
        article.segments = create_segments_from_content(&article.id, &article.content);
    }

    // 收集需要翻译的段落（没有翻译的）
    let untranslated: Vec<(String, String)> = article
        .segments
        .iter()
        .filter(|s| s.translation.is_none())
        .map(|s| (s.id.clone(), s.text.clone()))
        .collect();

    if !untranslated.is_empty() {
        let ai_service = get_ai_service(&state).await?;

        // 批量翻译（每批最多30条）
        const BATCH_SIZE: usize = 30;
        let total_count = untranslated.len();
        let total_chunks = (total_count + BATCH_SIZE - 1) / BATCH_SIZE;

        println!(
            "[Article] Starting quick translation for article: {}, items: {}",
            article_id, total_count
        );

        for (i, chunk) in untranslated.chunks(BATCH_SIZE).enumerate() {
            println!(
                "[Article] Translating chunk {}/{} ({} items)...",
                i + 1,
                total_chunks,
                chunk.len()
            );
            let batch_items: Vec<(String, String)> = chunk.to_vec();

            match ai_service
                .batch_translate(batch_items, &target_language)
                .await
            {
                Ok(translations) => {
                    // 将翻译结果写回对应的 segment
                    for (id, translation) in translations {
                        if let Some(seg) = article.segments.iter_mut().find(|s| s.id == id) {
                            seg.translation = Some(translation);
                        }
                    }
                    println!(
                        "[Article] Chunk {}/{} completed successfully",
                        i + 1,
                        total_chunks
                    );

                    // Emit progress event
                    let progress = serde_json::json!({
                        "current": (i + 1) * BATCH_SIZE,
                        "total": total_count,
                        "message": format!("Translating chunk {}/{}", i + 1, total_chunks)
                    });
                    let _ = app_handle
                        .emit(&format!("translation-progress://{}", article_id), progress);
                }
                Err(e) => {
                    // 批量翻译失败，记录错误但继续
                    eprintln!(
                        "[Article] Batch translation error in chunk {}/{}: {}",
                        i + 1,
                        total_chunks,
                        e
                    );
                }
            }
        }
    }

    // Emit complete event
    let _ = app_handle.emit(
        &format!("translation-progress://{}", article_id),
        serde_json::json!({
            "current": untranslated.len(),
            "total": untranslated.len(),
            "message": "Translation completed"
        }),
    );

    println!(
        "[Article] Quick translation completed for article: {}",
        article_id
    );
    article.translated = true;

    replace_backend_article(&app_handle, &article).await
}

#[tauri::command]
pub async fn analyze_article(
    app_handle: AppHandle,
    state: AppState<'_>,
    article_id: String,
    analysis_type: String,
) -> Result<String, String> {
    let article = get_article(app_handle.clone(), article_id.clone()).await?;

    let analysis_type = match analysis_type.as_str() {
        "summary" => AnalysisType::Summary,
        "key_points" => AnalysisType::KeyPoints,
        "vocabulary" => AnalysisType::Vocabulary,
        "grammar" => AnalysisType::Grammar,
        "full" => AnalysisType::FullAnalysis,
        _ => return Err("Invalid analysis type".to_string()),
    };

    let request = AnalysisRequest {
        text: article.content,
        analysis_type,
    };

    let response = analyze_text(state, request).await?;
    Ok(response.result)
}

// Return type for fetch_url_content
#[derive(serde::Serialize)]
pub struct FetchedContent {
    pub title: String,
    pub content: String,
}

// Fetch content from a URL
#[tauri::command]
pub async fn fetch_url_content(url: String) -> Result<FetchedContent, String> {
    require_external_tools_enabled("fetch_url_content")?;

    // Validate URL
    let parsed_url = url::Url::parse(&url).map_err(|_| "Invalid URL format".to_string())?;

    // Only allow http/https
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err("Only HTTP and HTTPS URLs are supported".to_string());
    }

    // Create HTTP client with timeout
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    // Fetch the page with better headers to avoid blocking
    let response = client
        .get(&url)
        .header("User-Agent", "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9,zh-CN;q=0.8,zh;q=0.7")
        .send()
        .await
        .map_err(|e| format!("Failed to fetch URL: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    // Get HTML content
    // Note: readability prefers a "Cursor" or string. We'll get text first.
    let html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Pre-process HTML to handle common issues (optional)
    // For now, feed directly to readability.

    // Extract content using readability
    // This removes ads, sidebars, navigation, and JS.
    let mut cursor = std::io::Cursor::new(html.as_bytes());
    let mut title = String::new();
    let mut content = String::new();

    // Try readability first
    if let Ok(extracted) =
        readability::extractor::extract(&mut cursor, &url::Url::parse(&url).unwrap())
    {
        title = extracted.title;
        content = html_to_text_preserving_layout(&extracted.content);
    }

    // Check if we got meaningful content. If not, try fallback selectors.
    // Uta-net returns very short content (e.g. "Voting thanks") via readability.
    if content.trim().len() < 200 {
        if let Some(fallback_content) = try_fallback_extraction(&html) {
            // If fallback found something substantial, use it
            if fallback_content.len() > content.len() {
                content = html_to_text_preserving_layout(&fallback_content);
                // If title was missing, try to get it again or keep old one
                if title.is_empty() {
                    title = extract_title_from_html(&html, &url);
                }
            }
        }
    }

    // Final check
    if content.trim().len() < 10 {
        if content.trim().is_empty() {
            return Err("Could not extract meaningful content. The page might be empty or require JavaScript interaction that is not supported.".to_string());
        }
    }

    // If title is still empty
    if title.is_empty() {
        title = extract_title_from_html(&html, &url);
    }

    Ok(FetchedContent { title, content })
}

/// Fallback extraction using CSS selectors for known difficult sites
fn try_fallback_extraction(html: &str) -> Option<String> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);

    // List of selectors to try, in order of preference
    // #kashi_area: Uta-net
    // .lyrics_box: common lyrics class
    // #lyrics: common lyrics id
    let selectors = vec![
        "#kashi_area",
        "div[itemprop='text']", // Generic schema.org text
        ".lyrics",
        "#lyrics",
        ".post-content",
        "article",
        "main",
    ];

    for selector_str in selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                let html_content = element.html();
                // Simple heuristic: must be at least somewhat long
                if html_content.len() > 100 {
                    return Some(html_content);
                }
            }
        }
    }

    None
}

/// Convert HTML to text, preserving significant layout (newlines)
/// Ideal for lyrics, poems, and clean articles.
fn html_to_text_preserving_layout(html: &str) -> String {
    use regex::Regex;

    // 1. Normalize newlines in source to spaces (browser behavior), we will re-add them based on tags.
    let normalized = html.replace("\r", " ").replace("\n", " ");

    // 2. Replace block tags with sentinel newlines
    // <br>, <br/> -> \n
    // <p>, <div>, <li>, <h1>-<h6>, <blockquote>, <pre> -> \n\n (surround with breaks)
    // </tr> -> \n (table rows)
    let re_br = Regex::new(r"(?i)<br\s*/?>").unwrap();
    let with_br = re_br.replace_all(&normalized, "\n");

    let re_block_start = Regex::new(r"(?i)<(p|div|h[1-6]|li|blockquote|pre|tr)[^>]*>").unwrap();
    let with_block_start = re_block_start.replace_all(&with_br, "\n"); // Add newline before block

    let re_block_end = Regex::new(r"(?i)</(p|div|h[1-6]|li|blockquote|pre|tr)>").unwrap();
    let with_block_end = re_block_end.replace_all(&with_block_start, "\n\n"); // Add double newline after block

    // 3. Strip all other tags
    let re_tags = Regex::new(r"<[^>]*>").unwrap();
    let stripped = re_tags.replace_all(&with_block_end, "");

    // 4. Decode HTML entities
    let decoded = html_escape::decode_html_entities(&stripped);

    // 5. Clean up whitespace
    // Split by newline, trim each line, filter empty lines if they are excessive (more than 2)
    // But for lyrics, we want to keep single empty lines (stanza breaks).
    let lines: Vec<&str> = decoded.lines().collect();
    let mut clean_lines = Vec::new();
    let mut empty_count = 0;

    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            empty_count += 1;
            // Allow up to 2 consecutive empty lines (paragraph break)
            if empty_count <= 2 {
                clean_lines.push("");
            }
        } else {
            empty_count = 0;
            clean_lines.push(trimmed);
        }
    }

    let result = clean_lines.join("\n");

    // Final trim of the whole text
    result.trim().to_string()
}

// Extract title from HTML
fn extract_title_from_html(html: &str, url: &str) -> String {
    let html_lower = html.to_lowercase();

    // Find <title> tag
    if let Some(start) = html_lower.find("<title>") {
        let start = start + 7; // len("<title>")
        if let Some(end) = html_lower[start..].find("</title>") {
            let title_html = &html[start..start + end];
            // Decode basic HTML entities
            let decoded = html_escape::decode_html_entities(title_html).to_string();
            let trimmed = decoded.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }

    // Fallback: extract from URL
    if let Ok(parsed) = url::Url::parse(url) {
        if let Some(segments) = parsed.path_segments() {
            let last = segments.last().unwrap_or("");
            if !last.is_empty() {
                return last
                    .replace('-', " ")
                    .replace('_', " ")
                    .split(' ')
                    .map(|s| {
                        let mut chars = s.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(first) => {
                                if !first.is_alphabetic() {
                                    String::new()
                                } else {
                                    first.to_uppercase().collect::<String>() + chars.as_str()
                                }
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            return parsed.host_str().unwrap_or("Untitled").to_string();
        }
    }

    "Untitled".to_string()
}

// ============================================================================
// Favorites Commands - 收藏夹命令
// ============================================================================

/// 创建单词包
#[tauri::command]
pub async fn create_word_pack_cmd(
    app_handle: AppHandle,
    name: String,
    description: Option<String>,
    cover_url: Option<String>,
    author: Option<String>,
    language_from: Option<String>,
    language_to: Option<String>,
    tags: Option<Vec<String>>,
    version: Option<String>,
) -> Result<WordPack, String> {
    let now = chrono::Utc::now().to_rfc3339();
    let pack = WordPack {
        id: Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        description,
        cover_url,
        author,
        language_from,
        language_to,
        tags: tags.unwrap_or_default(),
        version,
        created_at: now.clone(),
        updated_at: now,
        is_system: false,
    };

    if pack.name.is_empty() {
        return Err("Pack name is required".to_string());
    }

    backend_client_for_app(&app_handle)?
        .upsert_word_pack(&pack)
        .await
        .map_err(backend_error_to_string)
}

/// 更新单词包
#[tauri::command]
pub async fn update_word_pack_cmd(
    app_handle: AppHandle,
    id: String,
    name: Option<String>,
    description: Option<String>,
    cover_url: Option<String>,
    author: Option<String>,
    language_from: Option<String>,
    language_to: Option<String>,
    tags: Option<Vec<String>>,
    version: Option<String>,
) -> Result<WordPack, String> {
    let client = backend_client_for_app(&app_handle)?;
    let mut pack = client
        .get_word_pack(&id)
        .await
        .map_err(backend_error_to_string)?;

    if let Some(name) = name {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err("Pack name is required".to_string());
        }
        pack.name = trimmed.to_string();
    }
    if description.is_some() {
        pack.description = description;
    }
    if cover_url.is_some() {
        pack.cover_url = cover_url;
    }
    if author.is_some() {
        pack.author = author;
    }
    if language_from.is_some() {
        pack.language_from = language_from;
    }
    if language_to.is_some() {
        pack.language_to = language_to;
    }
    if tags.is_some() {
        pack.tags = tags.unwrap_or_default();
    }
    if version.is_some() {
        pack.version = version;
    }

    pack.updated_at = chrono::Utc::now().to_rfc3339();

    client
        .patch_word_pack(&pack.id, &pack)
        .await
        .map_err(backend_error_to_string)
}

/// 列出所有单词包
#[tauri::command]
pub async fn list_word_packs_cmd(app_handle: AppHandle) -> Result<Vec<WordPack>, String> {
    let mut packs = backend_client_for_app(&app_handle)?
        .list_word_packs()
        .await
        .map_err(backend_error_to_string)?;
    packs.sort_by(|a, b| a.name.cmp(&b.name));
    packs.sort_by(|a, b| b.is_system.cmp(&a.is_system));
    Ok(packs)
}

/// 删除单词包（系统包不可删除）
#[tauri::command]
pub async fn delete_word_pack_cmd(app_handle: AppHandle, id: String) -> Result<(), String> {
    if id == DEFAULT_UNGROUPED_PACK_ID {
        return Err("System pack cannot be deleted".to_string());
    }

    backend_client_for_app(&app_handle)?
        .delete_word_pack(&id)
        .await
        .map_err(backend_error_to_string)
}

/// 添加单词收藏
#[tauri::command]
pub async fn add_favorite_vocabulary_cmd(
    app_handle: AppHandle,
    word: String,
    meaning: String,
    usage: String,
    explanation: Option<String>,
    example: Option<String>,
    reading: Option<String>,
    source_article_id: Option<String>,
    source_article_title: Option<String>,
    pack_ids: Option<Vec<String>>,
) -> Result<FavoriteVocabulary, String> {
    let client = backend_client_for_app(&app_handle)?;
    let packs = client
        .list_word_packs()
        .await
        .map_err(backend_error_to_string)?;
    let default_pack = default_word_pack_from_packs(&packs);
    let existing_pack_ids: HashSet<String> = packs.into_iter().map(|p| p.id).collect();

    let normalized_input = normalize_word(&word);
    if normalized_input.is_empty() || meaning.trim().is_empty() {
        return Err("Word and meaning are required".to_string());
    }

    let mut pack_ids = filter_existing_pack_ids(
        sanitize_pack_ids(pack_ids),
        &existing_pack_ids,
        &default_pack.id,
    );

    let mut favorites = client
        .list_favorite_vocabularies()
        .await
        .map_err(backend_error_to_string)?;
    if let Some(existing) = favorites
        .iter_mut()
        .find(|fav| normalize_word(&fav.word) == normalized_input)
    {
        let mut merged = existing.pack_ids.clone();
        merged.append(&mut pack_ids);
        existing.pack_ids = sanitize_pack_ids(Some(merged));
        if existing.pack_ids.is_empty() {
            existing.pack_ids.push(default_pack.id.clone());
        }

        if existing.meaning.trim().is_empty() {
            existing.meaning = meaning.clone();
        }
        if existing.usage.trim().is_empty() {
            existing.usage = usage.clone();
        }
        if existing.example.is_none() {
            existing.example = example.clone();
        }
        if existing.reading.is_none() {
            existing.reading = reading.clone();
        }
        if existing.explanation.is_none() {
            existing.explanation = explanation.clone();
        }
        if existing.source_article_id.is_none() {
            existing.source_article_id = source_article_id.clone();
        }
        if existing.source_article_title.is_none() {
            existing.source_article_title = source_article_title.clone();
        }

        return client
            .patch_favorite_vocabulary(&existing.id, existing)
            .await
            .map_err(backend_error_to_string);
    }

    let favorite = FavoriteVocabulary {
        id: Uuid::new_v4().to_string(),
        word: word.trim().to_string(),
        meaning: meaning.trim().to_string(),
        usage: usage.trim().to_string(),
        explanation,
        example,
        reading,
        source_article_id,
        source_article_title,
        pack_ids,
        srs_state: "new".to_string(),
        ease_factor: 2.5,
        repetitions: 0,
        interval_days: 0,
        due_date: today_local_date().format("%Y-%m-%d").to_string(),
        last_reviewed_at: None,
        review_count: 0,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    client
        .upsert_favorite_vocabulary(&favorite)
        .await
        .map_err(backend_error_to_string)
}

/// 列出所有单词收藏
#[tauri::command]
pub async fn list_favorite_vocabularies_cmd(
    app_handle: AppHandle,
) -> Result<Vec<FavoriteVocabulary>, String> {
    let mut favorites = backend_client_for_app(&app_handle)?
        .list_favorite_vocabularies()
        .await
        .map_err(backend_error_to_string)?;

    // 按创建时间降序排列
    favorites.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(favorites)
}

/// 删除单词收藏
#[tauri::command]
pub async fn delete_favorite_vocabulary_cmd(
    app_handle: AppHandle,
    id: String,
) -> Result<(), String> {
    backend_client_for_app(&app_handle)?
        .delete_favorite_vocabulary(&id)
        .await
        .map_err(backend_error_to_string)
}

/// 设置单词收藏所属合集
#[tauri::command]
pub async fn set_vocabulary_pack_ids_cmd(
    app_handle: AppHandle,
    vocabulary_id: String,
    pack_ids: Vec<String>,
) -> Result<FavoriteVocabulary, String> {
    let client = backend_client_for_app(&app_handle)?;
    let packs = client
        .list_word_packs()
        .await
        .map_err(backend_error_to_string)?;
    let default_pack = default_word_pack_from_packs(&packs);
    let existing_pack_ids: HashSet<String> = packs.into_iter().map(|pack| pack.id).collect();
    let mut favorite = client
        .get_favorite_vocabulary(&vocabulary_id)
        .await
        .map_err(backend_error_to_string)?;

    favorite.pack_ids = filter_existing_pack_ids(
        sanitize_pack_ids(Some(pack_ids)),
        &existing_pack_ids,
        &default_pack.id,
    );
    let favorite_id = favorite.id.clone();
    client
        .patch_favorite_vocabulary(&favorite_id, &favorite)
        .await
        .map_err(backend_error_to_string)
}

/// 按合集列出单词收藏
#[tauri::command]
pub async fn list_favorite_vocabularies_by_pack_cmd(
    app_handle: AppHandle,
    pack_id: String,
) -> Result<Vec<FavoriteVocabulary>, String> {
    let mut favorites = list_favorite_vocabularies_cmd(app_handle).await?;
    if pack_id != "all" {
        favorites.retain(|fav| fav.pack_ids.iter().any(|id| id == &pack_id));
    }
    favorites.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(favorites)
}

/// 获取指定日期到期的背诵队列
#[tauri::command]
pub async fn get_due_vocabulary_queue_cmd(
    app_handle: AppHandle,
    pack_id: String,
    date_local: String,
) -> Result<Vec<FavoriteVocabulary>, String> {
    let config = load_config(&app_handle)?.unwrap_or_default();
    let all = list_favorite_vocabularies_cmd(app_handle).await?;
    build_due_vocabulary_queue(
        all,
        &pack_id,
        &date_local,
        config.srs_daily_new_limit,
        config.srs_daily_review_limit,
    )
}

/// 复习单词并更新 SM-2 状态
#[tauri::command]
pub async fn review_vocabulary_cmd(
    app_handle: AppHandle,
    vocabulary_id: String,
    grade: String,
    date_local: String,
) -> Result<FavoriteVocabulary, String> {
    let review_date = parse_local_date(&date_local)?;

    let client = backend_client_for_app(&app_handle)?;
    let mut favorite = client
        .get_favorite_vocabulary(&vocabulary_id)
        .await
        .map_err(backend_error_to_string)?;

    let next = calculate_sm2_update(
        favorite.repetitions,
        favorite.interval_days,
        favorite.ease_factor,
        &grade,
        review_date,
    )?;

    favorite.srs_state = next.srs_state;
    favorite.repetitions = next.repetitions;
    favorite.interval_days = next.interval_days;
    favorite.ease_factor = next.ease_factor;
    favorite.due_date = next.due_date;
    favorite.last_reviewed_at = Some(chrono::Utc::now().to_rfc3339());
    favorite.review_count += 1;

    let favorite_id = favorite.id.clone();
    client
        .patch_favorite_vocabulary(&favorite_id, &favorite)
        .await
        .map_err(backend_error_to_string)
}

/// 导出单词包为 OpenKoto JSON 包
#[tauri::command]
pub async fn export_word_pack_cmd(
    app_handle: AppHandle,
    pack_id: String,
) -> Result<ExportWordPackResult, String> {
    let client = backend_client_for_app(&app_handle)?;
    if pack_id == "all" {
        let entries = client
            .list_favorite_vocabularies()
            .await
            .map_err(backend_error_to_string)?
            .into_iter()
            .map(favorite_to_word_pack_export_entry)
            .collect();

        return build_word_pack_export_result(
            WordPackExportMeta {
                name: "全部单词".to_string(),
                description: Some("所有收藏单词".to_string()),
                cover_url: None,
                author: None,
                language_from: None,
                language_to: None,
                tags: Vec::new(),
                version: Some("1.0.0".to_string()),
            },
            entries,
        );
    }

    let pack = client
        .get_word_pack(&pack_id)
        .await
        .map_err(backend_error_to_string)?;

    let entries: Vec<WordPackExportEntry> =
        list_favorite_vocabularies_by_pack_cmd(app_handle.clone(), pack_id)
            .await?
            .into_iter()
            .map(favorite_to_word_pack_export_entry)
            .collect();

    build_word_pack_export_result(
        WordPackExportMeta {
            name: pack.name.clone(),
            description: pack.description.clone(),
            cover_url: pack.cover_url.clone(),
            author: pack.author.clone(),
            language_from: pack.language_from.clone(),
            language_to: pack.language_to.clone(),
            tags: pack.tags.clone(),
            version: pack.version.clone(),
        },
        entries,
    )
}

/// 导入 OpenKoto JSON 单词包
#[tauri::command]
pub async fn import_word_pack_cmd(
    app_handle: AppHandle,
    json_content: String,
) -> Result<ImportWordPackResult, String> {
    let client = backend_client_for_app(&app_handle)?;
    let packs = client
        .list_word_packs()
        .await
        .map_err(backend_error_to_string)?;
    let default_pack = default_word_pack_from_packs(&packs);
    let parsed = parse_import_word_pack_json(&json_content)?;

    if parsed.entries.len() > 20000 {
        return Err("Word pack is too large (max 20000 entries)".to_string());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let pack = WordPack {
        id: Uuid::new_v4().to_string(),
        name: if parsed.pack.name.trim().is_empty() {
            "Imported Pack".to_string()
        } else {
            parsed.pack.name.trim().to_string()
        },
        description: parsed.pack.description.clone(),
        cover_url: parsed.pack.cover_url.clone(),
        author: parsed.pack.author.clone(),
        language_from: parsed.pack.language_from.clone(),
        language_to: parsed.pack.language_to.clone(),
        tags: parsed.pack.tags.clone(),
        version: parsed.pack.version.clone(),
        created_at: now.clone(),
        updated_at: now,
        is_system: false,
    };

    let pack = client
        .upsert_word_pack(&pack)
        .await
        .map_err(backend_error_to_string)?;

    let mut existing_by_word: HashMap<String, FavoriteVocabulary> = client
        .list_favorite_vocabularies()
        .await
        .map_err(backend_error_to_string)?
        .into_iter()
        .filter_map(|fav| {
            let normalized = normalize_word(&fav.word);
            if normalized.is_empty() {
                None
            } else {
                Some((normalized, fav))
            }
        })
        .collect();
    let mut file_seen_words = HashSet::new();

    let total = parsed.entries.len();
    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors = Vec::new();

    for (index, entry) in parsed.entries.into_iter().enumerate() {
        let word = entry.word.trim().to_string();
        let meaning = entry.meaning.trim().to_string();
        if word.is_empty() || meaning.is_empty() {
            skipped += 1;
            errors.push(format!("Entry {} missing required word/meaning", index + 1));
            continue;
        }

        let normalized = normalize_word(&word);
        if file_seen_words.contains(&normalized) {
            skipped += 1;
            continue;
        }

        file_seen_words.insert(normalized.clone());

        let usage = entry.usage.unwrap_or_default();
        let example = entry.example;
        let reading = entry.reading;
        let explanation = entry.explanation;

        if let Some(existing) = existing_by_word.get_mut(&normalized) {
            let mut merged_pack_ids = existing.pack_ids.clone();
            merged_pack_ids.push(pack.id.clone());
            existing.pack_ids = sanitize_pack_ids(Some(merged_pack_ids));
            if existing.pack_ids.is_empty() {
                existing.pack_ids.push(default_pack.id.clone());
            }

            if existing.meaning.trim().is_empty() {
                existing.meaning = meaning;
            }
            if existing.usage.trim().is_empty() {
                existing.usage = usage;
            }
            if existing.example.is_none() {
                existing.example = example;
            }
            if existing.reading.is_none() {
                existing.reading = reading;
            }
            if existing.explanation.is_none() {
                existing.explanation = explanation;
            }

            if let Err(e) = client
                .patch_favorite_vocabulary(&existing.id, existing)
                .await
                .map_err(backend_error_to_string)
            {
                skipped += 1;
                errors.push(format!("Entry {} failed to merge: {}", index + 1, e));
                continue;
            }

            imported += 1;
            continue;
        }

        let favorite = FavoriteVocabulary {
            id: Uuid::new_v4().to_string(),
            word,
            meaning,
            usage,
            explanation,
            example,
            reading,
            source_article_id: None,
            source_article_title: None,
            pack_ids: vec![pack.id.clone()],
            srs_state: "new".to_string(),
            ease_factor: 2.5,
            repetitions: 0,
            interval_days: 0,
            due_date: today_local_date().format("%Y-%m-%d").to_string(),
            last_reviewed_at: None,
            review_count: 0,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        if let Err(e) = client
            .upsert_favorite_vocabulary(&favorite)
            .await
            .map_err(backend_error_to_string)
        {
            skipped += 1;
            errors.push(format!("Entry {} failed to import: {}", index + 1, e));
            continue;
        }

        existing_by_word.insert(normalized, favorite.clone());
        imported += 1;
    }

    Ok(ImportWordPackResult {
        created_pack_id: pack.id,
        total,
        imported,
        skipped,
        errors,
    })
}

/// 添加语法收藏
#[tauri::command]
pub async fn add_favorite_grammar_cmd(
    app_handle: AppHandle,
    point: String,
    explanation: String,
    example: Option<String>,
    source_article_id: Option<String>,
    source_article_title: Option<String>,
) -> Result<FavoriteGrammar, String> {
    let favorite = FavoriteGrammar {
        id: Uuid::new_v4().to_string(),
        point,
        explanation,
        example,
        source_article_id,
        source_article_title,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    backend_client_for_app(&app_handle)?
        .upsert_favorite_grammar(&favorite)
        .await
        .map_err(backend_error_to_string)
}

/// 列出所有语法收藏
#[tauri::command]
pub async fn list_favorite_grammars_cmd(
    app_handle: AppHandle,
) -> Result<Vec<FavoriteGrammar>, String> {
    let mut favorites = backend_client_for_app(&app_handle)?
        .list_favorite_grammars()
        .await
        .map_err(backend_error_to_string)?;

    // 按创建时间降序排列
    favorites.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(favorites)
}

/// 删除语法收藏
#[tauri::command]
pub async fn delete_favorite_grammar_cmd(app_handle: AppHandle, id: String) -> Result<(), String> {
    backend_client_for_app(&app_handle)?
        .delete_favorite_grammar(&id)
        .await
        .map_err(backend_error_to_string)
}

// YouTube Import
#[tauri::command]
pub async fn import_youtube_video_cmd(
    app_handle: AppHandle,
    url: String,
    import_job_id: Option<String>,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    require_external_tools_enabled("import_youtube_video_cmd")?;
    let source = MaterialImportSource {
        source_kind: "youtube".to_string(),
        source_uri: Some(url.clone()),
        content: None,
        file_path: None,
        file_id: None,
        title: None,
        metadata: serde_json::json!({ "source": "desktop_import" }),
    };
    let import_handle = app_handle.clone();
    run_material_import(
        &app_handle,
        source,
        ImportJobOptions {
            import_job_id,
            duplicate_policy,
        },
        || async move {
            let article = crate::youtube::import_youtube_video(import_handle, url).await?;
            let hash = article
                .media_path
                .as_deref()
                .map(Path::new)
                .map(file_sha256)
                .transpose()?;
            Ok(PreparedMaterialImport {
                article,
                metadata: Some(serde_json::json!({ "source": "youtube" })),
                file_sha256: hash,
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn import_local_video_cmd(
    app_handle: AppHandle,
    file_path: String,
    subtitle_path: Option<String>,
    import_job_id: Option<String>,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    let src_path = PathBuf::from(&file_path);
    if !src_path.is_file() {
        return Err("Source file does not exist".to_string());
    }
    let file_name = src_path
        .file_name()
        .ok_or("Invalid file name")?
        .to_string_lossy()
        .into_owned();
    let ext = src_path
        .extension()
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "mp4".to_string());
    let is_audio = is_audio_file_extension(&ext);
    let source_kind = if is_audio { "audio" } else { "video" };
    let source = MaterialImportSource {
        source_kind: source_kind.to_string(),
        source_uri: Some(format!("file://{file_path}")),
        content: None,
        file_path: Some(src_path.clone()),
        file_id: None,
        title: Some(file_name.clone()),
        metadata: serde_json::json!({
            "source": "desktop_import",
            "subtitle_path": subtitle_path.clone(),
        }),
    };
    let import_handle = app_handle.clone();
    run_material_import(
        &app_handle,
        source,
        ImportJobOptions {
            import_job_id,
            duplicate_policy,
        },
        || async move {
            let backend_client = backend_client_for_app(&import_handle)?;
            let app_data_dir = import_handle
                .path()
                .app_data_dir()
                .map_err(|error| format!("Failed to get app data dir: {error}"))?;
            let videos_dir = app_data_dir.join("videos");
            std::fs::create_dir_all(&videos_dir)
                .map_err(|error| format!("Failed to create videos dir: {error}"))?;
            let id = Uuid::new_v4().to_string();
            let dest_path = videos_dir.join(format!("{id}.{ext}"));
            std::fs::copy(&src_path, &dest_path)
                .map_err(|error| format!("Failed to copy file: {error}"))?;
            let uploaded_file = backend_client
                .upload_file_path(
                    &src_path,
                    Some(serde_json::json!({
                        "kind": source_kind,
                        "source": "desktop_import",
                        "cached_path": dest_path.to_string_lossy(),
                    })),
                )
                .await
                .map_err(backend_error_to_string)?;
            let content = if is_audio {
                format!("[Audio Import] {file_name}")
            } else {
                format!("[Local Import] {file_name}")
            };
            let mut article = Article {
                id: id.clone(),
                title: file_name,
                content,
                source_type: Some(if is_audio {
                    "audio".to_string()
                } else {
                    "local_video".to_string()
                }),
                source_url: Some(format!("file://{file_path}")),
                media_path: Some(dest_path.to_string_lossy().into_owned()),
                book_path: None,
                book_type: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                translated: false,
                active_mind_map_artifact_id: None,
                segments: Vec::new(),
                metadata: serde_json::json!({}),
                tags: Vec::new(),
                reading_progress: None,
                archived_at: None,
            };
            if let Some(subtitle_path) = subtitle_path {
                let subtitle_path = PathBuf::from(subtitle_path);
                import_subtitles_into_article(&mut article, &subtitle_path)?;
                backend_client
                    .upload_file_path(
                        &subtitle_path,
                        Some(serde_json::json!({
                            "kind": "subtitle",
                            "source": "desktop_import",
                            "material_id": article.id,
                        })),
                    )
                    .await
                    .map_err(backend_error_to_string)?;
            }
            Ok(PreparedMaterialImport {
                article,
                metadata: Some(serde_json::json!({
                    "backend_file_id": uploaded_file.id,
                    "backend_download_url": uploaded_file.download_url,
                    "original_name": uploaded_file.original_name,
                    "cached_path": dest_path.to_string_lossy(),
                })),
                file_sha256: Some(uploaded_file.sha256),
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn import_article_subtitles_cmd(
    app_handle: AppHandle,
    article_id: String,
    subtitle_path: String,
    import_job_id: Option<String>,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    let is_replay = import_job_id.is_some();
    let backend_client = backend_client_for_app(&app_handle)?;
    let mut article = get_article(app_handle.clone(), article_id.clone()).await?;

    if article.media_path.is_none() {
        return Err("仅媒体素材支持导入字幕".to_string());
    }

    let subtitle_path = PathBuf::from(&subtitle_path);
    let file_hash = file_sha256(&subtitle_path)?;
    let source_uri = format!("file://{}", subtitle_path.to_string_lossy());
    let job_id = import_job_id
        .as_deref()
        .ok_or_else(|| "subtitle attachment requires a confirmed import preview".to_string())?;
    validate_uuid(job_id, "import_job_id")?;
    let mut job = backend_client
        .get_material_import_job(job_id)
        .await
        .map_err(backend_error_to_string)?;
    if job.source_kind != "subtitle" {
        return Err("import_job_id belongs to a different source_kind".to_string());
    }
    if job.input_hash.as_deref() != Some(file_hash.as_str())
        || job.source_uri.as_deref() != Some(source_uri.as_str())
        || job
            .metadata
            .pointer("/resume_payload/target_material_id")
            .and_then(serde_json::Value::as_str)
            != Some(article_id.as_str())
        || job
            .metadata
            .pointer("/resume_payload/mode")
            .and_then(serde_json::Value::as_str)
            != Some("attach")
    {
        return Err("subtitle attachment does not match the confirmed preview".to_string());
    }
    let effective_duplicate_policy = material_import_effective_duplicate_policy(
        duplicate_policy.as_deref(),
        material_import_recorded_duplicate_policy(&job.metadata).as_deref(),
    )?;
    let policy = parse_duplicate_policy(effective_duplicate_policy.as_deref())?;
    if let Some(policy) = policy {
        let mut commit = material_import_commit_state_from_metadata(&job.metadata);
        if commit.duplicate_policy.as_deref() != Some(policy.as_str()) {
            commit.duplicate_policy = Some(policy.as_str().to_string());
            job = persist_import_job_commit_state(&backend_client, &job, &commit)
                .await
                .map_err(backend_error_to_string)?;
        }
    }
    if let Some(article) =
        recover_completed_import_side_effect(&backend_client, job.clone()).await?
    {
        return Ok(article);
    }
    if metadata_has_subtitle_hash(&article.metadata, &file_hash) {
        if job.status == "failed_retryable"
            && job.error_code.as_deref() == Some("interrupted_commit")
        {
            let mut commit = import_commit_state(policy, "subtitle_attach", article.id.clone());
            commit.result_material_id = Some(article.id.clone());
            job = begin_import_job_commit(&backend_client, job, &commit)
                .await
                .map_err(backend_error_to_string)?;
            finish_import_job(&backend_client, &job.id, &article.id)
                .await
                .map_err(backend_error_to_string)?;
            return Ok(article);
        }
        let _ = backend_client.cancel_material_import_job(&job.id).await;
        return Err("The same subtitle file is already attached to this material".to_string());
    }
    if let Err(error) = import_subtitles_into_article(&mut article, &subtitle_path) {
        mark_import_job_failed(
            &backend_client,
            &job.id,
            "subtitle_parse_failed",
            &error,
            is_replay,
        )
        .await;
        return Err(error);
    }
    let mut commit = import_commit_state(policy, "subtitle_attach", article.id.clone());
    job = begin_import_job_commit(&backend_client, job, &commit)
        .await
        .map_err(backend_error_to_string)?;
    let uploaded_file = match backend_client
        .upload_file_path(
            &subtitle_path,
            Some(serde_json::json!({
                "kind": "subtitle",
                "source": "desktop_import",
                "material_id": article_id,
            })),
        )
        .await
    {
        Ok(file) => file,
        Err(error) => {
            let message = error.to_string();
            mark_import_job_failed(
                &backend_client,
                &job.id,
                "subtitle_upload_failed",
                &message,
                is_replay || retryable_backend_error(&error),
            )
            .await;
            return Err(backend_error_to_string(error));
        }
    };
    if !article.metadata.is_object() {
        article.metadata = serde_json::json!({});
    }
    let metadata = article
        .metadata
        .as_object_mut()
        .expect("article metadata was normalized to an object");
    metadata.insert(
        "subtitle_file".to_string(),
        serde_json::json!({
            "backend_file_id": uploaded_file.id,
            "backend_download_url": uploaded_file.download_url,
            "original_name": uploaded_file.original_name,
            "sha256": uploaded_file.sha256,
        }),
    );
    metadata.insert(
        "import_job_id".to_string(),
        serde_json::json!(job.id.clone()),
    );
    let updated = match replace_backend_article(&app_handle, &article).await {
        Ok(article) => article,
        Err(error) => {
            mark_import_job_failed(
                &backend_client,
                &job.id,
                "subtitle_commit_failed",
                &error,
                is_replay || retryable_import_message(&error),
            )
            .await;
            return Err(error);
        }
    };
    commit.result_material_id = Some(updated.id.clone());
    let _ = persist_import_job_commit_state(&backend_client, &job, &commit)
        .await
        .map_err(backend_error_to_string)?;
    finish_import_job(&backend_client, &job.id, &updated.id)
        .await
        .map_err(backend_error_to_string)?;
    Ok(updated)
}

#[tauri::command]
pub async fn import_srt_file_cmd(
    app_handle: AppHandle,
    file_path: String,
    title: Option<String>,
    import_job_id: Option<String>,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    let source_path = PathBuf::from(&file_path);
    let preview_article = create_article_from_srt(&source_path, title.clone())?;
    let source = MaterialImportSource {
        source_kind: "subtitle".to_string(),
        source_uri: Some(format!("file://{file_path}")),
        content: Some(preview_article.content.clone()),
        file_path: Some(source_path.clone()),
        file_id: None,
        title: Some(preview_article.title),
        metadata: serde_json::json!({ "subtitle_type": "srt" }),
    };
    let prepare_app = app_handle.clone();
    run_material_import(
        &app_handle,
        source,
        ImportJobOptions {
            import_job_id,
            duplicate_policy,
        },
        || async move {
            let backend_client = backend_client_for_app(&prepare_app)?;
            let article = create_article_from_srt(&source_path, title)?;
            let uploaded_file = backend_client
                .upload_file_path(
                    &source_path,
                    Some(serde_json::json!({
                        "kind": "subtitle",
                        "source": "desktop_import",
                        "material_id": article.id,
                    })),
                )
                .await
                .map_err(backend_error_to_string)?;
            Ok(PreparedMaterialImport {
                article,
                metadata: Some(serde_json::json!({
                    "backend_file_id": uploaded_file.id,
                    "backend_download_url": uploaded_file.download_url,
                    "original_name": uploaded_file.original_name,
                    "subtitle_type": "srt",
                })),
                file_sha256: Some(uploaded_file.sha256),
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn prepare_ktv_segments_cmd(
    app_handle: AppHandle,
    article_id: String,
    language_hint: Option<String>,
) -> Result<Article, String> {
    let article = get_article(app_handle.clone(), article_id.clone()).await?;

    let prepared = prepare_ktv_segments(article, language_hint.as_deref())?;
    replace_backend_article(&app_handle, &prepared).await
}

#[tauri::command]
pub async fn export_ktv_video_cmd(
    app_handle: AppHandle,
    article_id: String,
    output_path: String,
    config: KtvExportConfig,
) -> Result<KtvExportResult, String> {
    require_external_tools_enabled("export_ktv_video_cmd")?;

    let article = get_article(app_handle.clone(), article_id).await?;

    export_ktv_video(
        &app_handle,
        &article,
        &config,
        std::path::Path::new(&output_path),
    )
    .await
}

// 字幕提取
/// 提取视频字幕
/// 使用 Gemini 多模态 API 从视频中提取音频并转录为字幕
#[tauri::command]
pub async fn extract_subtitles_cmd(
    app_handle: AppHandle,
    article_id: String,
    transcription_config_id: Option<String>,
) -> Result<Article, String> {
    require_external_tools_enabled("extract_subtitles_cmd")?;

    println!(
        "[ExtractSubtitles] 开始提取字幕: {} (transcription_config_id={:?})",
        article_id, transcription_config_id
    );

    // 1. 加载文章
    let mut article = get_article(app_handle.clone(), article_id.clone()).await?;

    // 2. 验证是视频并获取视频路径
    let video_path = article
        .media_path
        .as_ref()
        .ok_or("该文章不是视频，无法提取字幕")?;
    let video_path = std::path::Path::new(video_path);

    if !video_path.exists() {
        return Err(format!("视频文件不存在: {:?}", video_path));
    }

    // 3. 获取 API 配置
    let mut config = load_config(&app_handle)?.ok_or("未配置 API，请先在设置中配置 AI 模型")?;

    // 旧的 Gemini/Kimi「LLM 听写」回退路径解析（沿用激活对话模型 + 白名单校验）
    let resolve_llm = |config: &crate::types::AppConfig| -> Result<(String, String, String, Option<String>, bool), String> {
        let active_config = config.get_active_config().ok_or(
            "未配置字幕转写模型。请在 设置 → 字幕转写 添加一个转写模型（推荐 302ai whisper-1），或切换到 Gemini/Kimi 模型。",
        )?;
        let provider = active_config.api_provider.clone();
        let model = active_config.model.clone();
        if provider == "ollama" || provider == "lmstudio" {
            return Err(
                "字幕提取暂不支持 Ollama / LM Studio 本地模型。请在 设置 → 字幕转写 配置转写模型，或切换到 Gemini/Kimi。"
                    .to_string(),
            );
        }
        let is_supported = model.contains("gemini")
            || model.starts_with("google/gemini")
            || provider == "google"
            || provider == "google-ai-studio"
            || (is_moonshot_provider(&provider) && model.contains("kimi"))
            || model.contains("kimi");
        if !is_supported {
            return Err(
                "未配置字幕转写模型。请在 设置 → 字幕转写 添加一个转写模型（推荐 302ai whisper-1），或切换到 Gemini/Kimi 模型。"
                    .to_string(),
            );
        }
        Ok((provider, active_config.api_key.clone(), model, active_config.base_url.clone(), false))
    };

    // "__llm__" 哨兵 = 强制走旧的 Gemini/Kimi 听写；真实 id = 指定 ASR 配置；None = 默认。
    const LLM_SENTINEL: &str = "__llm__";
    let force_llm = transcription_config_id.as_deref() == Some(LLM_SENTINEL);
    let explicit_asr_id: Option<String> = match transcription_config_id.as_deref() {
        Some(LLM_SENTINEL) | None => None,
        Some(id) if id.is_empty() => None,
        Some(id) => Some(id.to_string()),
    };

    let (provider, api_key, model, base_url, use_asr): (
        String,
        String,
        String,
        Option<String>,
        bool,
    ) = if let Some(id) = explicit_asr_id {
        // 菜单显式选了某个转写模型
        let asr = config
            .asr_configs
            .iter()
            .find(|c| c.id == id)
            .ok_or("所选转写模型不存在，请在 设置 → 字幕转写 重新选择。")?;
        let picked = (
            asr.api_provider.clone(),
            asr.api_key.clone(),
            asr.model.clone(),
            asr.base_url.clone(),
            true,
        );
        // 记为默认（设为激活）并落盘
        if config.active_asr_model_id.as_deref() != Some(id.as_str()) {
            config.active_asr_model_id = Some(id.clone());
            if let Err(e) = save_config(&app_handle, &config) {
                println!("[ExtractSubtitles] 保存激活转写配置失败: {}", e);
            }
        }
        picked
    } else if force_llm {
        resolve_llm(&config)?
    } else if let Some(asr) = config.get_active_asr_config() {
        (
            asr.api_provider.clone(),
            asr.api_key.clone(),
            asr.model.clone(),
            asr.base_url.clone(),
            true,
        )
    } else {
        resolve_llm(&config)?
    };

    // 4. 调用字幕提取模块 (使用 article_id 作为 event_id)
    let segments = crate::subtitle_extraction::extract_subtitles(
        app_handle.clone(),
        video_path,
        &article_id,
        &provider,
        &api_key,
        &model,
        base_url.as_deref(),
        use_asr,
        &article_id, // event_id 用于进度事件
    )
    .await?;

    if segments.is_empty() {
        return Err("未能从视频中提取到字幕内容".to_string());
    }

    println!("[ExtractSubtitles] 提取到 {} 个字幕片段", segments.len());

    // 5. 更新文章内容
    article.segments = segments;
    article.content = article
        .segments
        .iter()
        .map(|s| s.text.clone())
        .collect::<Vec<_>>()
        .join(" ");

    let updated = replace_backend_article(&app_handle, &article).await?;

    println!("[ExtractSubtitles] 字幕提取完成并保存");

    Ok(updated)
}

// ============================================================================
// 书籍导入功能 - 支持 EPUB、TXT 和 PDF 格式
// ============================================================================

const BOOKS_DIR: &str = "books";

fn is_audio_file_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "mp3" | "wav" | "m4a" | "aac" | "flac" | "ogg" | "wma"
    )
}

fn is_text_import_extension(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "md" | "markdown" | "txt" | "docx"
    )
}

fn normalize_imported_text(content: String) -> String {
    content
        .trim_start_matches('\u{feff}')
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .trim()
        .to_string()
}

fn extract_docx_text(path: &Path) -> Result<String, String> {
    use std::io::Read;

    let file = std::fs::File::open(path).map_err(|e| format!("读取 DOCX 文件失败: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("解析 DOCX 压缩包失败: {}", e))?;
    let mut document = archive
        .by_name("word/document.xml")
        .map_err(|_| "DOCX 文件缺少 word/document.xml".to_string())?;
    let mut xml = String::new();
    document
        .read_to_string(&mut xml)
        .map_err(|e| format!("读取 DOCX 正文失败: {}", e))?;

    let paragraph_re = regex::Regex::new(r#"(?s)<w:p\b[^>]*>(.*?)</w:p>"#)
        .map_err(|e| format!("DOCX 段落解析器初始化失败: {}", e))?;
    let text_re = regex::Regex::new(r#"(?s)<w:t\b[^>]*>(.*?)</w:t>"#)
        .map_err(|e| format!("DOCX 文本解析器初始化失败: {}", e))?;
    let tab_re = regex::Regex::new(r#"<w:tab\s*/>"#)
        .map_err(|e| format!("DOCX 制表符解析器初始化失败: {}", e))?;

    let mut paragraphs = Vec::new();
    for paragraph in paragraph_re.captures_iter(&xml) {
        let Some(paragraph_xml) = paragraph.get(1).map(|capture| capture.as_str()) else {
            continue;
        };
        // Preserve tab position when collecting text nodes below.
        let paragraph_xml = tab_re.replace_all(paragraph_xml, "<w:t> </w:t>");
        let text = text_re
            .captures_iter(&paragraph_xml)
            .filter_map(|capture| capture.get(1).map(|value| value.as_str()))
            .map(|text| html_escape::decode_html_entities(text).to_string())
            .collect::<Vec<_>>()
            .join("");
        let text = text.trim();
        if !text.is_empty() {
            paragraphs.push(text.to_string());
        }
    }

    if paragraphs.is_empty() {
        return Err("未能从 DOCX 文件中提取到正文".to_string());
    }

    Ok(paragraphs.join("\n\n"))
}

fn read_text_import_content(path: &Path, ext: &str) -> Result<String, String> {
    let content = match ext.to_lowercase().as_str() {
        "md" | "markdown" | "txt" => {
            std::fs::read_to_string(path).map_err(|e| format!("读取文本文件失败: {}", e))?
        }
        "docx" => extract_docx_text(path)?,
        _ => return Err(format!("不支持的文本文件格式: {}", ext)),
    };
    let content = normalize_imported_text(content);
    if content.trim().is_empty() {
        return Err("文本文件内容为空".to_string());
    }
    Ok(content)
}

#[cfg(test)]
mod text_import_tests {
    use super::*;
    use std::io::Write;

    fn temp_path(extension: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "openkoto-text-import-{}.{}",
            Uuid::new_v4(),
            extension
        ))
    }

    #[test]
    fn reads_utf8_bom_and_normalizes_crlf_for_md_and_txt() {
        for extension in ["md", "txt"] {
            let path = temp_path(extension);
            std::fs::write(&path, b"\xEF\xBB\xBF  # Title\r\n\r\nBody\r")
                .expect("write temporary text file");

            let content = read_text_import_content(&path, extension).expect("read text file");
            assert_eq!(content, "# Title\n\nBody");
            std::fs::remove_file(path).expect("remove temporary text file");
        }
    }

    #[test]
    fn rejects_empty_text_with_clear_error() {
        let path = temp_path("txt");
        std::fs::write(&path, b"\xEF\xBB\xBF \r\n\t").expect("write temporary empty file");

        let error = read_text_import_content(&path, "txt").expect_err("empty text must fail");
        assert_eq!(error, "文本文件内容为空");
        std::fs::remove_file(path).expect("remove temporary empty file");
    }

    #[test]
    fn rejects_corrupted_docx_with_clear_error() {
        let path = temp_path("docx");
        std::fs::write(&path, b"not a zip archive").expect("write corrupted docx");

        let error = read_text_import_content(&path, "docx").expect_err("corrupted docx must fail");
        assert!(
            error.starts_with("解析 DOCX 压缩包失败:"),
            "unexpected error: {error}"
        );
        std::fs::remove_file(path).expect("remove corrupted docx");
    }

    #[test]
    fn extracts_body_from_minimal_valid_docx() {
        let path = temp_path("docx");
        let file = std::fs::File::create(&path).expect("create temporary docx");
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file(
                "word/document.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .expect("create document entry");
        archive
            .write_all(
                br#"<?xml version="1.0" encoding="UTF-8"?>
                    <w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
                      <w:body>
                        <w:p><w:r><w:t>Hello &amp; world</w:t><w:tab/><w:t>again</w:t></w:r></w:p>
                        <w:p><w:r><w:t>Second paragraph</w:t></w:r></w:p>
                      </w:body>
                    </w:document>"#,
            )
            .expect("write document xml");
        archive.finish().expect("finish temporary docx");

        let content = read_text_import_content(&path, "docx").expect("read valid docx");
        assert_eq!(content, "Hello & world again\n\nSecond paragraph");
        std::fs::remove_file(path).expect("remove temporary docx");
    }
}

#[tauri::command]
pub async fn import_text_file_cmd(
    app_handle: AppHandle,
    file_path: String,
    title: Option<String>,
    import_job_id: Option<String>,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    let src_path = PathBuf::from(&file_path);

    if !src_path.exists() {
        return Err(format!("文件不存在: {}", file_path));
    }

    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or("无法识别文件格式")?;

    if !is_text_import_extension(&ext) {
        return Err(format!("不支持的文本文件格式: {}", ext));
    }

    let content = read_text_import_content(&src_path, &ext)?;
    let file_name = src_path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("未命名文本");
    let article_title = title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| file_name.to_string());
    let source_uri = format!("file://{file_path}");
    let source = MaterialImportSource {
        source_kind: "text_file".to_string(),
        source_uri: Some(source_uri.clone()),
        content: Some(content.clone()),
        file_path: Some(src_path.clone()),
        file_id: None,
        title: Some(article_title.clone()),
        metadata: serde_json::json!({ "text_file_type": ext.clone() }),
    };
    let prepare_app = app_handle.clone();
    run_material_import(
        &app_handle,
        source,
        ImportJobOptions {
            import_job_id,
            duplicate_policy,
        },
        || async move {
            let backend_client = backend_client_for_app(&prepare_app)?;
            let uploaded_file = backend_client
                .upload_file_path(
                    &src_path,
                    Some(serde_json::json!({
                        "kind": "text_file",
                        "text_file_type": ext,
                        "source": "desktop_import",
                    })),
                )
                .await
                .map_err(backend_error_to_string)?;
            let id = Uuid::new_v4().to_string();
            let article = Article {
                id: id.clone(),
                title: article_title,
                content: content.clone(),
                source_type: Some("text_file".to_string()),
                source_url: Some(source_uri),
                media_path: None,
                book_path: None,
                book_type: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                translated: false,
                active_mind_map_artifact_id: None,
                segments: create_segments_from_content(&id, &content),
                metadata: serde_json::json!({}),
                tags: Vec::new(),
                reading_progress: None,
                archived_at: None,
            };
            Ok(PreparedMaterialImport {
                article,
                metadata: Some(serde_json::json!({
                    "backend_file_id": uploaded_file.id,
                    "backend_download_url": uploaded_file.download_url,
                    "original_name": uploaded_file.original_name,
                    "text_file_type": ext,
                })),
                file_sha256: Some(uploaded_file.sha256),
            })
        },
    )
    .await
}

/// 确保书籍存储目录存在
fn ensure_books_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取应用数据目录失败: {}", e))?;

    let books_dir = app_data_dir.join(BOOKS_DIR);
    if !books_dir.exists() {
        std::fs::create_dir_all(&books_dir).map_err(|e| format!("创建书籍目录失败: {}", e))?;
    }

    Ok(books_dir)
}

/// 导入书籍文件 (EPUB/TXT/PDF)
/// 将文件复制到应用数据目录并创建 Article 记录
#[tauri::command]
pub async fn import_book_cmd(
    app_handle: AppHandle,
    file_path: String,
    title: Option<String>,
    import_job_id: Option<String>,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    let src_path = PathBuf::from(&file_path);

    // 验证文件存在
    if !src_path.exists() {
        return Err(format!("文件不存在: {}", file_path));
    }

    // 获取文件扩展名并验证格式
    let ext = src_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .ok_or("无法识别文件格式")?;

    let book_type = match ext.as_str() {
        "epub" => "epub",
        "txt" => "txt",
        "pdf" => "pdf",
        _ => return Err(format!("不支持的文件格式: {}", ext)),
    };

    // 获取文件名作为默认标题
    let file_name = src_path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("未命名书籍");

    let book_title = title
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| file_name.to_string());
    let source_uri = format!("file://{file_path}");
    let source = MaterialImportSource {
        source_kind: "book".to_string(),
        source_uri: Some(source_uri.clone()),
        content: None,
        file_path: Some(src_path.clone()),
        file_id: None,
        title: Some(book_title.clone()),
        metadata: serde_json::json!({ "book_type": book_type }),
    };
    let prepare_app = app_handle.clone();
    let book_type = book_type.to_string();
    run_material_import(
        &app_handle,
        source,
        ImportJobOptions {
            import_job_id,
            duplicate_policy,
        },
        || async move {
            let backend_client = backend_client_for_app(&prepare_app)?;
            let books_dir = ensure_books_dir(&prepare_app)?;
            let id = Uuid::new_v4().to_string();
            let dest_path = books_dir.join(format!("{id}.{ext}"));
            std::fs::copy(&src_path, &dest_path)
                .map_err(|error| format!("复制文件失败: {error}"))?;
            let uploaded_file = backend_client
                .upload_file_path(
                    &src_path,
                    Some(serde_json::json!({
                        "kind": "book",
                        "book_type": book_type,
                        "source": "desktop_import",
                        "cached_path": dest_path.to_string_lossy(),
                    })),
                )
                .await
                .map_err(backend_error_to_string)?;
            let content = match book_type.as_str() {
                "txt" => std::fs::read_to_string(&dest_path)
                    .unwrap_or_else(|_| format!("[书籍已导入] {book_title}")),
                "epub" => format!("[EPUB 书籍] {book_title}"),
                "pdf" => format!("[PDF 书籍] {book_title}"),
                _ => format!("[书籍已导入] {book_title}"),
            };
            let article = Article {
                id,
                title: book_title,
                content,
                source_type: Some("book".to_string()),
                source_url: Some(source_uri),
                media_path: None,
                book_path: Some(dest_path.to_string_lossy().into_owned()),
                book_type: Some(book_type.clone()),
                created_at: chrono::Utc::now().to_rfc3339(),
                translated: false,
                active_mind_map_artifact_id: None,
                segments: Vec::new(),
                metadata: serde_json::json!({}),
                tags: Vec::new(),
                reading_progress: None,
                archived_at: None,
            };
            Ok(PreparedMaterialImport {
                article,
                metadata: Some(serde_json::json!({
                    "backend_file_id": uploaded_file.id,
                    "backend_download_url": uploaded_file.download_url,
                    "original_name": uploaded_file.original_name,
                    "book_type": book_type,
                    "cached_path": dest_path.to_string_lossy(),
                })),
                file_sha256: Some(uploaded_file.sha256),
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn import_web_material_cmd(
    app_handle: AppHandle,
    url: String,
    title: Option<String>,
    content: String,
    import_job_id: Option<String>,
    duplicate_policy: Option<String>,
) -> Result<Article, String> {
    require_external_tools_enabled("import_web_material_cmd")?;

    let parsed_url = url::Url::parse(&url).map_err(|_| "Invalid URL format".to_string())?;
    if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
        return Err("Only HTTP and HTTPS URLs are supported".to_string());
    }

    if content.trim().len() < 10 {
        return Err(
            "Extracted content is too short. Please check the URL and try again.".to_string(),
        );
    }

    let final_title = title.unwrap_or_else(|| "Untitled Web Material".to_string());
    let source = MaterialImportSource {
        source_kind: "url".to_string(),
        source_uri: Some(url.clone()),
        content: Some(content.clone()),
        file_path: None,
        file_id: None,
        title: Some(final_title.clone()),
        metadata: serde_json::json!({ "source": "web_import" }),
    };
    run_material_import(
        &app_handle,
        source,
        ImportJobOptions {
            import_job_id,
            duplicate_policy,
        },
        || async move {
            let id = Uuid::new_v4().to_string();
            Ok(PreparedMaterialImport {
                article: Article {
                    id: id.clone(),
                    title: final_title,
                    content: content.clone(),
                    source_type: Some("web".to_string()),
                    source_url: Some(url),
                    media_path: None,
                    book_path: None,
                    book_type: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                    translated: false,
                    active_mind_map_artifact_id: None,
                    segments: create_segments_from_content(&id, &content),
                    metadata: serde_json::json!({}),
                    tags: Vec::new(),
                    reading_progress: None,
                    archived_at: None,
                },
                metadata: None,
                file_sha256: None,
            })
        },
    )
    .await
}

// File System Commands
#[tauri::command]
pub async fn write_text_file(path: String, content: String) -> Result<(), String> {
    safe_file_io::write_text_export(&path, &content)
}

#[tauri::command]
pub async fn write_binary_file(path: String, content: Vec<u8>) -> Result<(), String> {
    safe_file_io::write_binary_export(&path, &content)
}

#[tauri::command]
pub async fn delete_article_subtitles_cmd(app_handle: AppHandle, id: String) -> Result<(), String> {
    let mut article = get_article(app_handle.clone(), id.clone()).await?;

    article.segments = Vec::new();
    article.translated = false;

    let _ = replace_backend_article(&app_handle, &article).await?;
    Ok(())
}

#[tauri::command]
pub async fn delete_article_analysis_cmd(app_handle: AppHandle, id: String) -> Result<(), String> {
    let mut article = get_article(app_handle.clone(), id.clone()).await?;

    for segment in &mut article.segments {
        segment.translation = None;
        segment.explanation = None;
    }
    article.translated = false;

    let _ = replace_backend_article(&app_handle, &article).await?;
    Ok(())
}

/// PDF全文翻译命令
/// 调用 Python PDF翻译插件进行翻译，生成纯译文和双语对照PDF
#[tauri::command]
pub async fn translate_pdf_document(
    app_handle: AppHandle,
    pdf_path: String,
    lang_in: String,
    lang_out: String,
    provider: String,
    api_key: String,
    model: String,
    base_url: Option<String>,
) -> Result<serde_json::Value, String> {
    require_external_tools_enabled("translate_pdf_document")?;

    use crate::logging::{self, LogLevel};
    use crate::pdf_sidecar;
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;

    let started_at = Instant::now();
    logging::log(
        LogLevel::Info,
        "pdf",
        "==================== PDF translation started ====================",
    );
    logging::log(
        LogLevel::Info,
        "pdf",
        format!(
            "request: lang_in={lang_in}, lang_out={lang_out}, provider={provider}, model={model}, base_url={}, api_key={}",
            base_url.as_deref().unwrap_or("<none>"),
            if api_key.trim().is_empty() {
                "<empty>".to_string()
            } else {
                format!("<set, {} chars>", api_key.trim().len())
            }
        ),
    );
    logging::log(LogLevel::Info, "pdf", format!("source pdf: {pdf_path}"));

    println!(
        "[PDF Translate] Starting translation: {} -> {}",
        lang_in, lang_out
    );
    println!("[PDF Translate] Provider: {}, Model: {}", provider, model);

    // 获取输出目录（与原PDF相同目录）
    let pdf_path_buf = PathBuf::from(&pdf_path);
    let output_dir = pdf_path_buf
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let filename_stem = pdf_path_buf
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".to_string());

    // The sidecar writes <stem>-mono.pdf / -dual.pdf into output_dir but does not
    // create it; make sure it exists so writing the result never fails.
    let _ = std::fs::create_dir_all(&output_dir);

    // 构建环境变量
    let mut envs: Vec<(&str, String)> = vec![
        ("OPENKOTO_PROVIDER", provider.clone()),
        ("OPENKOTO_API_KEY", api_key.clone()),
        ("OPENKOTO_MODEL", model.clone()),
    ];

    // Resolve the base URL with the same provider defaults used by the other AI
    // features, so providers whose URL is derived rather than stored (e.g.
    // Moonshot/Kimi) still work when the active model config has no explicit
    // base_url. Without this the sidecar receives no OPENKOTO_BASE_URL and dies
    // with `KeyError: 'OPENAI_BASE_URL'`.
    let resolved_base_url = base_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| default_base_url(&provider).map(ToOwned::to_owned));

    if let Some(url) = resolved_base_url {
        envs.push(("OPENKOTO_BASE_URL", url));
    }

    // Point the sidecar at bundled offline assets (DocLayout model + CJK fonts)
    // so the first translation doesn't block on a network download. Absent in
    // dev builds, in which case the sidecar falls back to on-demand download.
    if let Ok(resource_dir) = app_handle.path().resource_dir() {
        for candidate in ["resources/pdf-assets", "pdf-assets"] {
            let dir = resource_dir.join(candidate);
            if dir.join("models").is_dir() || dir.join("fonts").is_dir() {
                envs.push((
                    "OPENKOTO_OFFLINE_ASSETS_DIR",
                    dir.to_string_lossy().to_string(),
                ));
                break;
            }
        }
    }

    let sidecar = pdf_sidecar::resolve_pdf_sidecar(&app_handle)
        .map_err(|e| format!("PDF sidecar error: {e}"))?;
    let cmd = sidecar.program;
    let mut args = sidecar.args;
    let plugin_dir = sidecar.working_dir;

    args.extend([
        pdf_path.clone(),
        "-li".to_string(),
        lang_in,
        "-lo".to_string(),
        lang_out,
        "-s".to_string(),
        "openkoto".to_string(),
        "-o".to_string(),
        output_dir.clone(),
    ]);

    println!("[PDF Sidecar] Executing: {} {:?}", cmd, args);
    println!("[PDF Sidecar] CWD: {:?}", plugin_dir);
    logging::log(LogLevel::Info, "pdf", format!("spawn: {} {:?}", cmd, args));
    logging::log(LogLevel::Info, "pdf", format!("cwd: {:?}", plugin_dir));
    logging::log(
        LogLevel::Info,
        "pdf",
        format!(
            "offline assets: {}",
            envs.iter()
                .find(|(k, _)| *k == "OPENKOTO_OFFLINE_ASSETS_DIR")
                .map(|(_, v)| v.as_str())
                .unwrap_or("<none — sidecar may download on first run>")
        ),
    );

    // 在插件目录下执行，以确保 Python 模块导入正确 (如果是 Dev 模式)
    // 或者对于 Prod 模式，通常也不影响
    let mut command = Command::new(&cmd);
    command
        .args(&args)
        .envs(envs.iter().map(|(k, v)| (*k, v.as_str())))
        .current_dir(&plugin_dir) // 关键：设置工作目录为插件目录
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    pdf_sidecar::hide_console_window(&mut command); // Windows: 避免弹出黑色 cmd 窗口

    // Stream the sidecar output instead of blocking on .output(): this lets us
    // forward per-page progress to the UI and log lines to the console live,
    // so a long translation no longer looks frozen.
    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to execute PDF sidecar '{}': {}", cmd, e))?;

    let child_stdout = child
        .stdout
        .take()
        .ok_or_else(|| "PDF sidecar stdout unavailable".to_string())?;
    let child_stderr = child
        .stderr
        .take()
        .ok_or_else(|| "PDF sidecar stderr unavailable".to_string())?;

    // Shared "last time the sidecar said anything" clock. A stall (the classic
    // "stuck at 50%") shows up as a long gap with no new line; the watchdog
    // below turns that silent gap into an explicit warning in the log.
    let last_activity = Arc::new(Mutex::new(Instant::now()));

    // stdout: parse progress markers -> emit events + console log; pass the rest through.
    let app_for_stdout = app_handle.clone();
    let activity_stdout = last_activity.clone();
    let stdout_handle = std::thread::spawn(move || {
        let reader = BufReader::new(child_stdout);
        for line in reader.lines().map_while(Result::ok) {
            *activity_stdout.lock().unwrap() = Instant::now();
            if let Some(rest) = line.strip_prefix("OPENKOTO_PROGRESS ") {
                match serde_json::from_str::<serde_json::Value>(rest) {
                    Ok(payload) => {
                        let current = payload.get("current").and_then(|v| v.as_i64()).unwrap_or(0);
                        let total = payload.get("total").and_then(|v| v.as_i64()).unwrap_or(0);
                        let percent = payload.get("percent").and_then(|v| v.as_i64()).unwrap_or(0);
                        println!(
                            "[PDF Translate] progress {}/{} ({}%)",
                            current, total, percent
                        );
                        logging::log(
                            LogLevel::Info,
                            "pdf",
                            format!("progress: page {current}/{total} ({percent}%)"),
                        );
                        let _ = app_for_stdout.emit("pdf-translation-progress", payload);
                    }
                    Err(_) => {
                        println!("[PDF Sidecar] {}", line);
                        logging::log(LogLevel::Info, "python", format!("[stdout] {line}"));
                    }
                }
            } else {
                println!("[PDF Sidecar] {}", line);
                logging::log(LogLevel::Info, "python", format!("[stdout] {line}"));
            }
        }
    });

    // stderr: log live and accumulate so a failure still surfaces a useful message.
    // The Python side writes its detailed per-page / per-paragraph trace here.
    let activity_stderr = last_activity.clone();
    let stderr_handle = std::thread::spawn(move || {
        let reader = BufReader::new(child_stderr);
        let mut collected = String::new();
        for line in reader.lines().map_while(Result::ok) {
            *activity_stderr.lock().unwrap() = Instant::now();
            eprintln!("[PDF Sidecar:err] {}", line);
            let lower = line.to_ascii_lowercase();
            let level = if lower.contains("traceback")
                || lower.contains("error")
                || lower.contains("exception")
                || lower.contains("failed")
            {
                LogLevel::Error
            } else if lower.contains("retry") || lower.contains("warn") {
                LogLevel::Warn
            } else {
                LogLevel::Info
            };
            logging::log(level, "python", line.clone());
            collected.push_str(&line);
            collected.push('\n');
        }
        collected
    });

    // Watchdog: if the sidecar goes quiet for too long, say so explicitly so a
    // hang is visible in the log instead of just a frozen progress bar.
    let watchdog_done = Arc::new(AtomicBool::new(false));
    let watchdog_flag = watchdog_done.clone();
    let activity_watchdog = last_activity.clone();
    let watchdog_handle = std::thread::spawn(move || {
        const STALL_WARN_SECS: u64 = 20;
        let mut warned_at = 0u64;
        while !watchdog_flag.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_secs(5));
            if watchdog_flag.load(Ordering::Relaxed) {
                break;
            }
            let idle = activity_watchdog.lock().unwrap().elapsed().as_secs();
            if idle >= STALL_WARN_SECS && idle != warned_at {
                warned_at = idle;
                logging::log(
                    LogLevel::Warn,
                    "pdf",
                    format!(
                        "no sidecar output for {idle}s — translation may be stalled (hung API call or retry loop?)"
                    ),
                );
            }
        }
    });

    let status = child
        .wait()
        .map_err(|e| format!("Failed to wait for PDF sidecar: {}", e))?;
    let _ = stdout_handle.join();
    let stderr_output = stderr_handle.join().unwrap_or_default();
    watchdog_done.store(true, Ordering::Relaxed);
    let _ = watchdog_handle.join();

    let elapsed_secs = started_at.elapsed().as_secs_f64();
    logging::log(
        LogLevel::Info,
        "pdf",
        format!(
            "sidecar exited: success={}, code={:?}, elapsed={:.1}s",
            status.success(),
            status.code(),
            elapsed_secs
        ),
    );

    if status.success() {
        // Settle the UI at 100% once the files are written.
        let _ = app_handle.emit(
            "pdf-translation-progress",
            serde_json::json!({"type": "progress", "current": 0, "total": 0, "percent": 100}),
        );

        let mono_path = format!("{}/{}-mono.pdf", output_dir, filename_stem);
        let dual_path = format!("{}/{}-dual.pdf", output_dir, filename_stem);

        logging::log(
            LogLevel::Info,
            "pdf",
            format!("translation OK in {elapsed_secs:.1}s -> {mono_path} / {dual_path}"),
        );

        Ok(serde_json::json!({
            "success": true,
            "mono_pdf": mono_path,
            "dual_pdf": dual_path,
            "original_pdf": pdf_path,
        }))
    } else {
        logging::log(
            LogLevel::Error,
            "pdf",
            format!(
                "translation FAILED (code={:?}). Tail of sidecar stderr:\n{}",
                status.code(),
                stderr_output
                    .lines()
                    .rev()
                    .take(20)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        );
        Err(format!("PDF translation failed: {}", stderr_output))
    }
}

#[derive(serde::Serialize)]
pub struct TranslationFiles {
    pub mono_path: Option<String>,
    pub dual_path: Option<String>,
}

#[tauri::command]
pub async fn check_pdf_translation_files(pdf_path: String) -> Result<TranslationFiles, String> {
    use std::path::Path;
    let path = Path::new(&pdf_path);
    if !path.exists() {
        return Ok(TranslationFiles {
            mono_path: None,
            dual_path: None,
        });
    }

    let parent = path.parent().unwrap_or(Path::new("."));

    // Safety check: ensure file stem exists
    let stem = match path.file_stem() {
        Some(s) => s.to_string_lossy(),
        None => {
            return Ok(TranslationFiles {
                mono_path: None,
                dual_path: None,
            })
        }
    };

    let mono_name = format!("{}-mono.pdf", stem);
    let dual_name = format!("{}-dual.pdf", stem);

    let mono_path = parent.join(&mono_name);
    let dual_path = parent.join(&dual_name);

    Ok(TranslationFiles {
        mono_path: if mono_path.exists() {
            Some(mono_path.to_string_lossy().into_owned())
        } else {
            None
        },
        dual_path: if dual_path.exists() {
            Some(dual_path.to_string_lossy().into_owned())
        } else {
            None
        },
    })
}

#[tauri::command]
pub async fn export_file_cmd(
    app_handle: AppHandle,
    src_path: String,
    dest_path: String,
) -> Result<(), String> {
    safe_file_io::copy_app_data_file_to_export(&app_handle, &src_path, &dest_path)
}

// ============================================================================
// Bookmarks Commands - 书签命令
// ============================================================================

/// 添加书签
#[tauri::command]
pub async fn add_bookmark_cmd(
    app_handle: AppHandle,
    book_path: String,
    book_type: String,
    title: String,
    note: Option<String>,
    selected_text: Option<String>,
    page_number: Option<i32>,
    epub_cfi: Option<String>,
    color: Option<String>,
) -> Result<Bookmark, String> {
    let bookmark = Bookmark {
        id: Uuid::new_v4().to_string(),
        book_path,
        book_type,
        title,
        note,
        selected_text,
        page_number,
        epub_cfi,
        created_at: chrono::Utc::now().to_rfc3339(),
        color,
    };

    backend_client_for_app(&app_handle)?
        .upsert_bookmark(&bookmark)
        .await
        .map_err(backend_error_to_string)
}

/// 列出所有书签
#[tauri::command]
pub async fn list_bookmarks_cmd(app_handle: AppHandle) -> Result<Vec<Bookmark>, String> {
    let mut bookmarks = backend_client_for_app(&app_handle)?
        .list_bookmarks()
        .await
        .map_err(backend_error_to_string)?;

    // 按创建时间降序排列
    bookmarks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(bookmarks)
}

/// 列出指定书籍的书签
#[tauri::command]
pub async fn list_bookmarks_for_book_cmd(
    app_handle: AppHandle,
    book_path: String,
) -> Result<Vec<Bookmark>, String> {
    let mut bookmarks = backend_client_for_app(&app_handle)?
        .list_bookmarks_for_book(&book_path)
        .await
        .map_err(backend_error_to_string)?;

    // 按创建时间降序排列
    bookmarks.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    Ok(bookmarks)
}

/// 更新书签
#[tauri::command]
pub async fn update_bookmark_cmd(
    app_handle: AppHandle,
    id: String,
    title: Option<String>,
    note: Option<String>,
    color: Option<String>,
) -> Result<Bookmark, String> {
    let client = backend_client_for_app(&app_handle)?;
    let mut bookmark = client
        .get_bookmark(&id)
        .await
        .map_err(backend_error_to_string)?;

    if let Some(t) = title {
        bookmark.title = t;
    }
    if let Some(n) = note {
        bookmark.note = Some(n);
    }
    if let Some(c) = color {
        bookmark.color = Some(c);
    }

    client
        .patch_bookmark(&id, &bookmark)
        .await
        .map_err(backend_error_to_string)
}

/// 删除书签
#[tauri::command]
pub async fn delete_bookmark_cmd(app_handle: AppHandle, id: String) -> Result<(), String> {
    backend_client_for_app(&app_handle)?
        .delete_bookmark(&id)
        .await
        .map_err(backend_error_to_string)
}

#[cfg(test)]
mod word_pack_import_tests {
    use super::*;

    #[test]
    fn export_builder_uses_pack_name_for_filename_and_sorts_entries() {
        let result = build_word_pack_export_result(
            WordPackExportMeta {
                name: "全部单词".to_string(),
                description: None,
                cover_url: None,
                author: None,
                language_from: None,
                language_to: None,
                tags: Vec::new(),
                version: Some("1.0.0".to_string()),
            },
            vec![
                WordPackExportEntry {
                    word: "zebra".to_string(),
                    meaning: "斑马".to_string(),
                    usage: None,
                    example: None,
                    reading: None,
                    explanation: None,
                    tags: Vec::new(),
                },
                WordPackExportEntry {
                    word: "apple".to_string(),
                    meaning: "苹果".to_string(),
                    usage: None,
                    example: None,
                    reading: None,
                    explanation: None,
                    tags: Vec::new(),
                },
            ],
        )
        .expect("should build export result");

        assert_eq!(result.file_name, "全部单词.okpack.json");
        assert!(
            result.json_content.find("apple").unwrap() < result.json_content.find("zebra").unwrap()
        );
    }

    #[test]
    fn import_parser_accepts_standard_pack_schema() {
        let json = r#"{
          "schema_version":"openkoto-word-pack-v1",
          "pack":{"name":"Core 100","description":"desc"},
          "entries":[{"word":"abandon","meaning":"放弃"}]
        }"#;

        let parsed = parse_import_word_pack_json(json).expect("should parse standard schema");
        assert_eq!(parsed.pack.name, "Core 100");
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].word, "abandon");
    }

    #[test]
    fn import_parser_rejects_legacy_array_schema() {
        let json = r#"[
          {"word":"abandon","meaning":"放弃","usage":"v."},
          {"word":"ability","meaning":"能力"}
        ]"#;

        let error =
            parse_import_word_pack_json(json).expect_err("legacy array schema should be rejected");
        assert!(error.contains("Invalid word pack JSON"));
    }

    #[test]
    fn import_parser_rejects_unknown_schema_version() {
        let json = r#"{
          "schema_version":"openkoto-word-pack-v2",
          "pack":{"name":"Core 100"},
          "entries":[{"word":"abandon","meaning":"放弃"}]
        }"#;

        let error =
            parse_import_word_pack_json(json).expect_err("unknown schema should be rejected");
        assert!(error.contains("Unsupported word pack schema_version"));
    }

    #[test]
    fn import_parser_accepts_json_with_bom() {
        let json = "\u{feff}{\"schema_version\":\"openkoto-word-pack-v1\",\"pack\":{\"name\":\"BOM Pack\"},\"entries\":[{\"word\":\"apple\",\"meaning\":\"苹果\"}]}";

        let parsed = parse_import_word_pack_json(json).expect("should parse BOM-prefixed json");
        assert_eq!(parsed.pack.name, "BOM Pack");
        assert_eq!(parsed.entries.len(), 1);
    }
}

#[cfg(test)]
mod builtin_agent_turn_tests {
    use super::*;

    fn material(id: &str, title: &str, material_type: &str) -> MaterialSummary {
        MaterialSummary {
            id: id.to_string(),
            title: title.to_string(),
            material_type: material_type.to_string(),
            created_at: "2026-07-02T00:00:00Z".to_string(),
            translated: false,
        }
    }

    #[test]
    fn builtin_agent_turn_reports_current_material() {
        let current = material("a1", "Current Article", "web");
        let payload = builtin_agent_turn_payload("查看当前素材", &current, &[current.clone()])
            .expect("current material prompt should be handled locally")
            .0;

        assert_eq!(payload["action"]["kind"], "get_current_material");
        assert!(payload["reply"]
            .as_str()
            .expect("reply should be a string")
            .contains("Current Article"));
    }

    #[test]
    fn builtin_agent_turn_lists_materials() {
        let current = material("a1", "Current Article", "web");
        let materials = vec![current.clone(), material("a2", "Second Article", "article")];
        let payload = builtin_agent_turn_payload("列出素材", &current, &materials)
            .expect("list materials prompt should be handled locally")
            .0;

        assert_eq!(payload["action"]["kind"], "list_materials");
        let reply = payload["reply"].as_str().expect("reply should be a string");
        assert!(reply.contains("Current Article"));
        assert!(reply.contains("Second Article"));
    }

    #[test]
    fn builtin_agent_turn_opens_matching_material() {
        let current = material("a1", "Current Article", "web");
        let materials = vec![current.clone(), material("a2", "Second Article", "article")];
        let (payload, open_id) =
            builtin_agent_turn_payload("打开素材 second", &current, &materials)
                .expect("open material prompt should be handled locally");

        assert_eq!(open_id.as_deref(), Some("a2"));
        assert_eq!(payload["action"]["kind"], "open_material");
        assert_eq!(payload["action"]["material_id"], "a2");
    }
}
