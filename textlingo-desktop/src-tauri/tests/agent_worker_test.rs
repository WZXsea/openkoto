use std::{fs, path::PathBuf, thread::sleep, time::Duration};

use openkoto_desktop_lib::{
    agent_worker::{
        apply_worker_event_in_dir, apply_worker_event_to_task, build_assistant_worker_request,
        build_mind_map_worker_request, build_status_snapshot, build_worker_cancel_request,
        mark_running_tasks_interrupted_in_dir, parse_worker_event_line, push_worker_log,
        reconcile_task_after_restart, resolve_runtime_provider_config, worker_bundle_is_fresh,
        worker_event_log_entry, worker_event_timeline_request, WorkerHealth, WorkerLogEntry,
        WorkerLogLevel, WorkerRuntimeState,
    },
    storage::{
        load_legacy_agent_task_in_dir, load_legacy_artifact_in_dir, save_legacy_agent_task_in_dir,
    },
    types::{
        AgentTask, AgentTaskInput, AgentTaskStatus, AgentTaskType, Article,
        AssistantConversationMessage, MaterialSummary, ModelConfig,
    },
};

fn temp_data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openkoto-agent-worker-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn temp_worker_project_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openkoto-agent-worker-project-{}-{}",
        name,
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("dist")).unwrap();
    dir
}

fn save_article_fixture(data_dir: &PathBuf, article: &Article) {
    let articles_dir = data_dir.join("articles");
    fs::create_dir_all(&articles_dir).unwrap();
    fs::write(
        articles_dir.join(&article.id),
        serde_json::to_string(article).unwrap(),
    )
    .unwrap();
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
            user_message: None,
            conversation: Vec::new(),
            source_locator: None,
            learning_item_id: None,
        },
        progress: 0.0,
        stage: Some("queued".to_string()),
        message: None,
        error: None,
        worker_session_id: None,
        artifact_ids: Vec::new(),
        created_at: "2026-03-07T00:00:00Z".to_string(),
        updated_at: "2026-03-07T00:00:00Z".to_string(),
        started_at: None,
        finished_at: None,
        root_task_id: Some("task-1".to_string()),
        retry_of_task_id: None,
        attempt: 1,
        input_snapshot: serde_json::json!({}),
        output_version: 1,
        legacy_status: None,
    }
}

fn sample_assistant_task(status: AgentTaskStatus) -> AgentTask {
    AgentTask {
        id: "task-assistant-1".to_string(),
        task_type: AgentTaskType::AssistantAgentTurn,
        status,
        article_id: "article-1".to_string(),
        input: AgentTaskInput {
            article_id: "article-1".to_string(),
            display_language: "zh-CN".to_string(),
            max_depth: 0,
            evidence_mode: "none".to_string(),
            prefer_structure: "none".to_string(),
            user_message: Some("查看当前素材".to_string()),
            conversation: Vec::new(),
            source_locator: None,
            learning_item_id: None,
        },
        progress: 0.0,
        stage: Some("queued".to_string()),
        message: None,
        error: None,
        worker_session_id: None,
        artifact_ids: Vec::new(),
        created_at: "2026-03-07T00:00:00Z".to_string(),
        updated_at: "2026-03-07T00:00:00Z".to_string(),
        started_at: None,
        finished_at: None,
        root_task_id: Some("task-assistant-1".to_string()),
        retry_of_task_id: None,
        attempt: 1,
        input_snapshot: serde_json::json!({
            "user_message": "查看当前素材",
            "conversation": []
        }),
        output_version: 1,
        legacy_status: None,
    }
}

fn sample_article() -> Article {
    Article {
        id: "article-1".to_string(),
        title: "Sample Article".to_string(),
        content: "Alpha beta gamma. Delta epsilon zeta.".to_string(),
        source_type: Some("article".to_string()),
        source_url: None,
        media_path: None,
        book_path: None,
        book_type: None,
        created_at: "2026-03-07T00:00:00Z".to_string(),
        translated: false,
        active_mind_map_artifact_id: None,
        segments: Vec::new(),
        metadata: serde_json::json!({}),
        tags: Vec::new(),
        reading_progress: None,
        archived_at: None,
    }
}

fn sample_mind_map_result() -> serde_json::Value {
    serde_json::json!({
        "status": "applicable",
        "reason": null,
        "map": {
            "version": "1",
            "article_id": "article-1",
            "title": "Sample Article",
            "display_language": "zh-CN",
            "generation_mode": "evidence_first",
            "source_hash": "sha256:test",
            "summary": "summary",
            "root": {
                "id": "root",
                "title": "Root",
                "node_type": "root",
                "summary": "summary",
                "confidence": 1.0,
                "source_segment_ids": [],
                "source_offsets": [],
                "children": []
            }
        },
        "diagnostics": {
            "content_type": "article",
            "coverage": "full",
            "notes": [],
            "window_count": 1,
            "evidence_density": 1.0,
            "low_confidence_node_ids": []
        }
    })
}

fn sample_model_config(provider: &str, model: &str, base_url: Option<&str>) -> ModelConfig {
    ModelConfig {
        id: "model-1".to_string(),
        name: "Agent Model".to_string(),
        api_key: "secret".to_string(),
        api_provider: provider.to_string(),
        model: model.to_string(),
        is_default: true,
        created_at: Some("2026-03-07T00:00:00Z".to_string()),
        base_url: base_url.map(ToString::to_string),
    }
}

fn sample_material_summary(id: &str, title: &str, material_type: &str) -> MaterialSummary {
    MaterialSummary {
        id: id.to_string(),
        title: title.to_string(),
        material_type: material_type.to_string(),
        created_at: "2026-03-08T00:00:00Z".to_string(),
        translated: false,
    }
}

#[test]
fn worker_request_uses_agent_run_and_provider_config() {
    let provider_config = resolve_runtime_provider_config(&sample_model_config(
        "openrouter",
        "openai/gpt-4o-mini",
        None,
    ));
    let request = build_mind_map_worker_request(
        &sample_task(AgentTaskStatus::Queued),
        &sample_article(),
        &provider_config,
    );

    assert_eq!(request["type"], "request");
    assert_eq!(request["method"], "agent.run");
    assert_eq!(request["params"]["task_type"], "mind_map.generate");
    assert_eq!(request["params"]["input"]["article_id"], "article-1");
    assert_eq!(request["params"]["input"]["mode"], "balanced");
    assert_eq!(
        request["params"]["input"]["article_snapshot"]["title"],
        "Sample Article"
    );
    assert_eq!(
        request["params"]["input"]["article_snapshot"]["content"],
        "Alpha beta gamma. Delta epsilon zeta."
    );
    assert_eq!(
        request["params"]["provider_config"]["kind"],
        "openai_compatible"
    );
    assert_eq!(
        request["params"]["provider_config"]["baseUrl"],
        "https://openrouter.ai/api/v1"
    );
}

#[test]
fn build_assistant_worker_request_serializes_conversation_and_ui_context() {
    let provider_config = resolve_runtime_provider_config(&sample_model_config(
        "openrouter",
        "openai/gpt-4o-mini",
        None,
    ));

    let request = build_assistant_worker_request(
        "task-assistant-1",
        &provider_config,
        "打开最近的 PDF".to_string(),
        vec![AssistantConversationMessage {
            role: "user".to_string(),
            content: "先帮我看一下当前素材".to_string(),
        }],
        Some(sample_material_summary("article-1", "Current PDF", "pdf")),
        vec![sample_material_summary("article-2", "N1 PDF", "pdf")],
        Some("article-1".to_string()),
        "zh-CN".to_string(),
    );

    assert_eq!(request["params"]["task_type"], "assistant.agent_turn");
    assert_eq!(
        request["params"]["input"]["ui_context"]["current_article_id"],
        "article-1"
    );
    assert_eq!(
        request["params"]["input"]["conversation"][0]["content"],
        "先帮我看一下当前素材"
    );
    assert_eq!(
        request["params"]["input"]["available_materials"][0]["title"],
        "N1 PDF"
    );
}

#[test]
fn resolve_runtime_provider_config_maps_google_and_openai_compatible_providers() {
    let google =
        resolve_runtime_provider_config(&sample_model_config("google", "gemini-2.0-flash", None));
    let compatible = resolve_runtime_provider_config(&sample_model_config(
        "openai-compatible",
        "foo/bar",
        Some("https://example.com/v1"),
    ));

    assert_eq!(
        serde_json::to_value(&google).unwrap()["kind"],
        "native_google"
    );
    assert_eq!(
        serde_json::to_value(&compatible).unwrap()["kind"],
        "openai_compatible"
    );
}

#[test]
fn resolve_runtime_provider_config_maps_kimi_to_openai_compatible() {
    let kimi_cn = resolve_runtime_provider_config(&sample_model_config(
        "moonshot-cn",
        "kimi-k2-0711-preview",
        None,
    ));
    let kimi_legacy = resolve_runtime_provider_config(&sample_model_config(
        "moonshot",
        "kimi-k2-0711-preview",
        None,
    ));

    assert_eq!(
        serde_json::to_value(&kimi_cn).unwrap()["kind"],
        "openai_compatible"
    );
    assert_eq!(
        serde_json::to_value(&kimi_cn).unwrap()["baseUrl"],
        "https://api.moonshot.cn/v1"
    );
    assert_eq!(
        serde_json::to_value(&kimi_legacy).unwrap()["kind"],
        "openai_compatible"
    );
    assert_eq!(
        serde_json::to_value(&kimi_legacy).unwrap()["baseUrl"],
        "https://api.moonshot.cn/v1"
    );
}

#[test]
fn resolve_runtime_provider_config_maps_other_openai_compatible_providers() {
    let deepseek =
        resolve_runtime_provider_config(&sample_model_config("deepseek", "deepseek-chat", None));
    let siliconflow = resolve_runtime_provider_config(&sample_model_config(
        "siliconflow",
        "deepseek-ai/DeepSeek-V3",
        None,
    ));
    let provider_302 =
        resolve_runtime_provider_config(&sample_model_config("302ai", "gpt-4o", None));

    assert_eq!(
        serde_json::to_value(&deepseek).unwrap()["kind"],
        "openai_compatible"
    );
    assert_eq!(
        serde_json::to_value(&deepseek).unwrap()["baseUrl"],
        "https://api.deepseek.com/v1"
    );
    assert_eq!(
        serde_json::to_value(&siliconflow).unwrap()["kind"],
        "openai_compatible"
    );
    assert_eq!(
        serde_json::to_value(&siliconflow).unwrap()["baseUrl"],
        "https://api.siliconflow.cn/v1"
    );
    assert_eq!(
        serde_json::to_value(&provider_302).unwrap()["kind"],
        "openai_compatible"
    );
    assert_eq!(
        serde_json::to_value(&provider_302).unwrap()["baseUrl"],
        "https://api.302.ai/v1"
    );
}

#[test]
fn worker_health_turns_unhealthy_after_timeout() {
    let now = chrono::Utc::now();
    let stale = WorkerRuntimeState {
        worker_session_id: Some("worker-1".to_string()),
        started_at: Some(now - chrono::Duration::seconds(60)),
        last_heartbeat_at: Some(now - chrono::Duration::seconds(20)),
    };

    assert!(matches!(
        stale.health(now, chrono::Duration::seconds(10)),
        WorkerHealth::Unhealthy
    ));
}

#[test]
fn running_tasks_can_be_marked_interrupted_after_restart() {
    let data_dir = temp_data_dir("interrupt");
    let task = sample_task(AgentTaskStatus::Running);
    save_legacy_agent_task_in_dir(&data_dir, &task).unwrap();

    let interrupted = mark_running_tasks_interrupted_in_dir(&data_dir).unwrap();
    let stored = load_legacy_agent_task_in_dir(&data_dir, &task.id).unwrap();

    assert_eq!(interrupted, vec![task.id]);
    assert!(matches!(stored.status, AgentTaskStatus::Interrupted));
}

#[test]
fn cancel_request_targets_only_the_requested_task() {
    let request = build_worker_cancel_request("task-42");

    assert_eq!(request["method"], "agent.cancel");
    assert_eq!(request["params"]["task_id"], "task-42");
    assert!(request["id"]
        .as_str()
        .is_some_and(|id| id.starts_with("cancel-task-42-")));
}

#[test]
fn restart_recovery_never_reopens_terminal_state() {
    let now = chrono::Utc::now();
    for status in [AgentTaskStatus::Queued, AgentTaskStatus::Running] {
        let checkpoint = sample_task(status.clone());
        let backend = sample_task(status);
        let recovered = reconcile_task_after_restart(&checkpoint, &backend, now);
        assert!(matches!(recovered.status, AgentTaskStatus::Interrupted));
        assert_eq!(recovered.stage.as_deref(), Some("worker_restarted"));
    }

    let checkpoint_running = sample_task(AgentTaskStatus::Running);
    let backend_cancelled = sample_task(AgentTaskStatus::Cancelled);
    let recovered = reconcile_task_after_restart(&checkpoint_running, &backend_cancelled, now);
    assert!(matches!(recovered.status, AgentTaskStatus::Cancelled));

    let checkpoint_succeeded = sample_task(AgentTaskStatus::Succeeded);
    let backend_running = sample_task(AgentTaskStatus::Running);
    let recovered = reconcile_task_after_restart(&checkpoint_succeeded, &backend_running, now);
    assert!(matches!(recovered.status, AgentTaskStatus::Succeeded));
}

#[test]
fn late_worker_result_cannot_overwrite_cancelled_task() {
    let mut task = sample_task(AgentTaskStatus::Cancelled);
    task.finished_at = Some("2026-03-07T00:00:01Z".to_string());
    let before = task.clone();
    let event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.result",
            "payload": {
                "task_id": task.id,
                "artifact_type": "mind_map",
                "content": sample_mind_map_result(),
                "timestamp": "2026-03-07T00:00:02Z"
            }
        })
        .to_string(),
    )
    .unwrap();

    let artifact =
        apply_worker_event_to_task(&mut task, &mut WorkerRuntimeState::default(), &event).unwrap();
    let timeline = worker_event_timeline_request(&before, &task, &event).unwrap();

    assert!(matches!(task.status, AgentTaskStatus::Cancelled));
    assert!(task.artifact_ids.is_empty());
    assert!(artifact.is_none());
    assert_eq!(timeline.event_type, "worker.late_event_ignored");
    assert_eq!(timeline.metadata["original_event_type"], "task.result");
}

#[test]
fn worker_timeline_event_id_is_stable_for_backend_idempotency() {
    let before = sample_task(AgentTaskStatus::Running);
    let mut after = before.clone();
    let raw = serde_json::json!({
        "type": "event",
        "event": "task.progress",
        "payload": {
            "task_id": before.id,
            "stage": "generating",
            "progress": 0.5,
            "message": "Generating map",
            "timestamp": "2026-03-07T00:00:02Z"
        }
    })
    .to_string();
    let first = parse_worker_event_line(&raw).unwrap();
    apply_worker_event_to_task(&mut after, &mut WorkerRuntimeState::default(), &first).unwrap();
    let first_timeline = worker_event_timeline_request(&before, &after, &first).unwrap();
    let second = parse_worker_event_line(&raw).unwrap();
    let second_timeline = worker_event_timeline_request(&before, &after, &second).unwrap();

    assert_eq!(first_timeline.event_id, second_timeline.event_id);
    assert_eq!(first_timeline.created_at, "2026-03-07T00:00:02Z");
    assert_eq!(first_timeline.metadata["progress"], 0.5);
}

#[test]
fn parses_task_result_events_from_worker_stdout() {
    let event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.result",
            "payload": {
                "task_id": "task-1",
                "content": sample_mind_map_result(),
            }
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(event.event_name(), "task.result");
}

#[test]
fn parses_task_started_events_from_worker_stdout() {
    let event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.started",
            "payload": {
                "task_id": "task-1",
                "task_type": "mind_map.generate",
                "timestamp": "2026-03-07T00:00:01Z",
            }
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(event.event_name(), "task.started");
}

#[test]
fn result_events_persist_artifact_and_complete_task() {
    let data_dir = temp_data_dir("result");
    let task = sample_task(AgentTaskStatus::Running);
    save_article_fixture(&data_dir, &sample_article());
    save_legacy_agent_task_in_dir(&data_dir, &task).unwrap();

    let event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.result",
            "payload": {
                "task_id": task.id,
                "content": sample_mind_map_result(),
            }
        })
        .to_string(),
    )
    .unwrap();

    apply_worker_event_in_dir(
        &data_dir,
        &mut WorkerRuntimeState {
            worker_session_id: Some("worker-1".to_string()),
            started_at: Some(chrono::Utc::now()),
            last_heartbeat_at: None,
        },
        event,
    )
    .unwrap();

    let stored_task = load_legacy_agent_task_in_dir(&data_dir, &task.id).unwrap();
    assert!(matches!(stored_task.status, AgentTaskStatus::Succeeded));
    assert_eq!(stored_task.artifact_ids.len(), 1);

    let artifact = load_legacy_artifact_in_dir(
        &data_dir,
        &stored_task.article_id,
        &stored_task.artifact_ids[0],
    )
    .unwrap();
    assert_eq!(artifact.article_id, "article-1");
    assert_eq!(artifact.content["status"], "applicable");
}

#[test]
fn assistant_result_events_complete_task_without_persisting_artifacts() {
    let data_dir = temp_data_dir("assistant-result");
    let task = sample_assistant_task(AgentTaskStatus::Running);
    save_article_fixture(&data_dir, &sample_article());
    save_legacy_agent_task_in_dir(&data_dir, &task).unwrap();

    let event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.result",
            "payload": {
                "task_id": task.id,
                "artifact_type": "article_answer",
                "content": {
                    "reply": "我已经打开了 N1 PDF",
                    "action": {
                        "kind": "open_material",
                        "material_id": "article-2"
                    }
                },
            }
        })
        .to_string(),
    )
    .unwrap();

    let artifact = apply_worker_event_in_dir(
        &data_dir,
        &mut WorkerRuntimeState {
            worker_session_id: Some("worker-1".to_string()),
            started_at: Some(chrono::Utc::now()),
            last_heartbeat_at: None,
        },
        event,
    )
    .unwrap();

    let stored_task = load_legacy_agent_task_in_dir(&data_dir, &task.id).unwrap();
    assert!(matches!(stored_task.status, AgentTaskStatus::Succeeded));
    assert!(stored_task.artifact_ids.is_empty());
    assert_eq!(stored_task.message.as_deref(), Some("Agent turn completed"));
    assert!(artifact.is_none());
}

#[test]
fn task_started_event_marks_task_running() {
    let data_dir = temp_data_dir("task-started");
    let task = sample_task(AgentTaskStatus::Queued);
    save_legacy_agent_task_in_dir(&data_dir, &task).unwrap();

    let event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.started",
            "payload": {
                "task_id": task.id,
                "task_type": "mind_map.generate",
                "timestamp": "2026-03-07T00:00:01Z",
            }
        })
        .to_string(),
    )
    .unwrap();

    apply_worker_event_in_dir(
        &data_dir,
        &mut WorkerRuntimeState {
            worker_session_id: Some("worker-1".to_string()),
            started_at: Some(chrono::Utc::now()),
            last_heartbeat_at: None,
        },
        event,
    )
    .unwrap();

    let stored_task = load_legacy_agent_task_in_dir(&data_dir, &task.id).unwrap();
    assert!(matches!(stored_task.status, AgentTaskStatus::Running));
    assert_eq!(stored_task.stage.as_deref(), Some("started"));
    assert_eq!(stored_task.worker_session_id.as_deref(), Some("worker-1"));
}

#[test]
fn progress_events_do_not_reopen_completed_tasks() {
    let data_dir = temp_data_dir("progress-after-result");
    let mut task = sample_task(AgentTaskStatus::Succeeded);
    task.progress = 1.0;
    task.stage = Some("done".to_string());
    task.finished_at = Some("2026-03-07T00:00:02Z".to_string());
    save_legacy_agent_task_in_dir(&data_dir, &task).unwrap();

    let event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.progress",
            "payload": {
                "task_id": task.id,
                "stage": "done",
                "progress": 1.0,
                "message": "Mind map generated",
            }
        })
        .to_string(),
    )
    .unwrap();

    apply_worker_event_in_dir(&data_dir, &mut WorkerRuntimeState::default(), event).unwrap();

    let stored_task = load_legacy_agent_task_in_dir(&data_dir, &task.id).unwrap();
    assert!(matches!(stored_task.status, AgentTaskStatus::Succeeded));
    assert_eq!(stored_task.stage.as_deref(), Some("done"));
}

#[test]
fn status_snapshot_includes_recent_logs() {
    let now = chrono::Utc::now();
    let runtime_state = WorkerRuntimeState {
        worker_session_id: Some("worker-9".to_string()),
        started_at: Some(now),
        last_heartbeat_at: Some(now),
    };
    let logs = vec![WorkerLogEntry {
        timestamp: now.to_rfc3339(),
        level: WorkerLogLevel::Info,
        source: "worker".to_string(),
        message: "worker ready".to_string(),
    }];

    let snapshot = build_status_snapshot(&runtime_state, &logs, now);
    assert!(matches!(snapshot.health, WorkerHealth::Healthy));
    assert_eq!(snapshot.logs.len(), 1);
    assert_eq!(snapshot.logs[0].message, "worker ready");
}

#[test]
fn worker_log_buffer_keeps_only_latest_entries() {
    let mut logs = Vec::new();
    for index in 0..110 {
        push_worker_log(
            &mut logs,
            WorkerLogLevel::Info,
            "worker",
            format!("entry-{index}"),
        );
    }

    assert_eq!(logs.len(), 100);
    assert_eq!(logs.first().unwrap().message, "entry-10");
    assert_eq!(logs.last().unwrap().message, "entry-109");
}

#[test]
fn heartbeat_events_do_not_create_debug_log_entries() {
    let heartbeat = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "worker.heartbeat",
            "payload": {
                "worker_session_id": "worker-1",
                "timestamp": "2026-03-07T00:00:00Z"
            }
        })
        .to_string(),
    )
    .unwrap();

    assert!(worker_event_log_entry(&heartbeat).is_none());
}

#[test]
fn task_progress_events_are_converted_into_human_readable_logs() {
    let progress = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.progress",
            "payload": {
                "task_id": "task-1",
                "stage": "thinking",
                "progress": 0.42,
                "message": "Waiting for Claude response"
            }
        })
        .to_string(),
    )
    .unwrap();

    let entry = worker_event_log_entry(&progress).expect("expected progress log");
    assert!(matches!(entry.0, WorkerLogLevel::Info));
    assert_eq!(entry.1, "thinking 42% Waiting for Claude response");
}

#[test]
fn worker_ready_event_updates_runtime_state() {
    let mut runtime_state = WorkerRuntimeState::default();
    let data_dir = temp_data_dir("worker-ready");

    let event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "worker.ready",
            "payload": {
                "worker_session_id": "worker-ready-1",
                "timestamp": "2026-03-07T00:00:00Z",
                "runtime": "opencode",
                "version": "0.1.0"
            }
        })
        .to_string(),
    )
    .unwrap();

    apply_worker_event_in_dir(&data_dir, &mut runtime_state, event).unwrap();

    assert_eq!(
        runtime_state.worker_session_id,
        Some("worker-ready-1".to_string())
    );
    assert!(runtime_state.started_at.is_some());
}

#[test]
fn task_log_events_are_converted_into_log_entries() {
    let log_event = parse_worker_event_line(
        &serde_json::json!({
            "type": "event",
            "event": "task.log",
            "payload": {
                "task_id": "task-1",
                "level": "warn",
                "source": "provider",
                "message": "rate limit approaching",
                "timestamp": "2026-03-07T00:00:00Z"
            }
        })
        .to_string(),
    )
    .unwrap();

    let entry = worker_event_log_entry(&log_event).expect("expected log entry");
    assert!(matches!(entry.0, WorkerLogLevel::Warn));
    assert_eq!(entry.1, "provider: rate limit approaching");
}

#[test]
fn worker_bundle_is_stale_when_required_output_is_missing() {
    let project_dir = temp_worker_project_dir("missing-output");
    for file in [
        "index",
        "assistantTask",
        "mindMapTask",
        "protocol",
        "runtime",
    ] {
        fs::write(
            project_dir.join("src").join(format!("{file}.ts")),
            "export {};\n",
        )
        .unwrap();
    }
    fs::write(project_dir.join("dist").join("index.js"), "export {};\n").unwrap();

    assert!(!worker_bundle_is_fresh(&project_dir).unwrap());
}

#[test]
fn worker_bundle_is_stale_when_source_is_newer_than_dist() {
    let project_dir = temp_worker_project_dir("stale-output");
    for file in [
        "index",
        "assistantTask",
        "mindMapTask",
        "protocol",
        "runtime",
    ] {
        fs::write(
            project_dir.join("src").join(format!("{file}.ts")),
            "export {};\n",
        )
        .unwrap();
        fs::write(
            project_dir.join("dist").join(format!("{file}.js")),
            "export {};\n",
        )
        .unwrap();
    }

    sleep(Duration::from_millis(20));
    fs::write(
        project_dir.join("src").join("assistantTask.ts"),
        "export const newer = true;\n",
    )
    .unwrap();

    assert!(!worker_bundle_is_fresh(&project_dir).unwrap());
}
