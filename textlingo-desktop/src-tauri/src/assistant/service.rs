use reqwest::StatusCode;

use crate::{
    backend_client::{BackendClient, BackendClientError},
    types::{AgentTask, AgentTaskStatus, AgentTaskType, AppConfig, Artifact},
};

use super::dto::{
    AssistantActionAudit, AssistantActionAuditRequest, AssistantCommandError,
    AssistantTaskListQuery, AssistantTaskListResponse, AssistantTaskTimelineEvent,
    AssistantTaskView, AssistantTimelineIngestRequest,
};

pub struct AssistantService {
    client: BackendClient,
}

impl AssistantService {
    pub fn from_config(config: &AppConfig) -> Result<Self, AssistantCommandError> {
        Ok(Self {
            client: BackendClient::from_app_config(config).map_err(AssistantCommandError::from)?,
        })
    }

    pub async fn list_tasks(
        &self,
        query: &AssistantTaskListQuery,
    ) -> Result<AssistantTaskListResponse, AssistantCommandError> {
        self.client
            .list_agent_tasks(query)
            .await
            .map(AssistantTaskListResponse::from)
            .map_err(AssistantCommandError::from)
    }

    pub async fn task(&self, task_id: &str) -> Result<AgentTask, AssistantCommandError> {
        self.client
            .get_agent_task(task_id)
            .await
            .map_err(AssistantCommandError::from)
    }

    pub async fn task_view(
        &self,
        task_id: &str,
    ) -> Result<AssistantTaskView, AssistantCommandError> {
        self.task(task_id).await.map(AssistantTaskView::from)
    }

    pub async fn timeline(
        &self,
        task_id: &str,
    ) -> Result<Vec<AssistantTaskTimelineEvent>, AssistantCommandError> {
        self.client
            .get_agent_task_timeline(task_id)
            .await
            .map_err(AssistantCommandError::from)
    }

    pub async fn ingest_timeline(
        &self,
        task_id: &str,
        payload: &AssistantTimelineIngestRequest,
    ) -> Result<AssistantTaskTimelineEvent, AssistantCommandError> {
        self.client
            .ingest_agent_task_timeline_event(task_id, payload)
            .await
            .map_err(AssistantCommandError::from)
    }

    pub async fn cancel(&self, task_id: &str) -> Result<AgentTask, AssistantCommandError> {
        self.client
            .cancel_agent_task(task_id)
            .await
            .map_err(AssistantCommandError::from)
    }

    pub async fn retry(&self, task_id: &str) -> Result<AgentTask, AssistantCommandError> {
        let original = self.task(task_id).await?;
        validate_retryable_input(&original)?;
        let retried = self
            .client
            .retry_agent_task(task_id)
            .await
            .map_err(AssistantCommandError::from)?;
        validate_retry_copy(&original, &retried)?;
        Ok(retried)
    }

    pub async fn save_task(&self, task: &AgentTask) -> Result<AgentTask, AssistantCommandError> {
        self.client
            .save_agent_task(task)
            .await
            .map_err(AssistantCommandError::from)
    }

    pub async fn artifacts(&self, task_id: &str) -> Result<Vec<Artifact>, AssistantCommandError> {
        self.client
            .list_agent_task_artifacts(task_id)
            .await
            .map_err(|error| match error {
                BackendClientError::Backend {
                    status: StatusCode::NOT_FOUND,
                    message,
                    ..
                } => AssistantCommandError::artifact_missing(message),
                other => AssistantCommandError::from(other),
            })
    }

    pub async fn artifact(
        &self,
        article_id: &str,
        artifact_id: &str,
    ) -> Result<Artifact, AssistantCommandError> {
        self.client
            .get_artifact(article_id, artifact_id)
            .await
            .map_err(|error| match error {
                BackendClientError::Backend {
                    status: StatusCode::NOT_FOUND,
                    message,
                    ..
                } => AssistantCommandError::artifact_missing(message),
                other => AssistantCommandError::from(other),
            })
    }

    pub async fn audit_action(
        &self,
        task_id: &str,
        payload: &AssistantActionAuditRequest,
    ) -> Result<AssistantTaskTimelineEvent, AssistantCommandError> {
        self.client
            .record_agent_task_action(task_id, payload)
            .await
            .map_err(AssistantCommandError::from)
    }

    pub async fn actions(
        &self,
        task_id: &str,
    ) -> Result<Vec<AssistantActionAudit>, AssistantCommandError> {
        self.client
            .list_agent_task_actions(task_id)
            .await
            .map_err(AssistantCommandError::from)
    }
}

pub fn validate_retryable_input(task: &AgentTask) -> Result<(), AssistantCommandError> {
    if matches!(task.task_type, AgentTaskType::AssistantAgentTurn)
        && task
            .input
            .user_message
            .as_deref()
            .map(str::trim)
            .filter(|message| !message.is_empty())
            .is_none()
    {
        return Err(AssistantCommandError::new(
            "assistant_retry_input_unavailable",
            "Legacy Assistant task does not contain the user message required for retry",
        ));
    }
    Ok(())
}

pub fn validate_retry_copy(
    original: &AgentTask,
    retried: &AgentTask,
) -> Result<(), AssistantCommandError> {
    let original_type = serde_json::to_value(&original.task_type).map_err(|error| {
        AssistantCommandError::new("assistant_retry_integrity_error", error.to_string())
    })?;
    let retried_type = serde_json::to_value(&retried.task_type).map_err(|error| {
        AssistantCommandError::new("assistant_retry_integrity_error", error.to_string())
    })?;
    let original_input = serde_json::to_value(&original.input).map_err(|error| {
        AssistantCommandError::new("assistant_retry_integrity_error", error.to_string())
    })?;
    let retried_input = serde_json::to_value(&retried.input).map_err(|error| {
        AssistantCommandError::new("assistant_retry_integrity_error", error.to_string())
    })?;
    let valid = retried.id != original.id
        && retried.article_id == original.article_id
        && retried_type == original_type
        && retried_input == original_input
        && retried.input_snapshot == original.input_snapshot
        && retried.retry_of_task_id.as_deref() == Some(original.id.as_str())
        && retried.root_task_id == original.root_task_id
        && retried.attempt == original.attempt + 1
        && matches!(retried.status, AgentTaskStatus::Queued);
    if !valid {
        return Err(AssistantCommandError::new(
            "assistant_retry_integrity_error",
            "Backend retry response did not preserve the original task input and lineage",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        assistant::commands::{assistant_retry_replay_input, prepare_retry_dispatch},
        types::{AgentTaskInput, AssistantConversationMessage},
    };

    fn assistant_task(id: &str, status: AgentTaskStatus) -> AgentTask {
        let input = AgentTaskInput {
            article_id: "article-1".to_string(),
            display_language: "zh-CN".to_string(),
            max_depth: 0,
            evidence_mode: "none".to_string(),
            prefer_structure: "none".to_string(),
            user_message: Some("请结合上下文解释".to_string()),
            conversation: vec![AssistantConversationMessage {
                role: "user".to_string(),
                content: "先看第一段".to_string(),
            }],
            source_locator: Some(serde_json::json!({
                "version": 1,
                "kind": "segment",
                "segment_order": 0,
                "total_segments": 1,
                "segment_id": "segment-1"
            })),
            learning_item_id: Some("learning-1".to_string()),
        };
        AgentTask {
            id: id.to_string(),
            task_type: AgentTaskType::AssistantAgentTurn,
            status,
            article_id: input.article_id.clone(),
            input_snapshot: serde_json::to_value(&input).unwrap(),
            input,
            progress: 0.0,
            stage: Some("queued".to_string()),
            message: None,
            error: None,
            worker_session_id: None,
            artifact_ids: Vec::new(),
            created_at: "2026-07-15T00:00:00Z".to_string(),
            updated_at: "2026-07-15T00:00:00Z".to_string(),
            started_at: None,
            finished_at: None,
            root_task_id: Some("task-root".to_string()),
            retry_of_task_id: None,
            attempt: 1,
            output_version: 1,
            legacy_status: None,
        }
    }

    #[test]
    fn retry_validation_preserves_input_lineage_and_replay_payload() {
        let original = assistant_task("task-root", AgentTaskStatus::Succeeded);
        let mut retried = original.clone();
        retried.id = "task-retry".to_string();
        retried.status = AgentTaskStatus::Queued;
        retried.retry_of_task_id = Some(original.id.clone());
        retried.attempt = 2;

        validate_retry_copy(&original, &retried).unwrap();
        let (message, conversation) = assistant_retry_replay_input(&retried).unwrap();
        assert_eq!(message, "请结合上下文解释");
        assert_eq!(conversation[0].content, "先看第一段");
        assert_eq!(
            retried.input.learning_item_id.as_deref(),
            Some("learning-1")
        );
    }

    #[test]
    fn retry_dispatch_stays_queued_until_worker_started_event() {
        let retried = assistant_task("task-retry", AgentTaskStatus::Queued);
        let dispatching = prepare_retry_dispatch(retried);

        assert!(matches!(dispatching.status, AgentTaskStatus::Queued));
        assert_eq!(dispatching.stage.as_deref(), Some("retry_dispatch"));
        assert_eq!(
            dispatching.message.as_deref(),
            Some("Dispatching retried task to local worker")
        );
        assert!(dispatching.started_at.is_none());
    }

    #[test]
    fn legacy_assistant_task_without_user_message_is_not_retryable() {
        let mut legacy = assistant_task("task-legacy", AgentTaskStatus::Failed);
        legacy.input.user_message = None;

        let error = validate_retryable_input(&legacy).unwrap_err();
        assert_eq!(error.code, "assistant_retry_input_unavailable");
    }

    #[test]
    fn retry_validation_rejects_backend_input_drift() {
        let original = assistant_task("task-root", AgentTaskStatus::Succeeded);
        let mut retried = original.clone();
        retried.id = "task-retry".to_string();
        retried.status = AgentTaskStatus::Queued;
        retried.retry_of_task_id = Some(original.id.clone());
        retried.attempt = 2;
        retried.input.user_message = Some("被篡改的输入".to_string());

        let error = validate_retry_copy(&original, &retried).unwrap_err();
        assert_eq!(error.code, "assistant_retry_integrity_error");
    }
}
