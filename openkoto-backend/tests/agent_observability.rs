use std::{path::PathBuf, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use chrono::{Duration, Utc};
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
async fn agent_observability_enforces_timeline_lineage_actions_and_user_isolation() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();

    let storage_dir =
        std::env::temp_dir().join(format!("openkoto-agent-observability-{}", Uuid::new_v4()));
    let app = test_app(pool, database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "agent-observability-a").await;
    let token_b = register_user(app.clone(), "agent-observability-b").await;

    let (status, material) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Owned assistant source",
            "content": "A local source for assistant action validation.",
            "source_type": "article"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{material}");
    let material_id = material["id"].as_str().unwrap();

    let task_id = format!("task-{}", Uuid::new_v4());
    let base_time = Utc::now();
    let queued = task_payload(
        &task_id,
        material_id,
        "queued",
        0.0,
        "queued",
        base_time.to_rfc3339(),
        None,
    );
    let (status, created) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{task_id}"),
        queued.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["root_task_id"], task_id);
    assert_eq!(created["attempt"], 1);
    assert_eq!(created["input_snapshot"]["article_id"], material_id);
    let (status, duplicate_created) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{task_id}"),
        queued,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{duplicate_created}");
    assert_eq!(duplicate_created, created);

    let (status, page) = json_request(
        app.clone(),
        Method::GET,
        &format!("/agent-tasks?status=queued&article_id={material_id}&limit=10&offset=0"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{page}");
    assert_eq!(page["limit"], 10);
    assert_eq!(page["offset"], 0);
    assert!(page["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["id"] == task_id));

    let running_time = (base_time + Duration::seconds(1)).to_rfc3339();
    let (status, running) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{task_id}"),
        task_payload(
            &task_id,
            material_id,
            "running",
            0.4,
            "extracting",
            running_time,
            None,
        ),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{running}");

    let event_id = format!("worker-event-{}", Uuid::new_v4());
    let event_time = (base_time + Duration::seconds(2)).to_rfc3339();
    let timeline_payload = json!({
        "event_id": event_id,
        "event_type": "checkpoint_saved",
        "from_status": "running",
        "to_status": "running",
        "stage": "extracting",
        "message": "Checkpoint persisted",
        "metadata": {"checkpoint": 1},
        "created_at": event_time
    });
    let timeline_path = format!("/agent-tasks/{task_id}/timeline");
    let (status, first_event) = json_request(
        app.clone(),
        Method::POST,
        &timeline_path,
        timeline_payload.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first_event}");
    assert_eq!(first_event["id"], event_id);
    assert_eq!(first_event["task_id"], task_id);
    let (status, duplicate_event) = json_request(
        app.clone(),
        Method::POST,
        &timeline_path,
        timeline_payload.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{duplicate_event}");
    assert_eq!(duplicate_event, first_event);
    let mut conflicting_event = timeline_payload;
    conflicting_event["message"] = json!("Different payload");
    let (status, conflict) = json_request(
        app.clone(),
        Method::POST,
        &timeline_path,
        conflicting_event,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");
    assert_eq!(
        conflict["error"]["code"],
        "timeline_event_idempotency_conflict"
    );

    let failed_time = (base_time + Duration::seconds(3)).to_rfc3339();
    let (status, failed) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{task_id}"),
        task_payload(
            &task_id,
            material_id,
            "failed",
            0.4,
            "failed",
            failed_time,
            Some("worker failed"),
        ),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{failed}");
    assert_eq!(failed["status"], "failed");

    let (status, late_artifact) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/artifacts/artifact-late-{task_id}"),
        artifact_payload(
            &format!("artifact-late-{task_id}"),
            &task_id,
            material_id,
            base_time,
        ),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{late_artifact}");
    assert_eq!(late_artifact["error"]["code"], "artifact_task_terminal");

    let (status, retry) = json_request(
        app.clone(),
        Method::POST,
        &format!("/agent-tasks/{task_id}/retry"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{retry}");
    let retry_id = retry["id"].as_str().unwrap();
    assert_eq!(retry["status"], "queued");
    assert_eq!(retry["retry_of_task_id"], task_id);
    assert_eq!(retry["root_task_id"], task_id);
    assert_eq!(retry["attempt"], 2);
    assert_eq!(retry["input_snapshot"], created["input_snapshot"]);

    let (status, cancelled) = json_request(
        app.clone(),
        Method::POST,
        &format!("/agent-tasks/{retry_id}/cancel"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cancelled}");
    assert_eq!(cancelled["status"], "cancelled");
    let (status, cancel_conflict) = json_request(
        app.clone(),
        Method::POST,
        &format!("/agent-tasks/{retry_id}/cancel"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{cancel_conflict}");

    let (status, allowed_action) = json_request(
        app.clone(),
        Method::POST,
        &format!("/agent-tasks/{task_id}/actions"),
        json!({
            "action_kind": "open_source",
            "status": "executed",
            "payload": {"material_id": material_id}
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{allowed_action}");
    assert_eq!(allowed_action["event_type"], "action_executed");
    let (status, rejected_action) = json_request(
        app.clone(),
        Method::POST,
        &format!("/agent-tasks/{task_id}/actions"),
        json!({
            "action_kind": "anki_write",
            "status": "executed",
            "payload": {"deck": "unsafe"}
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{rejected_action}");
    assert_eq!(rejected_action["event_type"], "action_rejected");
    assert_eq!(
        rejected_action["metadata"]["code"],
        "external_write_forbidden"
    );
    let (status, actions) = json_request(
        app.clone(),
        Method::GET,
        &format!("/agent-tasks/{task_id}/actions"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{actions}");
    assert_eq!(actions.as_array().unwrap().len(), 2);
    assert_eq!(actions[1]["status"], "rejected");
    assert_eq!(actions[1]["external_write"], true);

    for path in [
        format!("/agent-tasks/{task_id}"),
        format!("/agent-tasks/{task_id}/timeline"),
        format!("/agent-tasks/{task_id}/actions"),
        format!("/agent-tasks/{task_id}/artifacts"),
    ] {
        let (status, body) =
            json_request(app.clone(), Method::GET, &path, Value::Null, Some(&token_b)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
    }
    let (status, b_page) = json_request(
        app.clone(),
        Method::GET,
        "/agent-tasks",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{b_page}");
    assert!(!b_page["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|task| task["id"] == task_id));

    let succeeded_id = format!("task-succeeded-{}", Uuid::new_v4());
    let succeeded_payload = task_payload(
        &succeeded_id,
        material_id,
        "succeeded",
        1.0,
        "done",
        (base_time + Duration::seconds(10)).to_rfc3339(),
        None,
    );
    let (status, succeeded) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{succeeded_id}"),
        succeeded_payload.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{succeeded}");
    let (status, duplicate_succeeded) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{succeeded_id}"),
        succeeded_payload,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{duplicate_succeeded}");
    assert_eq!(duplicate_succeeded, succeeded);
    let (status, stale_result) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{succeeded_id}"),
        task_payload(
            &succeeded_id,
            material_id,
            "running",
            0.5,
            "running",
            (base_time + Duration::seconds(9)).to_rfc3339(),
            None,
        ),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{stale_result}");
    assert_eq!(stale_result["status"], "succeeded");
    let (status, succeeded_timeline) = json_request(
        app.clone(),
        Method::GET,
        &format!("/agent-tasks/{succeeded_id}/timeline"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{succeeded_timeline}");
    assert!(succeeded_timeline
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["event_type"] == "stale_update_ignored"));
    let artifact_id = format!("artifact-{}", Uuid::new_v4());
    let (status, artifact) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/artifacts/{artifact_id}"),
        artifact_payload(&artifact_id, &succeeded_id, material_id, base_time),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{artifact}");
    let (status, succeeded_detail) = json_request(
        app.clone(),
        Method::GET,
        &format!("/agent-tasks/{succeeded_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{succeeded_detail}");
    assert_eq!(succeeded_detail["output_version"], 1);
    assert_eq!(succeeded_detail["artifact_ids"], json!([artifact_id]));
    let (status, artifact_detail) = json_request(
        app.clone(),
        Method::GET,
        &format!("/artifacts/{artifact_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{artifact_detail}");

    let interrupted_id = format!("task-interrupted-{}", Uuid::new_v4());
    let (status, interrupted) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/agent-tasks/{interrupted_id}"),
        task_payload(
            &interrupted_id,
            material_id,
            "interrupted",
            0.5,
            "interrupted",
            (base_time + Duration::seconds(20)).to_rfc3339(),
            Some("legacy interruption"),
        ),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{interrupted}");
    assert_eq!(interrupted["status"], "failed");
    assert_eq!(interrupted["legacy_status"], "interrupted");

    let (status, timeline) = json_request(
        app.clone(),
        Method::GET,
        &timeline_path,
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{timeline}");
    assert!(timeline
        .as_array()
        .unwrap()
        .iter()
        .all(|event| event["task_id"] == task_id));
    assert!(timeline
        .as_array()
        .unwrap()
        .iter()
        .any(|event| event["event_type"] == "checkpoint_saved"));

    let _ = tokio::fs::remove_dir_all(storage_dir).await;
}

fn task_payload(
    id: &str,
    article_id: &str,
    status: &str,
    progress: f64,
    stage: &str,
    updated_at: String,
    error: Option<&str>,
) -> Value {
    json!({
        "id": id,
        "task_type": "assistant_process",
        "status": status,
        "article_id": article_id,
        "input": {"article_id": article_id, "operation": "summarize"},
        "progress": progress,
        "stage": stage,
        "message": stage,
        "error": error,
        "worker_session_id": "worker-test",
        "artifact_ids": [],
        "created_at": updated_at,
        "updated_at": updated_at,
        "started_at": if status == "queued" { Value::Null } else { json!(updated_at) },
        "finished_at": if matches!(status, "succeeded" | "failed" | "cancelled" | "interrupted") {
            json!(updated_at)
        } else {
            Value::Null
        }
    })
}

fn artifact_payload(
    id: &str,
    task_id: &str,
    article_id: &str,
    now: chrono::DateTime<Utc>,
) -> Value {
    json!({
        "id": id,
        "task_id": task_id,
        "article_id": article_id,
        "artifact_type": "assistant_markdown",
        "version": "1",
        "content": {"markdown": "# Result"},
        "metadata": {"source": "test"},
        "created_at": now.to_rfc3339(),
        "updated_at": now.to_rfc3339()
    })
}

async fn register_user(app: Router, label: &str) -> String {
    let (status, body) = json_request(
        app,
        Method::POST,
        "/auth/register",
        json!({
            "email": format!("{label}-{}@example.com", Uuid::new_v4()),
            "password": "correct horse battery staple"
        }),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
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
    build_router(AppState {
        pool,
        config: Arc::new(config),
        started_at: Utc::now(),
    })
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
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = token {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let response = app
        .oneshot(builder.body(request_body).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value = serde_json::from_slice(&bytes).unwrap();
    (status, value)
}
