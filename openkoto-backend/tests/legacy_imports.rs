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
async fn legacy_imports_are_idempotent_and_user_isolated_when_database_is_configured() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let inspection_pool = pool.clone();

    let storage_dir =
        std::env::temp_dir().join(format!("openkoto-legacy-import-test-{}", Uuid::new_v4()));
    let app = test_app(pool, database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "legacy-a").await;
    let token_b = register_user(app.clone(), "legacy-b").await;

    let client_import_id = format!("legacy-import-{}", Uuid::new_v4());
    let payload = legacy_import_payload(&client_import_id);
    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/legacy-imports",
        payload.clone(),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, batch) = json_request(
        app.clone(),
        Method::POST,
        "/legacy-imports",
        payload.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        batch["client_import_id"].as_str().unwrap(),
        client_import_id
    );
    assert_eq!(batch["schema_version"], "legacy-import-v1");
    assert_eq!(batch["status"], "completed_with_errors");
    assert_eq!(batch["total_items"], 10);
    assert_eq!(batch["imported_items"], 8);
    assert_eq!(batch["failed_items"], 2);
    assert_eq!(batch["items"].as_array().unwrap().len(), 10);
    assert_eq!(batch["metadata"]["auth_token"], "[redacted]");

    let batch_id = batch["id"].as_str().unwrap().to_string();
    let material_target_id = item_target_id(&batch, "material", "legacy-video-id");
    assert!(Uuid::parse_str(&material_target_id).is_ok());
    assert_ne!(material_target_id, "legacy-video-id");
    assert!(batch["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["source_id"] == "legacy-vocab-invalid"
            && item["status"] == "failed"
            && item["error"]
                .as_str()
                .unwrap()
                .contains("favorite vocabulary word is required")));
    let config_item = batch["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["source_kind"] == "config")
        .unwrap();
    assert_eq!(
        config_item["payload"]["model_configs"][0]["api_key"],
        "[redacted]"
    );
    assert_eq!(
        config_item["payload"]["model_configs"][0]["access_token"],
        "[redacted]"
    );
    assert!(batch["items"].as_array().unwrap().iter().any(|item| {
        item["source_kind"]
            .as_str()
            .unwrap()
            .starts_with("unknown-")
            && item["source_id"]
                .as_str()
                .unwrap()
                .starts_with("missing-source-id-")
            && item["payload"]["api_key"] == "[redacted]"
    }));

    let (status, same_batch) = json_request(
        app.clone(),
        Method::POST,
        "/legacy-imports",
        payload.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(same_batch["id"], batch_id);
    assert_eq!(same_batch["items"].as_array().unwrap().len(), 10);

    let mut conflicting_payload = payload.clone();
    conflicting_payload["metadata"]["changed"] = json!(true);
    let (status, conflict) = json_request(
        app.clone(),
        Method::POST,
        "/legacy-imports",
        conflicting_payload,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "legacy_import_payload_conflict");

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/legacy-imports/{batch_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, batch_for_b) = json_request(
        app.clone(),
        Method::POST,
        "/legacy-imports",
        payload,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(batch_for_b["id"], batch_id);

    let (status, material) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_target_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(material["title"], "Legacy Reading");
    assert_eq!(material["segments"].as_array().unwrap().len(), 2);
    assert_eq!(material["segments"][0]["order"], 0);
    assert_eq!(material["segments"][1]["order"], 1);

    let (status, vocabulary) = json_request(
        app.clone(),
        Method::GET,
        "/favorite-vocabularies/legacy-vocab-valid",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(vocabulary["source_article_id"], material_target_id);
    assert_eq!(vocabulary["pack_ids"][0], "legacy-pack");

    let (status, task) = json_request(
        app.clone(),
        Method::GET,
        "/agent-tasks/legacy-task",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(task["article_id"], material_target_id);
    assert_eq!(task["input"]["article_id"], material_target_id);
    assert_eq!(task["input"]["evidence_mode"], "legacy_import");
    assert_eq!(task["input"]["prefer_structure"], "legacy_import");

    let (status, artifact) = json_request(
        app.clone(),
        Method::GET,
        &format!("/artifacts/{material_target_id}/legacy-artifact"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(artifact["article_id"], material_target_id);
    assert_eq!(artifact["artifact_type"], "mind_map");
    assert_eq!(artifact["version"], "1");

    let overwritten_material_id = Uuid::new_v4();
    for (suffix, title, content, source_url) in [
        (
            "old",
            "Legacy overwrite old",
            "legacy overwrite old content",
            "https://example.test/legacy-overwrite-old",
        ),
        (
            "new",
            "Legacy overwrite new",
            "legacy overwrite new content",
            "https://example.test/legacy-overwrite-new",
        ),
    ] {
        let (status, _) = json_request(
            app.clone(),
            Method::POST,
            "/legacy-imports",
            json!({
                "client_import_id": format!("legacy-overwrite-{suffix}-{}", Uuid::new_v4()),
                "materials": [{
                    "source_id": overwritten_material_id.to_string(),
                    "payload": {
                        "id": overwritten_material_id.to_string(),
                        "title": title,
                        "content": content,
                        "source_type": "web",
                        "source_url": source_url
                    }
                }]
            }),
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (normalized_url, content_hash): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT normalized_source_url,content_sha256 FROM materials WHERE id=$1")
            .bind(overwritten_material_id)
            .fetch_one(&inspection_pool)
            .await
            .unwrap();
    assert_eq!(
        normalized_url.as_deref(),
        Some("https://example.test/legacy-overwrite-new")
    );
    assert!(content_hash.is_some());

    let (status, old_fingerprint) = json_request(
        app.clone(),
        Method::POST,
        "/materials/duplicate-check",
        json!({
            "source_url": "https://example.test/legacy-overwrite-old",
            "content": "legacy overwrite old content"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(old_fingerprint["duplicate"], false);
    let (status, new_fingerprint) = json_request(
        app.clone(),
        Method::POST,
        "/materials/duplicate-check",
        json!({
            "source_url": "https://example.test/legacy-overwrite-new",
            "content": "legacy overwrite new content"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(new_fingerprint["duplicate"], true);

    let _ = tokio::fs::remove_dir_all(storage_dir).await;
}

fn legacy_import_payload(client_import_id: &str) -> Value {
    let now = Utc::now().to_rfc3339();
    json!({
        "client_import_id": client_import_id,
        "source_label": "local legacy json",
        "metadata": {
            "source": "test",
            "auth_token": "server-side-secret"
        },
        "config": {
            "theme": "light",
            "model_configs": [
                {
                    "name": "OpenAI",
                    "api_key": "sk-test",
                    "access_token": "access-secret"
                }
            ]
        },
        "failed_items": [
            {
                "source_kind": "",
                "source_id": "",
                "error": "preexisting parse failure",
                "payload": {
                    "api_key": "sk-failed"
                }
            }
        ],
        "word_packs": [
            {
                "source_id": "legacy-pack",
                "payload": {
                    "id": "legacy-pack",
                    "name": "Legacy Pack",
                    "description": "Imported pack",
                    "tags": ["legacy"],
                    "created_at": now,
                    "updated_at": now,
                    "is_system": false
                }
            }
        ],
        "materials": [
            {
                "source_id": "legacy-video-id",
                "payload": {
                    "id": "legacy-video-id",
                    "title": "Legacy Reading",
                    "content": "First sentence. Second sentence.",
                    "source_type": "youtube",
                    "source_url": "https://example.test/watch?v=legacy-video-id",
                    "translated": true,
                    "segments": [
                        {
                            "id": "legacy-segment-a",
                            "order": 7,
                            "text": "First sentence.",
                            "translation": "第一句。",
                            "is_new_paragraph": true
                        },
                        {
                            "id": "legacy-segment-b",
                            "order": 7,
                            "text": "Second sentence.",
                            "translation": "第二句。",
                            "is_new_paragraph": false
                        }
                    ]
                }
            }
        ],
        "favorite_vocabularies": [
            {
                "source_id": "legacy-vocab-valid",
                "payload": {
                    "id": "legacy-vocab-valid",
                    "word": "attenuate",
                    "meaning": "to reduce force or effect",
                    "usage": "attenuate the response",
                    "source_article_id": "legacy-video-id",
                    "source_article_title": "Legacy Reading",
                    "pack_ids": ["legacy-pack"],
                    "srs_state": "new",
                    "ease_factor": 2.5,
                    "repetitions": 0,
                    "interval_days": 0,
                    "due_date": "2026-07-09",
                    "last_reviewed_at": null,
                    "review_count": 0,
                    "created_at": now
                }
            },
            {
                "source_id": "legacy-vocab-invalid",
                "payload": {
                    "id": "legacy-vocab-invalid",
                    "word": "",
                    "meaning": "invalid row",
                    "usage": "",
                    "source_article_id": "legacy-video-id",
                    "pack_ids": ["legacy-pack"],
                    "srs_state": "unknown",
                    "ease_factor": 0.5,
                    "repetitions": -1,
                    "interval_days": -1,
                    "due_date": "2026-07-09",
                    "last_reviewed_at": null,
                    "review_count": -1,
                    "created_at": now
                }
            }
        ],
        "favorite_grammars": [
            {
                "source_id": "legacy-grammar",
                "payload": {
                    "id": "legacy-grammar",
                    "point": "reduced relative clause",
                    "explanation": "A shortened relative clause.",
                    "example": "Genes expressed in tissue were analyzed.",
                    "source_article_id": "legacy-video-id",
                    "source_article_title": "Legacy Reading",
                    "created_at": now
                }
            }
        ],
        "bookmarks": [
            {
                "source_id": "legacy-bookmark",
                "payload": {
                    "id": "legacy-bookmark",
                    "book_path": "/legacy/book.pdf",
                    "book_type": "pdf",
                    "title": "Legacy bookmark",
                    "note": "Imported note",
                    "selected_text": "Important sentence.",
                    "page_number": 2,
                    "epub_cfi": null,
                    "created_at": now,
                    "color": "yellow"
                }
            }
        ],
        "agent_tasks": [
            {
                "source_id": "legacy-task",
                "payload": {
                    "id": "legacy-task",
                    "task_type": "mind_map_generate",
                    "status": "succeeded",
                    "article_id": "legacy-video-id",
                    "input": {
                        "article_id": "legacy-video-id",
                        "display_language": "zh-CN",
                        "max_depth": 2
                    },
                    "progress": 1.0,
                    "stage": "done",
                    "message": "done",
                    "error": null,
                    "worker_session_id": null,
                    "artifact_ids": ["legacy-artifact"],
                    "created_at": now,
                    "updated_at": now,
                    "started_at": now,
                    "finished_at": now
                }
            }
        ],
        "artifacts": [
            {
                "source_id": "legacy-artifact",
                "payload": {
                    "id": "legacy-artifact",
                    "task_id": "legacy-task",
                    "article_id": "legacy-video-id",
                    "content": {
                        "root": {
                            "title": "Legacy Reading"
                        }
                    },
                    "metadata": {
                        "source": "legacy"
                    },
                    "created_at": now,
                    "updated_at": now
                }
            }
        ]
    })
}

fn item_target_id(batch: &Value, source_kind: &str, source_id: &str) -> String {
    batch["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["source_kind"] == source_kind && item["source_id"] == source_id)
        .and_then(|item| item["target_id"].as_str())
        .unwrap()
        .to_string()
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
