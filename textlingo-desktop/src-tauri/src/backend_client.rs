use std::{path::Path, time::Duration};

use reqwest::{multipart, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{
    AgentTask, AppConfig, Article, ArticleSegment, Artifact, Bookmark, BulkMaterialIdsRequest,
    BulkMaterialTagsRequest, BulkOperationResponse, CreateMaterialImportJobRequest,
    CreateMaterialTagRequest, DeleteResponse, DuplicateCheckRequest, DuplicateCheckResponse,
    FavoriteGrammar, FavoriteVocabulary, ListMaterialImportJobsQuery, ListMaterialsQuery,
    MaterialImportJob, MaterialTag, MergeMaterialTagRequest, PatchMaterialImportJobRequest,
    PatchMaterialTagRequest, ReadingProgress, SetMaterialTagsRequest, UpsertReadingProgressRequest,
    WordPack,
};

#[derive(Debug, Clone)]
pub struct BackendClient {
    client: Client,
    base_url: String,
    auth_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendClientConfig {
    pub base_url: String,
    pub auth_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendHealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub auth: BackendAuthHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendAuthHealth {
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendUser {
    pub id: String,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendAuthResponse {
    pub token_type: String,
    pub token: String,
    pub expires_at: String,
    pub user: BackendUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendCurrentUserResponse {
    pub user: BackendUser,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendFile {
    pub id: String,
    pub original_name: String,
    pub content_type: Option<String>,
    pub byte_size: i64,
    pub sha256: String,
    pub download_url: String,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMaterialRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub media_path: Option<String>,
    #[serde(default)]
    pub book_path: Option<String>,
    #[serde(default)]
    pub book_type: Option<String>,
    #[serde(default)]
    pub translated: Option<bool>,
    #[serde(default)]
    pub active_mind_map_artifact_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub segments: Option<Vec<ArticleSegment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyImportRequest {
    pub client_import_id: String,
    #[serde(default = "default_legacy_import_schema_version")]
    pub schema_version: String,
    #[serde(default)]
    pub source_label: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub config: Option<Value>,
    #[serde(default)]
    pub failed_items: Vec<LegacyImportFailedItem>,
    #[serde(default)]
    pub materials: Vec<LegacyImportItem<CreateMaterialRequest>>,
    #[serde(default)]
    pub word_packs: Vec<LegacyImportItem<WordPack>>,
    #[serde(default)]
    pub favorite_vocabularies: Vec<LegacyImportItem<FavoriteVocabulary>>,
    #[serde(default)]
    pub favorite_grammars: Vec<LegacyImportItem<FavoriteGrammar>>,
    #[serde(default)]
    pub bookmarks: Vec<LegacyImportItem<Bookmark>>,
    #[serde(default)]
    pub agent_tasks: Vec<LegacyImportItem<Value>>,
    #[serde(default)]
    pub artifacts: Vec<LegacyImportItem<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyImportItem<T> {
    pub source_id: String,
    pub payload: T,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyImportFailedItem {
    pub source_kind: String,
    pub source_id: String,
    pub error: String,
    #[serde(default)]
    pub payload: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyImportBatch {
    pub id: String,
    pub client_import_id: String,
    pub request_sha256: String,
    pub schema_version: String,
    pub source_label: Option<String>,
    pub status: String,
    pub total_items: i32,
    pub imported_items: i32,
    pub skipped_items: i32,
    pub failed_items: i32,
    pub metadata: Value,
    pub created_at: String,
    pub finished_at: Option<String>,
    pub items: Vec<LegacyImportItemResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegacyImportItemResult {
    pub id: String,
    pub source_kind: String,
    pub source_id: String,
    pub target_kind: Option<String>,
    pub target_id: Option<String>,
    pub status: String,
    pub error: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatchMaterialRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_type: Option<String>,
    #[serde(default)]
    pub translated: Option<bool>,
    #[serde(default)]
    pub active_mind_map_artifact_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub segments: Option<Vec<ArticleSegment>>,
}

const MATERIAL_SOURCE_PATCH_FIELDS: &[&str] = &[
    "source_type",
    "source_url",
    "media_path",
    "book_path",
    "book_type",
];

fn material_patch_request_body(
    payload: &PatchMaterialRequest,
    file_sha256: Option<&str>,
    clear_source_fields: bool,
) -> Value {
    let mut body = serde_json::to_value(payload)
        .expect("PatchMaterialRequest must serialize to a JSON object");
    let object = body
        .as_object_mut()
        .expect("PatchMaterialRequest must serialize to a JSON object");
    if let Some(file_sha256) = file_sha256 {
        object.insert(
            "file_sha256".to_string(),
            Value::String(file_sha256.to_string()),
        );
    }
    if clear_source_fields {
        for field in MATERIAL_SOURCE_PATCH_FIELDS {
            object.entry((*field).to_string()).or_insert(Value::Null);
        }
    }
    body
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningItem {
    pub id: String,
    #[serde(default)]
    pub material_id: Option<String>,
    #[serde(default)]
    pub segment_id: Option<String>,
    pub item_type: String,
    pub text: String,
    pub source_sentence: String,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    #[serde(default)]
    pub meaning_in_context: Option<String>,
    #[serde(default)]
    pub definition_en: Option<String>,
    #[serde(default)]
    pub definition_zh: Option<String>,
    #[serde(default)]
    pub collocations: Vec<Value>,
    #[serde(default)]
    pub examples: Vec<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub status: String,
    pub priority: i32,
    #[serde(default)]
    pub difficulty: Option<i32>,
    #[serde(default)]
    pub ai_explanation: Option<Value>,
    #[serde(default)]
    pub review_state: Value,
    #[serde(default)]
    pub source_material_title: Option<String>,
    #[serde(default)]
    pub source_segment_order: Option<i32>,
    #[serde(default)]
    pub accepted_at: Option<String>,
    #[serde(default)]
    pub rejected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeleteLearningItemResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListLearningItemsRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLearningItemRequest {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub material_id: Option<String>,
    #[serde(default)]
    pub segment_id: Option<String>,
    #[serde(default)]
    pub item_type: Option<String>,
    pub text: String,
    #[serde(default)]
    pub source_sentence: Option<String>,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    #[serde(default)]
    pub meaning_in_context: Option<String>,
    #[serde(default)]
    pub definition_en: Option<String>,
    #[serde(default)]
    pub definition_zh: Option<String>,
    #[serde(default)]
    pub collocations: Vec<Value>,
    #[serde(default)]
    pub examples: Vec<Value>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub difficulty: Option<i32>,
    #[serde(default)]
    pub ai_explanation: Option<Value>,
    #[serde(default)]
    pub review_state: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateLearningItemRequest {
    #[serde(default)]
    pub material_id: Option<String>,
    #[serde(default)]
    pub segment_id: Option<String>,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub source_sentence: Option<String>,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    #[serde(default)]
    pub meaning_in_context: Option<String>,
    #[serde(default)]
    pub definition_en: Option<String>,
    #[serde(default)]
    pub definition_zh: Option<String>,
    #[serde(default)]
    pub collocations: Option<Vec<Value>>,
    #[serde(default)]
    pub examples: Option<Vec<Value>>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub difficulty: Option<i32>,
    #[serde(default)]
    pub ai_explanation: Option<Value>,
    #[serde(default)]
    pub review_state: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AcceptedFavoriteType {
    Vocabulary,
    Grammar,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptLearningItemRequest {
    pub favorite_type: AcceptedFavoriteType,
    #[serde(default)]
    pub pack_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcceptedFavorite {
    Vocabulary { id: String, pack_ids: Vec<String> },
    Grammar { id: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptLearningItemResponse {
    pub learning_item: LearningItem,
    pub favorite: AcceptedFavorite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLearningItemFromSelectionRequest {
    pub material_id: String,
    #[serde(default)]
    pub segment_id: Option<String>,
    pub selected_text: String,
    #[serde(default)]
    pub item_type: Option<String>,
    #[serde(default)]
    pub source_sentence: Option<String>,
    #[serde(default)]
    pub context_before: Option<String>,
    #[serde(default)]
    pub context_after: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendErrorBody {
    error: BackendErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendErrorDetail {
    code: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
struct AuthRequest<'a> {
    email: &'a str,
    password: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<&'a str>,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendClientError {
    #[error("backend is not configured")]
    NotConfigured,
    #[error("backend request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("backend file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("backend returned {status}: {code}: {message}")]
    Backend {
        status: StatusCode,
        code: String,
        message: String,
    },
    #[error("file name is invalid")]
    InvalidFileName,
}

impl BackendClient {
    pub fn from_app_config(config: &AppConfig) -> Result<Self, BackendClientError> {
        let config = BackendClientConfig::from_app_config(config)?;
        Ok(Self::new(config))
    }

    pub fn for_base_url(base_url: &str) -> Result<Self, BackendClientError> {
        let base_url = base_url.trim();
        if base_url.is_empty() {
            return Err(BackendClientError::NotConfigured);
        }

        Ok(Self::new(BackendClientConfig {
            base_url: normalize_base_url(base_url),
            auth_token: String::new(),
        }))
    }

    pub fn new(config: BackendClientConfig) -> Self {
        Self {
            client: Client::new(),
            base_url: normalize_base_url(&config.base_url),
            auth_token: config.auth_token,
        }
    }

    pub async fn health(&self) -> Result<BackendHealthResponse, BackendClientError> {
        let response = self
            .client
            .get(self.url("/health"))
            .timeout(Duration::from_secs(3))
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn register(
        &self,
        email: &str,
        password: &str,
        display_name: Option<&str>,
    ) -> Result<BackendAuthResponse, BackendClientError> {
        let response = self
            .client
            .post(self.url("/auth/register"))
            .json(&AuthRequest {
                email,
                password,
                display_name,
            })
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn login(
        &self,
        email: &str,
        password: &str,
    ) -> Result<BackendAuthResponse, BackendClientError> {
        let response = self
            .client
            .post(self.url("/auth/login"))
            .json(&AuthRequest {
                email,
                password,
                display_name: None,
            })
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn me(&self) -> Result<BackendCurrentUserResponse, BackendClientError> {
        let response = self
            .client
            .get(self.url("/auth/me"))
            .bearer_auth(&self.auth_token)
            .timeout(Duration::from_secs(5))
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn list_materials(&self) -> Result<Vec<Article>, BackendClientError> {
        self.list_materials_with_query(None).await
    }

    pub async fn list_materials_with_query(
        &self,
        query: Option<&ListMaterialsQuery>,
    ) -> Result<Vec<Article>, BackendClientError> {
        let request = self
            .client
            .get(self.url("/materials"))
            .bearer_auth(&self.auth_token);
        let request = if let Some(query) = query {
            request.query(query)
        } else {
            request
        };
        let response = request.send().await?;
        self.parse_response(response).await
    }

    pub async fn list_material_tags(&self) -> Result<Vec<MaterialTag>, BackendClientError> {
        let response = self
            .client
            .get(self.url("/material-tags"))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn create_material_tag(
        &self,
        payload: &CreateMaterialTagRequest,
    ) -> Result<MaterialTag, BackendClientError> {
        let response = self
            .client
            .post(self.url("/material-tags"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_material_tag(
        &self,
        id: &str,
        payload: &PatchMaterialTagRequest,
    ) -> Result<MaterialTag, BackendClientError> {
        let response = self
            .client
            .patch(self.url(&format!("/material-tags/{id}")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn delete_material_tag(
        &self,
        id: &str,
    ) -> Result<DeleteResponse, BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/material-tags/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn merge_material_tag(
        &self,
        source_tag_id: &str,
        payload: &MergeMaterialTagRequest,
    ) -> Result<MaterialTag, BackendClientError> {
        let response = self
            .client
            .post(self.url(&format!("/material-tags/{source_tag_id}/merge")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_material_tags(
        &self,
        material_id: &str,
    ) -> Result<Vec<MaterialTag>, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/materials/{material_id}/tags")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn set_material_tags(
        &self,
        material_id: &str,
        payload: &SetMaterialTagsRequest,
    ) -> Result<Vec<MaterialTag>, BackendClientError> {
        let response = self
            .client
            .put(self.url(&format!("/materials/{material_id}/tags")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn bulk_material_tags(
        &self,
        payload: &BulkMaterialTagsRequest,
    ) -> Result<BulkOperationResponse, BackendClientError> {
        let response = self
            .client
            .post(self.url("/materials/bulk-tags"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_reading_progress(
        &self,
        material_id: &str,
    ) -> Result<Option<ReadingProgress>, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/materials/{material_id}/reading-progress")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn upsert_reading_progress(
        &self,
        material_id: &str,
        payload: &UpsertReadingProgressRequest,
    ) -> Result<ReadingProgress, BackendClientError> {
        let response = self
            .client
            .put(self.url(&format!("/materials/{material_id}/reading-progress")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn list_material_import_jobs(
        &self,
        query: Option<&ListMaterialImportJobsQuery>,
    ) -> Result<Vec<MaterialImportJob>, BackendClientError> {
        let request = self
            .client
            .get(self.url("/material-import-jobs"))
            .bearer_auth(&self.auth_token);
        let request = if let Some(query) = query {
            request.query(query)
        } else {
            request
        };
        let response = request.send().await?;
        self.parse_response(response).await
    }

    pub async fn create_material_import_job(
        &self,
        payload: &CreateMaterialImportJobRequest,
    ) -> Result<MaterialImportJob, BackendClientError> {
        let response = self
            .client
            .post(self.url("/material-import-jobs"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_material_import_job(
        &self,
        id: &str,
    ) -> Result<MaterialImportJob, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/material-import-jobs/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_material_import_job(
        &self,
        id: &str,
        payload: &PatchMaterialImportJobRequest,
    ) -> Result<MaterialImportJob, BackendClientError> {
        let response = self
            .client
            .patch(self.url(&format!("/material-import-jobs/{id}")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_material_import_job_metadata(
        &self,
        id: &str,
        metadata: Value,
    ) -> Result<MaterialImportJob, BackendClientError> {
        self.patch_material_import_job(
            id,
            &PatchMaterialImportJobRequest {
                metadata: Some(metadata),
                ..Default::default()
            },
        )
        .await
    }

    pub async fn cancel_material_import_job(
        &self,
        id: &str,
    ) -> Result<MaterialImportJob, BackendClientError> {
        self.patch_material_import_job(
            id,
            &PatchMaterialImportJobRequest {
                status: Some("cancelled".to_string()),
                ..Default::default()
            },
        )
        .await
    }

    pub async fn delete_material_import_job(
        &self,
        id: &str,
    ) -> Result<DeleteResponse, BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/material-import-jobs/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn check_material_duplicates(
        &self,
        payload: &DuplicateCheckRequest,
    ) -> Result<DuplicateCheckResponse, BackendClientError> {
        let response = self
            .client
            .post(self.url("/materials/duplicate-check"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    async fn bulk_material_operation(
        &self,
        route: &str,
        payload: &BulkMaterialIdsRequest,
    ) -> Result<BulkOperationResponse, BackendClientError> {
        let response = self
            .client
            .post(self.url(route))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn bulk_archive_materials(
        &self,
        payload: &BulkMaterialIdsRequest,
    ) -> Result<BulkOperationResponse, BackendClientError> {
        self.bulk_material_operation("/materials/bulk-archive", payload)
            .await
    }

    pub async fn bulk_unarchive_materials(
        &self,
        payload: &BulkMaterialIdsRequest,
    ) -> Result<BulkOperationResponse, BackendClientError> {
        self.bulk_material_operation("/materials/bulk-unarchive", payload)
            .await
    }

    pub async fn bulk_delete_materials(
        &self,
        payload: &BulkMaterialIdsRequest,
    ) -> Result<BulkOperationResponse, BackendClientError> {
        self.bulk_material_operation("/materials/bulk-delete", payload)
            .await
    }

    pub async fn get_material(&self, id: &str) -> Result<Article, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/materials/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn create_material(
        &self,
        payload: &CreateMaterialRequest,
    ) -> Result<Article, BackendClientError> {
        self.create_material_with_options(payload, None, None).await
    }

    pub async fn create_material_with_options(
        &self,
        payload: &CreateMaterialRequest,
        file_sha256: Option<&str>,
        duplicate_policy: Option<&str>,
    ) -> Result<Article, BackendClientError> {
        let mut body = serde_json::to_value(payload)
            .expect("CreateMaterialRequest must serialize to a JSON object");
        let object = body
            .as_object_mut()
            .expect("CreateMaterialRequest must serialize to a JSON object");
        if let Some(file_sha256) = file_sha256 {
            object.insert(
                "file_sha256".to_string(),
                Value::String(file_sha256.to_string()),
            );
        }
        if let Some(duplicate_policy) = duplicate_policy {
            object.insert(
                "duplicate_policy".to_string(),
                Value::String(duplicate_policy.to_string()),
            );
        }
        let response = self
            .client
            .post(self.url("/materials"))
            .bearer_auth(&self.auth_token)
            .json(&body)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_material(
        &self,
        id: &str,
        payload: &PatchMaterialRequest,
    ) -> Result<Article, BackendClientError> {
        self.patch_material_with_file_hash(id, payload, None).await
    }

    pub async fn patch_material_with_file_hash(
        &self,
        id: &str,
        payload: &PatchMaterialRequest,
        file_sha256: Option<&str>,
    ) -> Result<Article, BackendClientError> {
        self.patch_material_with_options(id, payload, file_sha256, false)
            .await
    }

    pub async fn patch_material_replacing_source_fields_with_file_hash(
        &self,
        id: &str,
        payload: &PatchMaterialRequest,
        file_sha256: Option<&str>,
    ) -> Result<Article, BackendClientError> {
        self.patch_material_with_options(id, payload, file_sha256, true)
            .await
    }

    async fn patch_material_with_options(
        &self,
        id: &str,
        payload: &PatchMaterialRequest,
        file_sha256: Option<&str>,
        clear_source_fields: bool,
    ) -> Result<Article, BackendClientError> {
        let body = material_patch_request_body(payload, file_sha256, clear_source_fields);
        let response = self
            .client
            .patch(self.url(&format!("/materials/{id}")))
            .bearer_auth(&self.auth_token)
            .json(&body)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn delete_material(&self, id: &str) -> Result<(), BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/materials/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_empty_response(response).await
    }

    pub async fn list_learning_items(
        &self,
        query: Option<&ListLearningItemsRequest>,
    ) -> Result<Vec<LearningItem>, BackendClientError> {
        let request = self
            .client
            .get(self.url("/learning-items"))
            .bearer_auth(&self.auth_token);
        let request = if let Some(query) = query {
            request.query(query)
        } else {
            request
        };
        let response = request.send().await?;
        self.parse_response(response).await
    }

    pub async fn create_learning_item(
        &self,
        payload: &CreateLearningItemRequest,
    ) -> Result<LearningItem, BackendClientError> {
        let response = self
            .client
            .post(self.url("/learning-items"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn create_learning_item_from_selection(
        &self,
        payload: &CreateLearningItemFromSelectionRequest,
    ) -> Result<LearningItem, BackendClientError> {
        let response = self
            .client
            .post(self.url("/learning-items/from-selection"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_learning_item(
        &self,
        id: &str,
        payload: &UpdateLearningItemRequest,
    ) -> Result<LearningItem, BackendClientError> {
        let response = self
            .client
            .patch(self.url(&format!("/learning-items/{id}")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn delete_learning_item(&self, id: &str) -> Result<(), BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/learning-items/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        let _deleted: DeleteLearningItemResponse = self.parse_response(response).await?;
        Ok(())
    }

    pub async fn accept_learning_item(
        &self,
        id: &str,
        payload: &AcceptLearningItemRequest,
    ) -> Result<AcceptLearningItemResponse, BackendClientError> {
        let response = self
            .client
            .post(self.url(&format!("/learning-items/{id}/accept")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn list_word_packs(&self) -> Result<Vec<WordPack>, BackendClientError> {
        let response = self
            .client
            .get(self.url("/word-packs"))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_word_pack(&self, id: &str) -> Result<WordPack, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/word-packs/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn upsert_word_pack(
        &self,
        payload: &WordPack,
    ) -> Result<WordPack, BackendClientError> {
        let response = self
            .client
            .post(self.url("/word-packs"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_word_pack(
        &self,
        id: &str,
        payload: &WordPack,
    ) -> Result<WordPack, BackendClientError> {
        let response = self
            .client
            .patch(self.url(&format!("/word-packs/{id}")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn delete_word_pack(&self, id: &str) -> Result<(), BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/word-packs/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_empty_response(response).await
    }

    pub async fn list_favorite_vocabularies(
        &self,
    ) -> Result<Vec<FavoriteVocabulary>, BackendClientError> {
        let response = self
            .client
            .get(self.url("/favorite-vocabularies"))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_favorite_vocabulary(
        &self,
        id: &str,
    ) -> Result<FavoriteVocabulary, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/favorite-vocabularies/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn upsert_favorite_vocabulary(
        &self,
        payload: &FavoriteVocabulary,
    ) -> Result<FavoriteVocabulary, BackendClientError> {
        let response = self
            .client
            .post(self.url("/favorite-vocabularies"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_favorite_vocabulary(
        &self,
        id: &str,
        payload: &FavoriteVocabulary,
    ) -> Result<FavoriteVocabulary, BackendClientError> {
        let response = self
            .client
            .patch(self.url(&format!("/favorite-vocabularies/{id}")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn delete_favorite_vocabulary(&self, id: &str) -> Result<(), BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/favorite-vocabularies/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_empty_response(response).await
    }

    pub async fn list_favorite_grammars(&self) -> Result<Vec<FavoriteGrammar>, BackendClientError> {
        let response = self
            .client
            .get(self.url("/favorite-grammars"))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn upsert_favorite_grammar(
        &self,
        payload: &FavoriteGrammar,
    ) -> Result<FavoriteGrammar, BackendClientError> {
        let response = self
            .client
            .post(self.url("/favorite-grammars"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn delete_favorite_grammar(&self, id: &str) -> Result<(), BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/favorite-grammars/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_empty_response(response).await
    }

    pub async fn list_bookmarks(&self) -> Result<Vec<Bookmark>, BackendClientError> {
        let response = self
            .client
            .get(self.url("/bookmarks"))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn list_bookmarks_for_book(
        &self,
        book_path: &str,
    ) -> Result<Vec<Bookmark>, BackendClientError> {
        let response = self
            .client
            .get(self.url("/bookmarks"))
            .bearer_auth(&self.auth_token)
            .query(&[("book_path", book_path)])
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_bookmark(&self, id: &str) -> Result<Bookmark, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/bookmarks/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn upsert_bookmark(
        &self,
        payload: &Bookmark,
    ) -> Result<Bookmark, BackendClientError> {
        let response = self
            .client
            .post(self.url("/bookmarks"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_bookmark(
        &self,
        id: &str,
        payload: &Bookmark,
    ) -> Result<Bookmark, BackendClientError> {
        let response = self
            .client
            .patch(self.url(&format!("/bookmarks/{id}")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn delete_bookmark(&self, id: &str) -> Result<(), BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/bookmarks/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_empty_response(response).await
    }

    pub async fn save_agent_task(
        &self,
        payload: &AgentTask,
    ) -> Result<AgentTask, BackendClientError> {
        let response = self
            .client
            .put(self.url(&format!("/agent-tasks/{}", payload.id)))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_agent_task(&self, id: &str) -> Result<AgentTask, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/agent-tasks/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn save_artifact(&self, payload: &Artifact) -> Result<Artifact, BackendClientError> {
        let response = self
            .client
            .put(self.url(&format!("/artifacts/{}", payload.id)))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_artifact(
        &self,
        article_id: &str,
        artifact_id: &str,
    ) -> Result<Artifact, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/artifacts/{article_id}/{artifact_id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn upload_file_path(
        &self,
        path: &Path,
        metadata: Option<Value>,
    ) -> Result<BackendFile, BackendClientError> {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(BackendClientError::InvalidFileName)?
            .to_string();
        let bytes = tokio::fs::read(path).await?;
        let file_part = multipart::Part::bytes(bytes).file_name(file_name);
        let mut form = multipart::Form::new();
        if let Some(metadata) = metadata {
            form = form.text("metadata", metadata.to_string());
        }
        form = form.part("file", file_part);

        let response = self
            .client
            .post(self.url("/files"))
            .bearer_auth(&self.auth_token)
            .multipart(form)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn create_legacy_import(
        &self,
        payload: &LegacyImportRequest,
    ) -> Result<LegacyImportBatch, BackendClientError> {
        let response = self
            .client
            .post(self.url("/legacy-imports"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_legacy_import(
        &self,
        id: &str,
    ) -> Result<LegacyImportBatch, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/legacy-imports/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn parse_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, BackendClientError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response.json::<T>().await?);
        }

        Err(parse_backend_error(status, response.text().await.ok()))
    }

    async fn parse_empty_response(
        &self,
        response: reqwest::Response,
    ) -> Result<(), BackendClientError> {
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        Err(parse_backend_error(status, response.text().await.ok()))
    }
}

fn default_legacy_import_schema_version() -> String {
    "legacy-import-v1".to_string()
}

impl BackendClientConfig {
    pub fn from_app_config(config: &AppConfig) -> Result<Self, BackendClientError> {
        let base_url = config
            .backend_url
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .ok_or(BackendClientError::NotConfigured)?;
        let auth_token = config
            .auth_token
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .ok_or(BackendClientError::NotConfigured)?;

        Ok(Self {
            base_url: normalize_base_url(base_url),
            auth_token: auth_token.trim().to_string(),
        })
    }
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

fn parse_backend_error(status: StatusCode, body: Option<String>) -> BackendClientError {
    if let Some(body) = body {
        if let Ok(parsed) = serde_json::from_str::<BackendErrorBody>(&body) {
            return BackendClientError::Backend {
                status,
                code: parsed.error.code,
                message: parsed.error.message,
            };
        }
    }

    BackendClientError::Backend {
        status,
        code: "backend_error".to_string(),
        message: "backend request failed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_patch_body_omits_partial_fields_and_only_clears_missing_replace_sources() {
        let payload = PatchMaterialRequest {
            source_type: Some("web".to_string()),
            source_url: Some("https://example.com/material".to_string()),
            ..Default::default()
        };

        let partial = material_patch_request_body(&payload, None, false);
        assert_eq!(partial["source_type"], "web");
        assert_eq!(partial["source_url"], "https://example.com/material");
        for field in ["media_path", "book_path", "book_type"] {
            assert!(partial.get(field).is_none(), "{field} must be omitted");
        }

        let replacement = material_patch_request_body(&payload, None, true);
        assert_eq!(replacement["source_type"], "web");
        assert_eq!(replacement["source_url"], "https://example.com/material");
        for field in ["media_path", "book_path", "book_type"] {
            assert_eq!(replacement.get(field), Some(&Value::Null));
        }
    }

    #[test]
    fn config_requires_url_and_token() {
        let config = AppConfig {
            backend_url: Some("http://127.0.0.1:4000/".to_string()),
            auth_token: Some(" token ".to_string()),
            ..Default::default()
        };
        let resolved = BackendClientConfig::from_app_config(&config).unwrap();

        assert_eq!(resolved.base_url, "http://127.0.0.1:4000");
        assert_eq!(resolved.auth_token, "token");

        let missing = AppConfig::default();
        assert!(matches!(
            BackendClientConfig::from_app_config(&missing),
            Err(BackendClientError::NotConfigured)
        ));
    }

    #[test]
    fn parses_structured_backend_error() {
        let error = parse_backend_error(
            StatusCode::UNAUTHORIZED,
            Some(r#"{"error":{"code":"invalid_token","message":"invalid bearer token"}}"#.into()),
        );

        match error {
            BackendClientError::Backend {
                status,
                code,
                message,
            } => {
                assert_eq!(status, StatusCode::UNAUTHORIZED);
                assert_eq!(code, "invalid_token");
                assert_eq!(message, "invalid bearer token");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn learning_item_selection_payload_uses_snake_case_contract() {
        let payload = CreateLearningItemFromSelectionRequest {
            material_id: "3a8f4371-0f10-4d2a-8140-287f8c052441".to_string(),
            segment_id: Some("4e79490f-17b2-42eb-86d4-97bf2548eff7".to_string()),
            selected_text: "mitigate".to_string(),
            item_type: Some("word".to_string()),
            source_sentence: Some("This can mitigate risk.".to_string()),
            context_before: Some("This can".to_string()),
            context_after: Some("risk.".to_string()),
            tags: vec!["academic".to_string()],
        };

        let serialized = serde_json::to_value(&payload).unwrap();

        assert_eq!(
            serialized["material_id"],
            "3a8f4371-0f10-4d2a-8140-287f8c052441"
        );
        assert_eq!(serialized["selected_text"], "mitigate");
        assert_eq!(
            serialized["segment_id"],
            "4e79490f-17b2-42eb-86d4-97bf2548eff7"
        );
        assert_eq!(serialized["source_sentence"], "This can mitigate risk.");
    }

    #[test]
    fn learning_item_delete_response_accepts_backend_body() {
        let response: DeleteLearningItemResponse =
            serde_json::from_str(r#"{"deleted":true}"#).unwrap();

        assert!(response.deleted);
    }

    #[test]
    fn learning_item_acceptance_contract_matches_backend() {
        let request = AcceptLearningItemRequest {
            favorite_type: AcceptedFavoriteType::Vocabulary,
            pack_ids: vec!["pack-a".to_string()],
        };
        let serialized = serde_json::to_value(request).unwrap();
        assert_eq!(serialized["favorite_type"], "vocabulary");
        assert_eq!(serialized["pack_ids"][0], "pack-a");

        let response: AcceptLearningItemResponse = serde_json::from_value(serde_json::json!({
            "learning_item": {
                "id": "item-1",
                "material_id": null,
                "segment_id": null,
                "item_type": "word",
                "text": "mitigate",
                "source_sentence": "This can mitigate risk.",
                "collocations": [],
                "examples": [],
                "tags": [],
                "status": "accepted",
                "priority": 0,
                "review_state": {},
                "created_at": "2026-07-13T00:00:00Z",
                "updated_at": "2026-07-13T00:00:00Z"
            },
            "favorite": {
                "type": "vocabulary",
                "id": "learning-item-item-1",
                "pack_ids": ["pack-a"]
            }
        }))
        .unwrap();

        assert_eq!(response.learning_item.status, "accepted");
        assert!(matches!(
            response.favorite,
            AcceptedFavorite::Vocabulary { ref pack_ids, .. } if pack_ids == &["pack-a"]
        ));
    }

    #[test]
    fn article_keeps_material_library_fields_from_backend() {
        let article: Article = serde_json::from_value(serde_json::json!({
            "id": "material-1",
            "title": "Reading",
            "content": "Body",
            "source_type": "article",
            "source_url": null,
            "media_path": null,
            "book_path": null,
            "book_type": null,
            "created_at": "2026-07-11T00:00:00Z",
            "translated": false,
            "segments": [],
            "metadata": { "source": "test" },
            "tags": [{
                "id": "tag-1",
                "name": "Research",
                "color": "#2255aa",
                "created_at": "2026-07-11T00:00:00Z",
                "updated_at": "2026-07-11T00:00:00Z"
            }],
            "reading_progress": {
                "material_id": "material-1",
                "reader_kind": "article",
                "locator": { "kind": "segment", "segment_order": 2, "total_segments": 5 },
                "progress_ratio": 0.4,
                "status": "reading",
                "last_opened_at": "2026-07-11T00:00:00Z",
                "completed_at": null,
                "updated_at": "2026-07-11T00:00:00Z"
            },
            "archived_at": null
        }))
        .unwrap();

        assert_eq!(article.tags[0].name, "Research");
        assert_eq!(
            article.reading_progress.unwrap().locator["segment_order"],
            2
        );
        assert_eq!(article.metadata["source"], "test");
    }
}
