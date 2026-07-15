use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::{
    backend_client::BackendClientError,
    platform::safe_paths::{resolve_existing_child_path, resolve_existing_file_within_base},
    types::{AgentTask, Artifact, ArtifactType},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssistantTaskListQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub article_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTaskView {
    #[serde(flatten)]
    pub task: AgentTask,
    #[serde(default)]
    pub retry_root_task_id: Option<String>,
    #[serde(default)]
    pub retry_attempt: Option<i32>,
}

impl From<AgentTask> for AssistantTaskView {
    fn from(task: AgentTask) -> Self {
        Self {
            retry_root_task_id: task.root_task_id.clone(),
            retry_attempt: Some(task.attempt),
            task,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTaskListBackendResponse {
    #[serde(default)]
    pub items: Vec<AgentTask>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTaskListResponse {
    pub items: Vec<AssistantTaskView>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantArtifactView {
    #[serde(flatten)]
    pub artifact: Artifact,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_available: Option<bool>,
    pub missing: bool,
}

impl AssistantArtifactView {
    pub fn from_artifact(artifact: Artifact, app_data_dir: &Path) -> Self {
        if !matches!(&artifact.artifact_type, ArtifactType::File) {
            return Self {
                artifact,
                file_available: None,
                missing: false,
            };
        }

        let file_path = artifact
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("path").or_else(|| metadata.get("file_path")))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|path| !path.is_empty());
        let file_available = file_path.is_some_and(|path| {
            let path_buf = PathBuf::from(path);
            if path_buf.is_absolute() {
                resolve_existing_file_within_base(app_data_dir, &path_buf).is_ok()
            } else {
                resolve_existing_child_path(app_data_dir, path).is_ok()
            }
        });
        Self {
            artifact,
            file_available: Some(file_available),
            missing: !file_available,
        }
    }
}

impl From<AssistantTaskListBackendResponse> for AssistantTaskListResponse {
    fn from(response: AssistantTaskListBackendResponse) -> Self {
        Self {
            items: response
                .items
                .into_iter()
                .map(AssistantTaskView::from)
                .collect(),
            total: response.total,
            limit: response.limit,
            offset: response.offset,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTaskTimelineEvent {
    pub id: String,
    #[serde(default)]
    pub task_id: String,
    pub event_type: String,
    #[serde(default)]
    pub from_status: Option<String>,
    #[serde(default)]
    pub to_status: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTimelineIngestRequest {
    pub event_id: String,
    pub event_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default)]
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantActionAuditRequest {
    pub action_kind: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantActionExecution {
    pub task_id: String,
    pub action_kind: String,
    pub status: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub navigation: Option<Value>,
    pub audit_event: AssistantTaskTimelineEvent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantActionAudit {
    pub id: String,
    pub task_id: String,
    pub action_kind: String,
    pub status: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub payload: Value,
    pub registry_scope: String,
    pub external_write: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssistantCommandError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_status: Option<u16>,
    #[serde(default)]
    pub details: Value,
}

impl AssistantCommandError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            http_status: None,
            details: Value::Object(Default::default()),
        }
    }

    pub fn artifact_missing(message: impl Into<String>) -> Self {
        Self {
            code: "assistant_artifact_not_found".to_string(),
            message: message.into(),
            http_status: Some(StatusCode::NOT_FOUND.as_u16()),
            details: Value::Object(Default::default()),
        }
    }
}

impl From<BackendClientError> for AssistantCommandError {
    fn from(error: BackendClientError) -> Self {
        match error {
            BackendClientError::Backend {
                status,
                code,
                message,
            } => Self {
                code,
                message,
                http_status: Some(status.as_u16()),
                details: Value::Object(Default::default()),
            },
            BackendClientError::NotConfigured => Self::new(
                "assistant_backend_not_configured",
                "Backend is required for Assistant task operations",
            ),
            other => Self::new("assistant_backend_error", other.to_string()),
        }
    }
}

pub type AssistantArtifacts = Vec<Artifact>;

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn artifact(artifact_type: ArtifactType, metadata: Option<Value>) -> Artifact {
        Artifact {
            id: "artifact-1".to_string(),
            task_id: "task-1".to_string(),
            article_id: "article-1".to_string(),
            artifact_type,
            version: "1".to_string(),
            content: serde_json::json!({}),
            metadata,
            created_at: "2026-07-15T00:00:00Z".to_string(),
            updated_at: "2026-07-15T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn content_artifacts_do_not_require_a_local_file() {
        let view = AssistantArtifactView::from_artifact(
            artifact(ArtifactType::StructuredReport, None),
            Path::new("/missing-app-data"),
        );
        assert_eq!(view.file_available, None);
        assert!(!view.missing);
    }

    #[test]
    fn file_artifact_reports_real_safe_path_availability() {
        let app_data = std::env::temp_dir().join(format!("assistant-artifact-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&app_data).unwrap();
        std::fs::write(app_data.join("report.pdf"), b"report").unwrap();

        let available = AssistantArtifactView::from_artifact(
            artifact(
                ArtifactType::File,
                Some(serde_json::json!({ "file_path": "report.pdf" })),
            ),
            &app_data,
        );
        let missing = AssistantArtifactView::from_artifact(
            artifact(
                ArtifactType::File,
                Some(serde_json::json!({ "file_path": "../outside.pdf" })),
            ),
            &app_data,
        );

        assert_eq!(available.file_available, Some(true));
        assert!(!available.missing);
        assert_eq!(missing.file_available, Some(false));
        assert!(missing.missing);
        std::fs::remove_dir_all(app_data).unwrap();
    }
}
