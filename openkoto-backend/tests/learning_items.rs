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
async fn learning_items_cover_candidate_flow_and_user_isolation() {
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
        std::env::temp_dir().join(format!("openkoto-learning-items-test-{}", Uuid::new_v4()));
    let app = test_app(pool, database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "learning-items-a").await;
    let token_b = register_user(app.clone(), "learning-items-b").await;

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        "/learning-items",
        Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let (status, material) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Tumor immunology reading",
            "content": "Macrophages can mitigate inflammatory damage. The response is context dependent.",
            "source_type": "article"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let material_id = material["id"].as_str().unwrap();
    let segment_id = material["segments"][0]["id"].as_str().unwrap();

    let selection_payload = json!({
        "material_id": material_id,
        "segment_id": segment_id,
        "selected_text": "mitigate",
        "source_sentence": "Macrophages can mitigate inflammatory damage.",
        "context_before": "Macrophages can",
        "context_after": "inflammatory damage.",
        "tags": ["immunology", "academic"]
    });

    let (status, candidate) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/from-selection",
        selection_payload.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(candidate["text"], "mitigate");
    assert_eq!(candidate["item_type"], "word");
    assert_eq!(candidate["status"], "candidate");
    assert_eq!(candidate["material_id"], material_id);
    assert_eq!(candidate["segment_id"], segment_id);
    assert_eq!(
        candidate["source_material_title"],
        "Tumor immunology reading"
    );
    assert_eq!(candidate["source_segment_order"], 0);
    assert_eq!(candidate["tags"], json!(["immunology", "academic"]));
    let learning_item_id = candidate["id"].as_str().unwrap().to_string();

    let (status, duplicate) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/from-selection",
        selection_payload,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(duplicate["id"], learning_item_id);

    let (status, listed_a) = json_request(
        app.clone(),
        Method::GET,
        "/learning-items?status=candidate",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed_a.as_array().unwrap().len(), 1);

    let (status, listed_b) = json_request(
        app.clone(),
        Method::GET,
        "/learning-items",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed_b.as_array().unwrap().is_empty());

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/learning-items/{learning_item_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, patched) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/learning-items/{learning_item_id}"),
        json!({
            "status": "accepted",
            "meaning_in_context": "to reduce or make less severe",
            "definition_en": "to make something less harmful or serious",
            "definition_zh": "减轻；缓和",
            "collocations": [{"text": "mitigate risk"}],
            "examples": [{"text": "Early treatment can mitigate tissue damage."}],
            "priority": 40,
            "difficulty": 3,
            "review_state": {"local": "new"}
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["status"], "accepted");
    assert_eq!(
        patched["meaning_in_context"],
        "to reduce or make less severe"
    );
    assert_eq!(patched["collocations"][0]["text"], "mitigate risk");
    assert_eq!(patched["accepted_at"].is_string(), true);

    let (status, bulk) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-status",
        json!({
            "ids": [learning_item_id],
            "status": "archived"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bulk["updated"], 1);
    assert_eq!(bulk["items"][0]["status"], "archived");

    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({
            "material_id": material_id,
            "segment_id": Uuid::new_v4(),
            "text": "context dependent"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, deleted) = json_request(
        app.clone(),
        Method::DELETE,
        &format!(
            "/learning-items/{}",
            bulk["items"][0]["id"].as_str().unwrap()
        ),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["deleted"], true);

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
