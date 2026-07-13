use std::{fs, path::PathBuf};

use openkoto_desktop_lib::{
    agent_worker::mark_running_tasks_interrupted_in_dir,
    storage::{
        load_worker_task_checkpoint_in_dir, persist_worker_task_checkpoint_after_backend,
        save_worker_task_checkpoint_in_dir,
    },
    types::{AgentTask, AgentTaskInput, AgentTaskStatus, AgentTaskType},
};

fn temp_data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openkoto-agent-checkpoint-integrity-{name}-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_task(status: AgentTaskStatus) -> AgentTask {
    AgentTask {
        id: "task-1".to_string(),
        task_type: AgentTaskType::MindMapGenerate,
        status,
        article_id: "article-1".to_string(),
        input: AgentTaskInput {
            article_id: "article-1".to_string(),
            display_language: "zh-CN".to_string(),
            max_depth: 3,
            evidence_mode: "strict".to_string(),
            prefer_structure: "topic_tree".to_string(),
        },
        progress: 0.0,
        stage: Some("queued".to_string()),
        message: None,
        error: None,
        worker_session_id: None,
        artifact_ids: Vec::new(),
        created_at: "2026-07-13T00:00:00Z".to_string(),
        updated_at: "2026-07-13T00:00:00Z".to_string(),
        started_at: None,
        finished_at: None,
    }
}

#[test]
fn backend_failure_does_not_create_worker_checkpoint() {
    let data_dir = temp_data_dir("backend-failure");

    let error = persist_worker_task_checkpoint_after_backend(
        &data_dir,
        Err("backend PUT /agent-tasks/task-1 failed".to_string()),
    )
    .unwrap_err();

    assert!(error.contains("backend PUT"));
    assert!(load_worker_task_checkpoint_in_dir(&data_dir, "task-1").is_err());
}

#[test]
fn backend_confirmed_duplicate_update_overwrites_checkpoint() {
    let data_dir = temp_data_dir("duplicate-update");
    let queued = sample_task(AgentTaskStatus::Queued);
    persist_worker_task_checkpoint_after_backend(&data_dir, Ok(queued)).unwrap();

    let mut running = sample_task(AgentTaskStatus::Running);
    running.progress = 0.5;
    running.stage = Some("analyzing".to_string());
    let saved = persist_worker_task_checkpoint_after_backend(&data_dir, Ok(running)).unwrap();

    let restored = load_worker_task_checkpoint_in_dir(&data_dir, &saved.id).unwrap();
    assert!(matches!(restored.status, AgentTaskStatus::Running));
    assert_eq!(restored.progress, 0.5);
    assert_eq!(restored.stage.as_deref(), Some("analyzing"));
}

#[test]
fn restart_marks_only_worker_checkpoint_for_backend_recovery() {
    let data_dir = temp_data_dir("restart");
    let running = sample_task(AgentTaskStatus::Running);
    save_worker_task_checkpoint_in_dir(&data_dir, &running).unwrap();

    let interrupted = mark_running_tasks_interrupted_in_dir(&data_dir).unwrap();
    let restored = load_worker_task_checkpoint_in_dir(&data_dir, &running.id).unwrap();

    assert_eq!(interrupted, vec![running.id]);
    assert!(matches!(restored.status, AgentTaskStatus::Interrupted));
    assert!(data_dir.join("agent_tasks").exists() == false);
}
