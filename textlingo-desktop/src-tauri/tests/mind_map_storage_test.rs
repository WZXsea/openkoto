use std::{fs, path::PathBuf};

use openkoto_desktop_lib::{
    storage::{
        load_agent_task_in_dir, load_artifact_in_dir, save_agent_task_in_dir, save_artifact_in_dir,
        update_article_active_mind_map_artifact_in_dir,
    },
    types::{
        AgentTask, AgentTaskInput, AgentTaskStatus, AgentTaskType, Article, Artifact, ArtifactType,
    },
};

fn temp_data_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("openkoto-{}-{}", name, uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_task() -> AgentTask {
    AgentTask {
        id: "task-1".to_string(),
        task_type: AgentTaskType::MindMapGenerate,
        status: AgentTaskStatus::Queued,
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
        created_at: "2026-03-07T00:00:00Z".to_string(),
        updated_at: "2026-03-07T00:00:00Z".to_string(),
        started_at: None,
        finished_at: None,
    }
}

fn sample_artifact() -> Artifact {
    Artifact {
        id: "artifact-1".to_string(),
        task_id: "task-1".to_string(),
        article_id: "article-1".to_string(),
        artifact_type: ArtifactType::MindMap,
        version: "1".to_string(),
        content: serde_json::json!({
            "status": "applicable"
        }),
        metadata: Some(serde_json::json!({
            "source_hash": "sha256:abc"
        })),
        created_at: "2026-03-07T00:00:00Z".to_string(),
        updated_at: "2026-03-07T00:00:00Z".to_string(),
    }
}

fn sample_article() -> Article {
    Article {
        id: "article-1".to_string(),
        title: "Sample Article".to_string(),
        content: "Some content".to_string(),
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

#[test]
fn saves_and_loads_agent_task_in_data_dir() {
    let data_dir = temp_data_dir("agent-task");
    let task = sample_task();

    save_agent_task_in_dir(&data_dir, &task).unwrap();
    let restored = load_agent_task_in_dir(&data_dir, &task.id).unwrap();

    assert_eq!(restored.id, task.id);
    assert_eq!(restored.article_id, task.article_id);
    assert!(matches!(restored.status, AgentTaskStatus::Queued));
}

#[test]
fn saves_and_loads_artifact_in_data_dir() {
    let data_dir = temp_data_dir("artifact");
    let artifact = sample_artifact();

    save_artifact_in_dir(&data_dir, &artifact).unwrap();
    let restored = load_artifact_in_dir(&data_dir, &artifact.article_id, &artifact.id).unwrap();

    assert_eq!(restored.id, artifact.id);
    assert!(matches!(restored.artifact_type, ArtifactType::MindMap));
    assert_eq!(restored.content["status"], serde_json::json!("applicable"));
}

#[test]
fn updates_article_active_mind_map_artifact_id_without_touching_other_fields() {
    let data_dir = temp_data_dir("article-update");
    let article = sample_article();
    let articles_dir = data_dir.join("articles");
    fs::create_dir_all(&articles_dir).unwrap();
    fs::write(
        articles_dir.join(&article.id),
        serde_json::to_string(&article).unwrap(),
    )
    .unwrap();

    update_article_active_mind_map_artifact_in_dir(
        &data_dir,
        &article.id,
        Some("artifact-1".to_string()),
    )
    .unwrap();

    let updated: Article =
        serde_json::from_str(&fs::read_to_string(articles_dir.join(&article.id)).unwrap()).unwrap();

    assert_eq!(
        updated.active_mind_map_artifact_id.as_deref(),
        Some("artifact-1")
    );
    assert_eq!(updated.title, article.title);
    assert_eq!(updated.content, article.content);
    assert_eq!(updated.source_type, article.source_type);
    assert_eq!(updated.translated, article.translated);
}

#[test]
fn storage_rejects_agent_task_path_traversal() {
    let data_dir = temp_data_dir("agent-task-traversal");
    fs::write(data_dir.join("config.json"), "{}").unwrap();

    let error = load_agent_task_in_dir(&data_dir, "../config").unwrap_err();

    assert!(error.contains("Invalid agent task id"));
    assert!(data_dir.join("config.json").exists());
}

#[test]
fn storage_rejects_artifact_article_id_path_traversal() {
    let data_dir = temp_data_dir("artifact-traversal");
    let mut artifact = sample_artifact();
    artifact.article_id = "../outside".to_string();

    let error = save_artifact_in_dir(&data_dir, &artifact).unwrap_err();

    assert!(error.contains("Invalid artifact article id"));
    assert!(!data_dir.join("artifacts/outside").exists());
}

#[test]
fn storage_rejects_article_update_path_traversal() {
    let data_dir = temp_data_dir("article-update-traversal");
    fs::write(data_dir.join("config.json"), "{}").unwrap();

    let error = update_article_active_mind_map_artifact_in_dir(
        &data_dir,
        "../config.json",
        Some("artifact-1".to_string()),
    )
    .unwrap_err();

    assert!(error.contains("Invalid article id"));
    assert!(data_dir.join("config.json").exists());
}
