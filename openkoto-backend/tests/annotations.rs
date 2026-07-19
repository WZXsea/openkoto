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
async fn annotations_cover_migration_crud_filters_idempotency_and_isolation() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260714001000_annotations.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/20260714001000_annotations.sql"))
        .execute(&pool)
        .await
        .unwrap();

    let storage_dir =
        std::env::temp_dir().join(format!("openkoto-annotations-test-{}", Uuid::new_v4()));
    let app = test_app(pool.clone(), database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "annotations-a").await;
    let token_b = register_user(app.clone(), "annotations-b").await;

    let material_a = create_material(app.clone(), &token_a, "Annotation source A").await;
    let material_b = create_material(app.clone(), &token_b, "Annotation source B").await;
    let material_a_id = material_a["id"].as_str().unwrap();
    let segment_a_id = material_a["segments"][0]["id"].as_str().unwrap();
    let segment_b_id = material_b["segments"][0]["id"].as_str().unwrap();

    let base_payload = json!({
        "material_id": material_a_id,
        "segment_id": segment_a_id,
        "kind": "vocabulary",
        "locator": {
            "kind": "text_range",
            "segment_id": segment_a_id,
            "segment_order": 0,
            "start_offset": 16,
            "end_offset": 24,
            "quote": {"exact": "mitigate", "prefix": "can ", "suffix": " damage"},
            "content_sha256": "A".repeat(64)
        },
        "source_text": "mitigate",
        "color": "yellow",
        "note": "high-value academic verb",
        "tags": ["academic", "immunology", "academic"],
        "client_request_id": format!("annotation-create-{}", Uuid::new_v4())
    });
    let first_create = json_request(
        app.clone(),
        Method::POST,
        "/annotations",
        base_payload.clone(),
        Some(&token_a),
    );
    let retry_create = json_request(
        app.clone(),
        Method::POST,
        "/annotations",
        base_payload.clone(),
        Some(&token_a),
    );
    let ((status, created), (retry_status, duplicate)) = tokio::join!(first_create, retry_create);
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(retry_status, StatusCode::OK, "{duplicate}");
    assert_eq!(created["locator"]["version"], 1);
    assert_eq!(created["locator"]["content_sha256"], "a".repeat(64));
    assert_eq!(created["tags"], json!(["academic", "immunology"]));
    assert!(created["material_revision"].is_string());
    let annotation_id = created["id"].as_str().unwrap().to_string();

    assert_eq!(duplicate["id"], annotation_id);

    let mut conflicting_payload = base_payload;
    conflicting_payload["note"] = json!("different payload");
    let (status, conflict) = json_request(
        app.clone(),
        Method::POST,
        "/annotations",
        conflicting_payload,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "annotation_idempotency_conflict");

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/annotations/{annotation_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/annotations",
        json!({
            "material_id": material_a_id,
            "segment_id": segment_b_id,
            "kind": "highlight",
            "locator": {"kind": "page", "page": 1},
            "source_text": "cross-user segment",
            "client_request_id": format!("cross-user-segment-{}", Uuid::new_v4())
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let locator_cases = [
        (
            "highlight",
            json!({"version": 1, "reader_kind": "article", "kind": "segment", "segment_order": 0, "total_segments": 1, "segment_id": segment_a_id}),
            "Macrophages can mitigate inflammatory damage.",
        ),
        (
            "excerpt",
            json!({"version": 1, "reader_kind": "pdf", "kind": "page", "page": 2, "total_pages": 5}),
            "Page excerpt",
        ),
        (
            "note",
            json!({"version": 1, "reader_kind": "epub", "kind": "cfi", "cfi": "epubcfi(/6/2!/4/1:0)"}),
            "",
        ),
        (
            "grammar",
            json!({"version": 1, "reader_kind": "media", "kind": "time_range", "current_time": 2.5, "end_time": 4.0, "duration": 10.0}),
            "can mitigate",
        ),
    ];
    for (index, (kind, locator, source_text)) in locator_cases.into_iter().enumerate() {
        let (status, body) = json_request(
            app.clone(),
            Method::POST,
            "/annotations",
            json!({
                "material_id": material_a_id,
                "kind": kind,
                "locator": locator,
                "source_text": source_text,
                "note": if kind == "note" { Some("CFI note") } else { None },
                "tags": [format!("case-{index}")],
                "client_request_id": format!("locator-case-{index}-{}", Uuid::new_v4())
            }),
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
    }

    let (status, filtered) = json_request(
        app.clone(),
        Method::GET,
        &format!(
            "/annotations?material_id={material_a_id}&kind=vocabulary&tag=academic&q=mitigate&limit=1&offset=0"
        ),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(filtered.as_array().unwrap().len(), 1);
    assert_eq!(filtered[0]["id"], annotation_id);

    let (status, future_annotations) = json_request(
        app.clone(),
        Method::GET,
        "/annotations?created_after=2999-01-01T00%3A00%3A00Z",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(future_annotations.as_array().unwrap().is_empty());

    let (status, patched) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/annotations/{annotation_id}"),
        json!({
            "kind": "excerpt",
            "color": null,
            "note": "updated note",
            "tags": ["revised"]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched["kind"], "excerpt");
    assert!(patched["color"].is_null());
    assert_eq!(patched["tags"], json!(["revised"]));

    sqlx::query("DELETE FROM material_segments WHERE id = $1")
        .bind(Uuid::parse_str(segment_a_id).unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let (status, after_segment_delete) = json_request(
        app.clone(),
        Method::GET,
        &format!("/annotations/{annotation_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(after_segment_delete["segment_id"].is_null());

    let invalid_locators = [
        json!({"version": 2, "kind": "page", "page": 1}),
        json!({"version": 1, "kind": "text_range", "start_offset": 8, "end_offset": 4, "quote": {"exact": "bad"}}),
        json!({"version": 1, "kind": "text_range", "start_offset": 1, "end_offset": 4}),
        json!({"version": 1, "kind": "page", "page": 6, "total_pages": 5}),
        json!({"version": 1, "kind": "epub_cfi", "cfi": ""}),
        json!({"version": 1, "kind": "time", "current_time": 11, "duration": 10}),
        json!({"version": 1, "kind": "page", "page": 1, "content_sha256": "bad"}),
    ];
    for locator in invalid_locators {
        let (status, error) = json_request(
            app.clone(),
            Method::POST,
            "/annotations",
            json!({
                "material_id": material_a_id,
                "kind": "highlight",
                "locator": locator,
                "source_text": "invalid locator",
                "client_request_id": format!("invalid-locator-{}", Uuid::new_v4())
            }),
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["error"]["code"], "invalid_annotation_locator");
    }

    let (status, deleted) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/annotations/{annotation_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["deleted"], true);
    let _ = tokio::fs::remove_dir_all(storage_dir).await;
}

#[tokio::test]
async fn annotation_conversion_is_transactional_idempotent_and_obeys_delete_policies() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let storage_dir =
        std::env::temp_dir().join(format!("openkoto-conversion-test-{}", Uuid::new_v4()));
    let app = test_app(pool.clone(), database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "conversion-a").await;
    let token_b = register_user(app.clone(), "conversion-b").await;
    let material = create_material(app.clone(), &token_a, "Conversion source").await;
    let material_id = material["id"].as_str().unwrap().to_string();
    let segment_id = material["segments"][0]["id"].as_str().unwrap();

    let (status, annotation) = json_request(
        app.clone(),
        Method::POST,
        "/annotations",
        json!({
            "material_id": material_id,
            "segment_id": segment_id,
            "kind": "vocabulary",
            "locator": {
                "version": 1,
                "kind": "text_range",
                "segment_id": segment_id,
                "start_offset": 16,
                "end_offset": 24,
                "quote": {"exact": "mitigate"}
            },
            "source_text": "Macrophages can mitigate inflammatory damage.",
            "note": "to make less severe",
            "tags": ["academic"],
            "client_request_id": format!("conversion-{}", Uuid::new_v4())
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let annotation_id = annotation["id"].as_str().unwrap();

    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        &format!("/annotations/{annotation_id}/convert-to-learning-item"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let conversion_uri = format!("/annotations/{annotation_id}/convert-to-learning-item");
    let first_conversion = json_request(
        app.clone(),
        Method::POST,
        &conversion_uri,
        Value::Null,
        Some(&token_a),
    );
    let retry_conversion = json_request(
        app.clone(),
        Method::POST,
        &conversion_uri,
        Value::Null,
        Some(&token_a),
    );
    let ((status, converted), (retry_status, concurrent_retry)) =
        tokio::join!(first_conversion, retry_conversion);
    assert_eq!(status, StatusCode::OK, "{converted}");
    assert_eq!(retry_status, StatusCode::OK, "{concurrent_retry}");
    assert_eq!(converted["learning_item"]["status"], "candidate");
    assert_eq!(converted["learning_item"]["item_type"], "word");
    assert_eq!(converted["learning_item"]["text"], "mitigate");
    assert_eq!(
        converted["learning_item"]["source_sentence"],
        "Macrophages can mitigate inflammatory damage."
    );
    assert_eq!(
        converted["learning_item"]["review_state"]["annotation_id"],
        annotation_id
    );
    let first_learning_item_id = converted["learning_item"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        converted["annotation"]["learning_item_id"],
        first_learning_item_id
    );
    assert_eq!(
        concurrent_retry["learning_item"]["id"],
        first_learning_item_id
    );

    let (status, repeated) = json_request(
        app.clone(),
        Method::POST,
        &format!("/annotations/{annotation_id}/convert-to-learning-item"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated["learning_item"]["id"], first_learning_item_id);
    let conversion_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM learning_items WHERE review_state->>'annotation_id' = $1",
    )
    .bind(annotation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(conversion_count, 1);

    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/learning-items/{first_learning_item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, after_learning_delete) = json_request(
        app.clone(),
        Method::GET,
        &format!("/annotations/{annotation_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(after_learning_delete["learning_item_id"].is_null());

    let (status, reconverted) = json_request(
        app.clone(),
        Method::POST,
        &format!("/annotations/{annotation_id}/convert-to-learning-item"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(reconverted["learning_item"]["id"], first_learning_item_id);

    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let annotation_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM annotations WHERE id = $1")
            .bind(Uuid::parse_str(annotation_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(annotation_count, 0);
    let learning_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM learning_items WHERE review_state->>'annotation_id' = $1",
    )
    .bind(annotation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(learning_count, 1);
    let preserved_material_id = sqlx::query_scalar::<_, Option<Uuid>>(
        "SELECT material_id FROM learning_items WHERE review_state->>'annotation_id' = $1",
    )
    .bind(annotation_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(preserved_material_id.is_none());
    let _ = tokio::fs::remove_dir_all(storage_dir).await;
}

async fn create_material(app: Router, token: &str, title: &str) -> Value {
    let (status, body) = json_request(
        app,
        Method::POST,
        "/materials",
        json!({
            "title": title,
            "content": "Macrophages can mitigate inflammatory damage.",
            "source_type": "article"
        }),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    body
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
    let mut builder = Request::builder().method(method).uri(uri);
    builder = builder.header(header::CONTENT_TYPE, "application/json");
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
