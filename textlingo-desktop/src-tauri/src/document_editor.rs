use serde::Serialize;
use serde_json::Value;
use tauri::AppHandle;

use crate::{
    app_config::service::{backend_client_for_app, backend_error_to_string},
    backend_client::{BackendClient, BackendClientError, PatchMaterialRequest},
    types::Article,
};
use uuid::Uuid;

pub(crate) fn article_patch_payload(article: &Article) -> PatchMaterialRequest {
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

fn import_replacement_blocks(document: &Value, content: &str) -> Vec<Value> {
    let existing = document
        .get("blocks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(index, text)| {
            let prior = existing.get(index);
            let attrs = prior
                .filter(|block| block.get("text").and_then(Value::as_str) == Some(text))
                .and_then(|block| block.get("attrs"))
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));
            serde_json::json!({
                "id": prior.and_then(|block| block.get("id")).cloned(),
                "block_type": "paragraph",
                "block_order": index,
                "text": text,
                "attrs": attrs,
            })
        })
        .collect()
}

fn document_matches_import_blocks(document: &Value, blocks: &[Value]) -> bool {
    let Some(existing) = document.get("blocks").and_then(Value::as_array) else {
        return blocks.is_empty();
    };
    existing.len() == blocks.len()
        && existing.iter().zip(blocks).all(|(left, right)| {
            left.get("block_type") == right.get("block_type")
                && left.get("text") == right.get("text")
                && left.get("attrs") == right.get("attrs")
        })
}

pub(crate) async fn replace_import_document(
    client: &BackendClient,
    material_id: &str,
    content: &str,
    import_job_id: &str,
) -> Result<(), BackendClientError> {
    let document = client.get_material_document(material_id).await?;
    let blocks = import_replacement_blocks(&document, content);
    if document_matches_import_blocks(&document, &blocks) {
        return Ok(());
    }
    let base_revision =
        document
            .get("current_revision")
            .cloned()
            .ok_or_else(|| BackendClientError::Backend {
                status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                code: "document_contract_error".to_string(),
                message: "material document is missing current_revision".to_string(),
            })?;
    let preview = client
        .preview_material_edit(
            material_id,
            &serde_json::json!({
                "base_revision": base_revision,
                "blocks": blocks,
            }),
        )
        .await?;
    let preview_token = preview
        .get("preview_token")
        .and_then(Value::as_str)
        .ok_or_else(|| BackendClientError::Backend {
            status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
            code: "document_contract_error".to_string(),
            message: "material edit preview is missing preview_token".to_string(),
        })?;
    client
        .commit_material_edit(
            material_id,
            &serde_json::json!({
                "base_revision": base_revision,
                "client_request_id": format!("material-import-replace:{import_job_id}"),
                "preview_token": preview_token,
                "blocks": blocks,
            }),
        )
        .await?;
    Ok(())
}

pub(crate) async fn replace_backend_media_subtitles_legacy(
    app_handle: &AppHandle,
    article: &Article,
) -> Result<Article, String> {
    if article.media_path.is_none() {
        return Err(
            "legacy full segment replacement is restricted to media subtitle maintenance"
                .to_string(),
        );
    }
    let client = backend_client_for_app(app_handle)?;
    client
        .patch_material(&article.id, &article_patch_payload(article))
        .await
        .map_err(backend_error_to_string)
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentCommandError {
    pub status: Option<u16>,
    pub code: String,
    pub message: String,
}

impl From<String> for DocumentCommandError {
    fn from(message: String) -> Self {
        Self {
            status: None,
            code: "document_backend_unavailable".to_string(),
            message,
        }
    }
}

fn document_backend_error(error: BackendClientError) -> DocumentCommandError {
    match error {
        BackendClientError::Backend {
            status,
            code,
            message,
        } => DocumentCommandError {
            status: Some(status.as_u16()),
            code,
            message,
        },
        BackendClientError::NotConfigured => DocumentCommandError {
            status: None,
            code: "backend_not_configured".to_string(),
            message:
                "Backend is required. Configure backend URL and sign in before editing materials."
                    .to_string(),
        },
        other => DocumentCommandError {
            status: None,
            code: "backend_request_failed".to_string(),
            message: other.to_string(),
        },
    }
}

fn sparse_material_metadata_payload(article: &Article) -> PatchMaterialRequest {
    PatchMaterialRequest {
        title: Some(article.title.clone()),
        content: None,
        source_type: article.source_type.clone(),
        source_url: article.source_url.clone(),
        media_path: article.media_path.clone(),
        book_path: article.book_path.clone(),
        book_type: article.book_type.clone(),
        translated: Some(article.translated),
        active_mind_map_artifact_id: article.active_mind_map_artifact_id.clone(),
        metadata: Some(article.metadata.clone()),
        segments: None,
    }
}

pub(crate) async fn persist_article_derived_fields(
    app_handle: &AppHandle,
    article: &Article,
) -> Result<Article, String> {
    let client = backend_client_for_app(app_handle)?;
    let updates = article
        .segments
        .iter()
        .map(|segment| {
            serde_json::json!({
                "segment_id": segment.id,
                "expected_text_sha256": segment.text_sha256,
                "reading_text": segment.reading_text,
                "translation": segment.translation,
                "explanation": segment.explanation,
            })
        })
        .collect::<Vec<_>>();
    if !updates.is_empty() {
        client
            .update_segment_derived(&article.id, &serde_json::json!({ "updates": updates }))
            .await
            .map_err(backend_error_to_string)?;
    }
    client
        .patch_material(&article.id, &sparse_material_metadata_payload(article))
        .await
        .map_err(backend_error_to_string)
}

pub(crate) async fn commit_article_content(
    app_handle: &AppHandle,
    article_id: &str,
    content: &str,
) -> Result<Article, String> {
    let client = backend_client_for_app(app_handle)?;
    let document = client
        .get_material_document(article_id)
        .await
        .map_err(backend_error_to_string)?;
    let current_revision = document
        .get("current_revision")
        .cloned()
        .ok_or_else(|| "material document is missing current_revision".to_string())?;
    let existing = document
        .get("blocks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let blocks = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .enumerate()
        .map(|(index, text)| {
            let prior = existing.get(index);
            serde_json::json!({
                "id": prior.and_then(|block| block.get("id")).cloned(),
                "block_type": prior
                    .and_then(|block| block.get("block_type"))
                    .and_then(Value::as_str)
                    .unwrap_or("paragraph"),
                "block_order": index,
                "text": text,
                "attrs": prior
                    .and_then(|block| block.get("attrs"))
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({})),
            })
        })
        .collect::<Vec<_>>();
    let preview = client
        .preview_material_edit(
            article_id,
            &serde_json::json!({
                "base_revision": current_revision,
                "blocks": blocks,
            }),
        )
        .await
        .map_err(backend_error_to_string)?;
    let preview_token = preview
        .get("preview_token")
        .and_then(Value::as_str)
        .ok_or_else(|| "material edit preview is missing preview_token".to_string())?;
    client
        .commit_material_edit(
            article_id,
            &serde_json::json!({
                "base_revision": current_revision,
                "client_request_id": Uuid::new_v4().to_string(),
                "preview_token": preview_token,
                "blocks": blocks,
            }),
        )
        .await
        .map_err(backend_error_to_string)?;
    client
        .get_material(article_id)
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn get_material_document_cmd(
    app_handle: AppHandle,
    material_id: String,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .get_material_document(&material_id)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn preview_material_edit_cmd(
    app_handle: AppHandle,
    material_id: String,
    payload: Value,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .preview_material_edit(&material_id, &payload)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn commit_material_edit_cmd(
    app_handle: AppHandle,
    material_id: String,
    payload: Value,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .commit_material_edit(&material_id, &payload)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn get_material_draft_cmd(
    app_handle: AppHandle,
    material_id: String,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .get_material_draft(&material_id)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn save_material_draft_cmd(
    app_handle: AppHandle,
    material_id: String,
    payload: Value,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .save_material_draft(&material_id, &payload)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn delete_material_draft_cmd(
    app_handle: AppHandle,
    material_id: String,
) -> Result<(), DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .delete_material_draft(&material_id)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn list_material_revisions_cmd(
    app_handle: AppHandle,
    material_id: String,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .list_material_revisions(&material_id)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn get_material_revision_cmd(
    app_handle: AppHandle,
    material_id: String,
    revision: i64,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .get_material_revision(&material_id, revision)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn restore_material_revision_cmd(
    app_handle: AppHandle,
    material_id: String,
    revision: i64,
    payload: Value,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .restore_material_revision(&material_id, revision, &payload)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn update_segment_derived_cmd(
    app_handle: AppHandle,
    material_id: String,
    payload: Value,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .update_segment_derived(&material_id, &payload)
        .await
        .map_err(document_backend_error)
}

#[tauri::command]
pub async fn create_editable_derivative_cmd(
    app_handle: AppHandle,
    material_id: String,
    payload: Option<Value>,
) -> Result<Value, DocumentCommandError> {
    backend_client_for_app(&app_handle)?
        .create_editable_derivative(
            &material_id,
            &payload.unwrap_or_else(|| serde_json::json!({})),
        )
        .await
        .map_err(document_backend_error)
}

#[cfg(test)]
mod tests {
    use reqwest::StatusCode;

    use super::*;

    #[test]
    fn preserves_revision_conflict_contract() {
        let error = document_backend_error(BackendClientError::Backend {
            status: StatusCode::CONFLICT,
            code: "material_revision_conflict".to_string(),
            message: "material revision changed".to_string(),
        });

        assert_eq!(error.status, Some(409));
        assert_eq!(error.code, "material_revision_conflict");
        assert_eq!(error.message, "material revision changed");
    }

    #[test]
    fn text_import_replacement_builds_document_blocks_without_legacy_segments() {
        let document = serde_json::json!({
            "blocks": [{
                "id": "block-1",
                "block_type": "heading",
                "text": "Old heading",
                "attrs": {"level": 2}
            }]
        });

        let blocks = import_replacement_blocks(&document, "New paragraph\n\nSecond paragraph");

        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0]["id"], "block-1");
        assert_eq!(blocks[0]["block_type"], "paragraph");
        assert_eq!(blocks[0]["attrs"], serde_json::json!({}));
        assert_eq!(blocks[1]["text"], "Second paragraph");
    }

    #[test]
    fn text_import_replacement_detects_an_already_committed_retry() {
        let document = serde_json::json!({
            "blocks": [{
                "id": "block-1",
                "block_type": "paragraph",
                "text": "Same paragraph",
                "attrs": {"source": "import"}
            }]
        });
        let blocks = import_replacement_blocks(&document, "Same paragraph");

        assert!(document_matches_import_blocks(&document, &blocks));
    }
}
