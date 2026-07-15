use tauri::{AppHandle, Manager, State};

use crate::{
    agent_worker::{resolve_runtime_provider_config, AgentWorkerManager},
    commands::{
        get_article, list_articles_cmd, material_summary_from_article,
        require_active_agent_model_config,
    },
    storage::{load_config, save_worker_task_checkpoint_in_dir},
    types::{AgentTask, AgentTaskStatus, AgentTaskType, Artifact},
};

use super::{
    actions::execute_registered_assistant_action,
    dto::{
        AssistantActionAudit, AssistantActionExecution, AssistantArtifactView,
        AssistantCommandError, AssistantTaskListQuery, AssistantTaskListResponse,
        AssistantTaskTimelineEvent, AssistantTaskView,
    },
    service::AssistantService,
};

fn service_for_app(app_handle: &AppHandle) -> Result<AssistantService, AssistantCommandError> {
    let config = load_config(app_handle)
        .map_err(|error| AssistantCommandError::new("assistant_config_error", error))?
        .unwrap_or_default();
    AssistantService::from_config(&config)
}

fn app_error(code: &str, error: impl Into<String>) -> AssistantCommandError {
    AssistantCommandError::new(code, error)
}

#[tauri::command]
pub async fn assistant_task_list_cmd(
    app_handle: AppHandle,
    query: Option<AssistantTaskListQuery>,
) -> Result<AssistantTaskListResponse, AssistantCommandError> {
    service_for_app(&app_handle)?
        .list_tasks(&query.unwrap_or_default())
        .await
}

#[tauri::command]
pub async fn assistant_task_detail_cmd(
    app_handle: AppHandle,
    task_id: String,
) -> Result<AssistantTaskView, AssistantCommandError> {
    service_for_app(&app_handle)?.task_view(&task_id).await
}

#[tauri::command]
pub async fn assistant_task_timeline_cmd(
    app_handle: AppHandle,
    task_id: String,
) -> Result<Vec<AssistantTaskTimelineEvent>, AssistantCommandError> {
    service_for_app(&app_handle)?.timeline(&task_id).await
}

#[tauri::command]
pub async fn assistant_task_cancel_cmd(
    app_handle: AppHandle,
    worker_manager: State<'_, AgentWorkerManager>,
    task_id: String,
) -> Result<AssistantTaskView, AssistantCommandError> {
    let service = service_for_app(&app_handle)?;
    let cancelled = service.cancel(&task_id).await?;
    worker_manager
        .cancel_task(&task_id)
        .map_err(|error| app_error("assistant_worker_cancel_failed", error))?;
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| app_error("assistant_checkpoint_error", error.to_string()))?;
    save_worker_task_checkpoint_in_dir(&data_dir, &cancelled)
        .map_err(|error| app_error("assistant_checkpoint_error", error))?;
    Ok(AssistantTaskView::from(cancelled))
}

#[tauri::command]
pub async fn assistant_task_retry_cmd(
    app_handle: AppHandle,
    worker_manager: State<'_, AgentWorkerManager>,
    task_id: String,
) -> Result<AssistantTaskView, AssistantCommandError> {
    let service = service_for_app(&app_handle)?;
    let retried = service.retry(&task_id).await?;
    let running = mark_retry_running(&service, retried).await?;
    match dispatch_retry(&app_handle, &worker_manager, &running).await {
        Ok(()) => service.task_view(&running.id).await,
        Err(error) => {
            let mut failed = running;
            failed.status = AgentTaskStatus::Failed;
            failed.stage = Some("retry_failed_to_start".to_string());
            failed.error = Some(error.message.clone());
            failed.updated_at = chrono::Utc::now().to_rfc3339();
            failed.finished_at = Some(failed.updated_at.clone());
            let failed = service.save_task(&failed).await?;
            Ok(AssistantTaskView::from(failed))
        }
    }
}

async fn mark_retry_running(
    service: &AssistantService,
    mut task: AgentTask,
) -> Result<AgentTask, AssistantCommandError> {
    task.status = AgentTaskStatus::Running;
    task.stage = Some("retry_dispatch".to_string());
    task.message = Some("Dispatching retried task to local worker".to_string());
    task.updated_at = chrono::Utc::now().to_rfc3339();
    task.started_at = Some(task.updated_at.clone());
    service.save_task(&task).await
}

async fn dispatch_retry(
    app_handle: &AppHandle,
    worker_manager: &AgentWorkerManager,
    task: &AgentTask,
) -> Result<(), AssistantCommandError> {
    let article = get_article(app_handle.clone(), task.article_id.clone())
        .await
        .map_err(|error| app_error("assistant_retry_material_unavailable", error))?;
    let active_model = require_active_agent_model_config(
        load_config(app_handle).map_err(|error| app_error("assistant_config_error", error))?,
    )
    .map_err(|error| app_error("assistant_retry_model_unavailable", error))?;
    let provider_config = resolve_runtime_provider_config(&active_model);

    match task.task_type {
        AgentTaskType::MindMapGenerate => worker_manager
            .submit_mind_map_task(app_handle, task, &article, &provider_config)
            .map_err(|error| app_error("assistant_retry_dispatch_failed", error)),
        AgentTaskType::AssistantAgentTurn => {
            let (user_message, conversation) = assistant_retry_replay_input(task)?;
            let articles = list_articles_cmd(app_handle.clone())
                .await
                .map_err(|error| app_error("assistant_retry_material_unavailable", error))?;
            let current_material = material_summary_from_article(&article);
            let available_materials = articles
                .iter()
                .map(material_summary_from_article)
                .collect::<Vec<_>>();
            worker_manager
                .submit_assistant_turn(
                    app_handle,
                    task,
                    user_message,
                    conversation,
                    current_material,
                    available_materials,
                    &provider_config,
                )
                .map_err(|error| app_error("assistant_retry_dispatch_failed", error))
        }
        _ => Err(app_error(
            "assistant_retry_task_type_unsupported",
            "This task type cannot be replayed by the local worker",
        )),
    }
}

pub fn assistant_retry_replay_input(
    task: &AgentTask,
) -> Result<(String, Vec<crate::types::AssistantConversationMessage>), AssistantCommandError> {
    let user_message = task
        .input
        .user_message
        .clone()
        .filter(|message| !message.trim().is_empty())
        .ok_or_else(|| {
            app_error(
                "assistant_retry_input_unavailable",
                "Assistant retry input does not include a user message",
            )
        })?;
    Ok((user_message, task.input.conversation.clone()))
}

#[tauri::command]
pub async fn assistant_task_artifacts_cmd(
    app_handle: AppHandle,
    task_id: String,
) -> Result<Vec<AssistantArtifactView>, AssistantCommandError> {
    let artifacts = service_for_app(&app_handle)?.artifacts(&task_id).await?;
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| app_error("assistant_artifact_path_error", error.to_string()))?;
    Ok(artifacts
        .into_iter()
        .map(|artifact| AssistantArtifactView::from_artifact(artifact, &app_data_dir))
        .collect())
}

#[tauri::command]
pub async fn assistant_artifact_detail_cmd(
    app_handle: AppHandle,
    article_id: String,
    artifact_id: String,
) -> Result<Artifact, AssistantCommandError> {
    service_for_app(&app_handle)?
        .artifact(&article_id, &artifact_id)
        .await
}

#[tauri::command]
pub async fn assistant_action_execute_cmd(
    app_handle: AppHandle,
    task_id: String,
    action: serde_json::Value,
) -> Result<AssistantActionExecution, AssistantCommandError> {
    execute_registered_assistant_action(&app_handle, &task_id, &action).await
}

#[tauri::command]
pub async fn assistant_task_actions_cmd(
    app_handle: AppHandle,
    task_id: String,
) -> Result<Vec<AssistantActionAudit>, AssistantCommandError> {
    service_for_app(&app_handle)?.actions(&task_id).await
}
