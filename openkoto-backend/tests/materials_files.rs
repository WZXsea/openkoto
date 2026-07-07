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
async fn materials_and_files_are_user_isolated_when_database_is_configured() {
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
        std::env::temp_dir().join(format!("openkoto-backend-test-{}", Uuid::new_v4()));
    let app = test_app(pool, database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "reader-a").await;
    let token_b = register_user(app.clone(), "reader-b").await;

    let (status, created) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Academic Reading",
            "content": "Dr. Smith reviewed it. It worked.",
            "source_type": "article"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(created["title"], "Academic Reading");
    assert_eq!(created["translated"], false);
    assert_eq!(created["segments"].as_array().unwrap().len(), 2);
    let material_id = created["id"].as_str().unwrap().to_string();

    let (status, listed_a) = json_request(
        app.clone(),
        Method::GET,
        "/materials",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed_a
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["id"] == material_id));

    let (status, listed_b) = json_request(
        app.clone(),
        Method::GET,
        "/materials",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed_b.as_array().unwrap().is_empty());

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, patched) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/materials/{material_id}"),
        json!({
            "title": "Updated Reading",
            "translated": true,
            "segments": [
                {
                    "id": "local-segment-1",
                    "order": 0,
                    "text": "Updated sentence.",
                    "translation": "更新后的句子。",
                    "is_new_paragraph": true
                }
            ]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["title"], "Updated Reading");
    assert_eq!(patched["translated"], true);
    assert_eq!(patched["segments"][0]["translation"], "更新后的句子。");

    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, still_exists) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(still_exists["id"], material_id);

    let (status, uploaded) = multipart_file_request(
        app.clone(),
        "/files",
        "note.txt",
        "text/plain",
        b"hello from openkoto",
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(uploaded["original_name"], "note.txt");
    assert_eq!(uploaded["content_type"], "text/plain");
    assert_eq!(uploaded["byte_size"], 19);
    assert_eq!(uploaded["sha256"].as_str().unwrap().len(), 64);
    let file_id = uploaded["id"].as_str().unwrap().to_string();

    let (status, bytes) = raw_request(
        app.clone(),
        Method::GET,
        &format!("/files/{file_id}"),
        Body::empty(),
        Some(&token_b),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!bytes.is_empty());

    let (status, bytes) = raw_request(
        app.clone(),
        Method::GET,
        &format!("/files/{file_id}"),
        Body::empty(),
        Some(&token_a),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, b"hello from openkoto");

    let (status, _) = json_request(
        app,
        Method::DELETE,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

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

async fn multipart_file_request(
    app: Router,
    uri: &str,
    file_name: &str,
    content_type: &str,
    file_bytes: &[u8],
    token: Option<&str>,
) -> (StatusCode, Value) {
    let boundary = format!("openkoto-{}", Uuid::new_v4());
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"metadata\"\r\nContent-Type: application/json\r\n\r\n",
    );
    body.extend_from_slice(br#"{"purpose":"test"}"#);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\nContent-Type: {content_type}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(file_bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let (status, bytes) = raw_request(
        app,
        Method::POST,
        uri,
        Body::from(body),
        token,
        Some(&format!("multipart/form-data; boundary={boundary}")),
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
