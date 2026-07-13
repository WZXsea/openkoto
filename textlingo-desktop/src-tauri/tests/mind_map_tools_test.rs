use std::{fs, path::PathBuf};

use openkoto_desktop_lib::{
    commands::{
        build_article_overview, collect_article_evidence, read_article_window,
        save_mind_map_artifact_in_dir, search_article_segments, update_agent_task_progress_in_dir,
    },
    storage::save_agent_task_in_dir,
    types::{
        AgentTask, AgentTaskInput, AgentTaskStatus, AgentTaskType, Article, ArticleSegment,
        ArtifactType,
    },
};

fn temp_data_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("openkoto-tools-{}-{}", name, uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_article() -> Article {
    Article {
        id: "article-1".to_string(),
        title: "Sample Article".to_string(),
        content: "Alpha beta gamma. Delta epsilon zeta. Theta iota kappa.".to_string(),
        source_type: Some("article".to_string()),
        source_url: None,
        media_path: None,
        book_path: None,
        book_type: None,
        created_at: "2026-03-07T00:00:00Z".to_string(),
        translated: false,
        active_mind_map_artifact_id: None,
        segments: vec![
            ArticleSegment {
                id: "seg-1".to_string(),
                article_id: "article-1".to_string(),
                order: 0,
                text: "Alpha beta gamma.".to_string(),
                reading_text: None,
                translation: None,
                explanation: None,
                start_time: Some(0.0),
                end_time: Some(2.0),
                created_at: "2026-03-07T00:00:00Z".to_string(),
                is_new_paragraph: true,
            },
            ArticleSegment {
                id: "seg-2".to_string(),
                article_id: "article-1".to_string(),
                order: 1,
                text: "Delta epsilon zeta.".to_string(),
                reading_text: None,
                translation: None,
                explanation: None,
                start_time: Some(2.0),
                end_time: Some(4.0),
                created_at: "2026-03-07T00:00:00Z".to_string(),
                is_new_paragraph: false,
            },
            ArticleSegment {
                id: "seg-3".to_string(),
                article_id: "article-1".to_string(),
                order: 2,
                text: "Theta iota kappa.".to_string(),
                reading_text: None,
                translation: None,
                explanation: None,
                start_time: Some(4.0),
                end_time: Some(6.0),
                created_at: "2026-03-07T00:00:00Z".to_string(),
                is_new_paragraph: false,
            },
        ],
        metadata: serde_json::json!({}),
        tags: Vec::new(),
        reading_progress: None,
        archived_at: None,
    }
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

#[test]
fn article_get_overview_reports_expected_shape() {
    let article = sample_article();
    let overview = build_article_overview(&article);

    assert_eq!(overview.article_id, article.id);
    assert_eq!(overview.title, article.title);
    assert_eq!(overview.segment_count, 3);
    assert!(overview.has_segments);
    assert!(overview.has_timestamps);
}

#[test]
fn article_read_window_returns_stable_cursor_and_segment_ids() {
    let article = sample_article();
    let window = read_article_window(&article, 0, 24);

    assert_eq!(window.cursor, 0);
    assert!(window.end_offset > window.start_offset);
    assert!(!window.text.is_empty());
    assert!(window.has_more);
    assert_eq!(
        window.source_segment_ids,
        vec!["seg-1".to_string(), "seg-2".to_string()]
    );
    assert_eq!(window.time_range.unwrap().start, 0.0);
}

#[test]
fn article_search_returns_matching_segments() {
    let article = sample_article();
    let result = search_article_segments(&article, "epsilon", 5);

    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].segment_id, "seg-2");
    assert_eq!(result.results[0].text, "Delta epsilon zeta.");
}

#[test]
fn article_get_evidence_returns_requested_items_in_input_order() {
    let article = sample_article();
    let evidence = collect_article_evidence(&article, &["seg-3".to_string(), "seg-1".to_string()]);

    assert_eq!(evidence.items.len(), 2);
    assert_eq!(evidence.items[0].segment_id, "seg-3");
    assert_eq!(evidence.items[1].segment_id, "seg-1");
}

#[test]
fn task_report_progress_updates_stored_task() {
    let data_dir = temp_data_dir("progress");
    let task = sample_task();
    save_agent_task_in_dir(&data_dir, &task).unwrap();

    let updated = update_agent_task_progress_in_dir(
        &data_dir,
        &task.id,
        "reading".to_string(),
        0.4,
        Some("Reading source windows".to_string()),
    )
    .unwrap();

    assert!(matches!(updated.status, AgentTaskStatus::Running));
    assert_eq!(updated.stage.as_deref(), Some("reading"));
    assert_eq!(updated.message.as_deref(), Some("Reading source windows"));
    assert!((updated.progress - 0.4).abs() < f64::EPSILON);
}

#[test]
fn artifact_save_persists_mind_map_payload() {
    let data_dir = temp_data_dir("artifact-save");

    let artifact = save_mind_map_artifact_in_dir(
        &data_dir,
        "task-1",
        "article-1",
        serde_json::json!({
            "status": "applicable",
            "map": {
                "title": "Sample"
            }
        }),
    )
    .unwrap();

    assert!(matches!(artifact.artifact_type, ArtifactType::MindMap));
    assert_eq!(artifact.article_id, "article-1");

    let saved_path = data_dir
        .join("artifacts/articles/article-1")
        .join(format!("{}.json", artifact.id));
    assert!(saved_path.exists());
}
