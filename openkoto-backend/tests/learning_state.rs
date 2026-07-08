use std::{path::PathBuf, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use chrono::Utc;
use openkoto_backend::{
    config::AppConfig,
    database::MIGRATOR,
    routes::{build_router, AppState},
};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_SECRET: &str = "0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn learning_state_is_user_isolated_when_database_is_configured() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();

    let storage_dir =
        std::env::temp_dir().join(format!("openkoto-learning-test-{}", Uuid::new_v4()));
    let app = test_app(pool, database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "learning-a").await;
    let token_b = register_user(app.clone(), "learning-b").await;

    let (status, _) =
        json_request(app.clone(), Method::GET, "/word-packs", Value::Null, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, packs_a) = json_request(
        app.clone(),
        Method::GET,
        "/word-packs",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(packs_a
        .as_array()
        .unwrap()
        .iter()
        .any(|pack| pack["id"] == "system-ungrouped"));

    let (status, packs_b) = json_request(
        app.clone(),
        Method::GET,
        "/word-packs",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(packs_b
        .as_array()
        .unwrap()
        .iter()
        .any(|pack| pack["id"] == "system-ungrouped"));

    let now = Utc::now().to_rfc3339();
    let shared_pack_id = format!("shared-pack-{}", Uuid::new_v4());
    for (token, name) in [(&token_a, "A shared pack"), (&token_b, "B shared pack")] {
        let (status, pack) = json_request(
            app.clone(),
            Method::POST,
            "/word-packs",
            json!({
                "id": shared_pack_id,
                "name": name,
                "tags": ["academic"],
                "created_at": now,
                "updated_at": now,
                "is_system": false
            }),
            Some(token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(pack["id"], shared_pack_id);
        assert_eq!(pack["name"], name);
    }

    let private_pack_id = format!("private-pack-{}", Uuid::new_v4());
    let (status, private_pack) = json_request(
        app.clone(),
        Method::POST,
        "/word-packs",
        json!({
            "id": private_pack_id,
            "name": "A private pack",
            "description": "owned by A",
            "tags": ["renal"],
            "created_at": now,
            "updated_at": now,
            "is_system": false
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(private_pack["tags"][0], "renal");

    let system_attempt_id = format!("system-attempt-{}", Uuid::new_v4());
    let (status, system_attempt) = json_request(
        app.clone(),
        Method::POST,
        "/word-packs",
        json!({
            "id": system_attempt_id,
            "name": "User controlled system flag",
            "tags": [],
            "created_at": now,
            "updated_at": now,
            "is_system": true
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(system_attempt["is_system"], false);

    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/word-packs/{system_attempt_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/word-packs/{private_pack_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let vocabulary_id = format!("vocab-{}", Uuid::new_v4());
    let (status, vocabulary) = json_request(
        app.clone(),
        Method::POST,
        "/favorite-vocabularies",
        json!({
            "id": vocabulary_id,
            "word": "attenuate",
            "meaning": "to reduce the force or effect",
            "usage": "attenuate the signal",
            "source_article_id": "article-a",
            "source_article_title": "Article A",
            "pack_ids": [private_pack_id],
            "srs_state": "new",
            "ease_factor": 2.5,
            "repetitions": 0,
            "interval_days": 0,
            "due_date": "2026-07-08",
            "last_reviewed_at": null,
            "review_count": 0,
            "created_at": now
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(vocabulary["pack_ids"][0], private_pack_id);

    let (status, listed_b) = json_request(
        app.clone(),
        Method::GET,
        "/favorite-vocabularies",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed_b.as_array().unwrap().is_empty());

    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/word-packs/{private_pack_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, vocabulary_after_pack_delete) = json_request(
        app.clone(),
        Method::GET,
        &format!("/favorite-vocabularies/{vocabulary_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        vocabulary_after_pack_delete["pack_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["system-ungrouped"]
    );

    let grammar_id = format!("grammar-{}", Uuid::new_v4());
    let (status, grammar) = json_request(
        app.clone(),
        Method::POST,
        "/favorite-grammars",
        json!({
            "id": grammar_id,
            "point": "reduced relative clause",
            "explanation": "A shortened relative clause.",
            "example": "Genes expressed in tissue were analyzed.",
            "source_article_id": "article-a",
            "source_article_title": "Article A",
            "created_at": now
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(grammar["point"], "reduced relative clause");

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/favorite-grammars/{grammar_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let bookmark_id = format!("bookmark-{}", Uuid::new_v4());
    let book_path = format!("/books/{}.pdf", Uuid::new_v4());
    let (status, bookmark) = json_request(
        app.clone(),
        Method::POST,
        "/bookmarks",
        json!({
            "id": bookmark_id,
            "book_path": book_path,
            "book_type": "pdf",
            "title": "Important page",
            "note": "review later",
            "selected_text": "important sentence",
            "page_number": 12,
            "epub_cfi": null,
            "created_at": now,
            "color": "yellow"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bookmark["page_number"], 12);

    let (status, bookmarks_b) = json_request(
        app.clone(),
        Method::GET,
        &format!("/bookmarks?book_path={book_path}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(bookmarks_b.as_array().unwrap().is_empty());

    let (status, updated_bookmark) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/bookmarks/{bookmark_id}"),
        json!({
            "id": bookmark_id,
            "book_path": book_path,
            "book_type": "pdf",
            "title": "Updated page",
            "note": "kept",
            "selected_text": "important sentence",
            "page_number": 12,
            "epub_cfi": null,
            "created_at": now,
            "color": "blue"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated_bookmark["title"], "Updated page");
    assert_eq!(updated_bookmark["color"], "blue");

    let material_id = Uuid::new_v4().to_string();
    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "id": material_id,
            "title": "Learning Material",
            "content": "One sentence."
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let task_id = format!("task-{}", Uuid::new_v4());
    let (status, task) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{task_id}"),
        json!({
            "id": task_id,
            "task_type": "mind_map_generate",
            "status": "running",
            "article_id": material_id,
            "input": {
                "article_id": material_id,
                "display_language": "zh-CN",
                "max_depth": 3,
                "evidence_mode": "strict",
                "prefer_structure": "topic_tree"
            },
            "progress": 0.5,
            "stage": "generating",
            "message": "working",
            "error": null,
            "worker_session_id": null,
            "artifact_ids": [],
            "created_at": now,
            "updated_at": now,
            "started_at": now,
            "finished_at": null
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["input"]["display_language"], "zh-CN");

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/agent-tasks/{task_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let artifact_id = format!("artifact-{}", Uuid::new_v4());
    let (status, artifact) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/artifacts/{artifact_id}"),
        json!({
            "id": artifact_id,
            "task_id": task_id,
            "article_id": material_id,
            "artifact_type": "mind_map",
            "version": "1",
            "content": { "root": { "title": "Learning Material" } },
            "metadata": { "source": "test" },
            "created_at": now,
            "updated_at": now
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(artifact["content"]["root"]["title"], "Learning Material");
    assert_eq!(artifact["metadata"]["source"], "test");

    let stale_update_time = "2026-07-08T00:00:01Z";
    let terminal_update_time = "2026-07-08T00:00:10Z";
    let ordered_task_id = format!("ordered-task-{}", Uuid::new_v4());
    let (status, _) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{ordered_task_id}"),
        json!({
            "id": ordered_task_id,
            "task_type": "mind_map_generate",
            "status": "succeeded",
            "article_id": material_id,
            "input": {
                "article_id": material_id,
                "display_language": "zh-CN",
                "max_depth": 3,
                "evidence_mode": "strict",
                "prefer_structure": "topic_tree"
            },
            "progress": 1.0,
            "stage": "done",
            "message": "done",
            "error": null,
            "worker_session_id": "worker-1",
            "artifact_ids": ["artifact-a"],
            "created_at": stale_update_time,
            "updated_at": terminal_update_time,
            "started_at": stale_update_time,
            "finished_at": terminal_update_time
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, ordered_after_stale) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{ordered_task_id}"),
        json!({
            "id": ordered_task_id,
            "task_type": "mind_map_generate",
            "status": "running",
            "article_id": material_id,
            "input": {
                "article_id": material_id,
                "display_language": "zh-CN",
                "max_depth": 3,
                "evidence_mode": "strict",
                "prefer_structure": "topic_tree"
            },
            "progress": 0.3,
            "stage": "generating",
            "message": "stale progress",
            "error": null,
            "worker_session_id": "worker-1",
            "artifact_ids": [],
            "created_at": stale_update_time,
            "updated_at": stale_update_time,
            "started_at": stale_update_time,
            "finished_at": null
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(ordered_after_stale["status"], "succeeded");
    assert_eq!(ordered_after_stale["progress"], 1.0);
    assert_eq!(ordered_after_stale["artifact_ids"], json!(["artifact-a"]));

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/artifacts/{material_id}/{artifact_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let _ = tokio::fs::remove_dir_all(storage_dir).await;
}

async fn register_user(app: Router, label: &str) -> String {
    let email = format!("{label}-{}@example.com", Uuid::new_v4());
    let (status, body) = json_request(
        app,
        Method::POST,
        "/auth/register",
        json!({
            "email": email,
            "password": "correct horse battery staple"
        }),
        None,
    )
    .await;

    assert_eq!(status, StatusCode::OK);
    body["token"].as_str().unwrap().to_string()
}

fn test_app(pool: sqlx::PgPool, database_url: String, storage_dir: PathBuf) -> Router {
    let config = AppConfig::from_lookup(|key| match key {
        "DATABASE_URL" => Some(database_url.clone()),
        "OPENKOTO_JWT_SECRET" => Some(TEST_SECRET.to_string()),
        "OPENKOTO_FILE_STORAGE_DIR" => Some(storage_dir.to_string_lossy().into_owned()),
        _ => None,
    })
    .unwrap();
    let state = AppState {
        pool,
        config: Arc::new(config),
        started_at: Utc::now(),
    };

    build_router(state)
}

async fn json_request(
    app: Router,
    method: Method,
    uri: &str,
    body: Value,
    token: Option<&str>,
) -> (StatusCode, Value) {
    let request_body = if body.is_null() {
        Body::empty()
    } else {
        Body::from(body.to_string())
    };
    let (status, bytes) = raw_request(
        app,
        method,
        uri,
        request_body,
        token,
        Some("application/json"),
    )
    .await;
    let value = serde_json::from_slice(&bytes).unwrap();

    (status, value)
}

async fn raw_request(
    app: Router,
    method: Method,
    uri: &str,
    body: Body,
    token: Option<&str>,
    content_type: Option<&str>,
) -> (StatusCode, Vec<u8>) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(content_type) = content_type {
        builder = builder.header(header::CONTENT_TYPE, content_type);
    }
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app.oneshot(builder.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();

    (status, bytes)
}
