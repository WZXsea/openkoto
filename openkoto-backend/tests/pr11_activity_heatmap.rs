use std::{path::PathBuf, sync::Arc};

use axum::{
    body::{to_bytes, Body},
    http::{header, Method, Request, StatusCode},
    Router,
};
use chrono::{DateTime, Duration, Utc};
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
async fn activity_heatmap_is_timezone_safe_zero_filled_capped_and_user_scoped() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .after_connect(|connection, _metadata| {
            Box::pin(async move {
                sqlx::query("SET TIME ZONE 'America/Los_Angeles'")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let session_timezone = sqlx::query_scalar::<_, String>("SHOW TIME ZONE")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(session_timezone, "America/Los_Angeles");

    let storage_dir = std::env::temp_dir().join(format!("openkoto-pr11-test-{}", Uuid::new_v4()));
    let app = test_app(pool.clone(), database_url.clone(), storage_dir);
    let (token_a, user_a) = register_user(app.clone(), "pr11-heatmap-a").await;
    let (token_b, user_b) = register_user(app.clone(), "pr11-heatmap-b").await;
    let material_a1 = create_material(app.clone(), &token_a, "A one").await;
    let material_a2 = create_material(app.clone(), &token_a, "A two").await;
    let material_b = create_material(app.clone(), &token_b, "B one").await;

    insert_event(
        &pool,
        user_a,
        Some(material_a1),
        "read",
        "2026-07-11T15:59:59Z",
    )
    .await;
    for (material_id, occurred_at) in [
        (material_a1, "2026-07-12T16:00:00Z"),
        (material_a1, "2026-07-12T18:00:00Z"),
        (material_a2, "2026-07-12T19:00:00Z"),
    ] {
        insert_event(&pool, user_a, Some(material_id), "read", occurred_at).await;
    }
    for (index, event_type) in [
        "create",
        "accept",
        "reject",
        "archive",
        "restore",
        "organize",
        "local_preview",
        "merge",
    ]
    .into_iter()
    .enumerate()
    {
        let occurred_at = format!("2026-07-12T20:{index:02}:00Z");
        insert_event(&pool, user_a, Some(material_a1), event_type, &occurred_at).await;
    }
    insert_event(
        &pool,
        user_a,
        Some(material_a1),
        "migrate",
        "2026-07-12T20:30:00Z",
    )
    .await;
    insert_event(
        &pool,
        user_a,
        Some(material_a1),
        "create",
        "2026-07-13T16:00:00Z",
    )
    .await;
    insert_event(
        &pool,
        user_a,
        Some(material_a1),
        "create",
        "2026-07-14T16:00:00Z",
    )
    .await;
    insert_event(
        &pool,
        user_b,
        Some(material_b),
        "read",
        "2026-07-12T17:00:00Z",
    )
    .await;
    insert_event(
        &pool,
        user_b,
        Some(material_b),
        "accept",
        "2026-07-12T17:01:00Z",
    )
    .await;

    let (status, heatmap) = json_request(
        app.clone(),
        Method::GET,
        "/learning-review/activity-heatmap?start_date=2026-07-12&end_date=2026-07-14&timezone_offset_minutes=480",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{heatmap}");
    assert_eq!(heatmap["start_date"], "2026-07-12");
    assert_eq!(heatmap["end_date"], "2026-07-14");
    assert_eq!(heatmap["days"].as_array().unwrap().len(), 3);
    assert_eq!(
        heatmap["days"][0],
        json!({
            "date": "2026-07-12",
            "read_materials": 0,
            "learning_actions": 0,
            "activity_score": 0
        })
    );
    assert_eq!(
        heatmap["days"][1],
        json!({
            "date": "2026-07-13",
            "read_materials": 2,
            "learning_actions": 8,
            "activity_score": 10
        })
    );
    assert_eq!(
        heatmap["days"][2],
        json!({
            "date": "2026-07-14",
            "read_materials": 0,
            "learning_actions": 1,
            "activity_score": 1
        })
    );

    let (status, other_user_heatmap) = json_request(
        app.clone(),
        Method::GET,
        "/learning-review/activity-heatmap?start_date=2026-07-13&end_date=2026-07-13&timezone_offset_minutes=480",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{other_user_heatmap}");
    assert_eq!(other_user_heatmap["days"][0]["read_materials"], 1);
    assert_eq!(other_user_heatmap["days"][0]["learning_actions"], 1);
    assert_eq!(other_user_heatmap["days"][0]["activity_score"], 2);

    let (status, default_heatmap) = json_request(
        app.clone(),
        Method::GET,
        "/learning-review/activity-heatmap?timezone_offset_minutes=480",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{default_heatmap}");
    assert_eq!(default_heatmap["days"].as_array().unwrap().len(), 84);
    let expected_end = (Utc::now() + Duration::minutes(480)).date_naive();
    assert_eq!(default_heatmap["end_date"], expected_end.to_string());
    assert_eq!(
        default_heatmap["start_date"],
        (expected_end - Duration::days(83)).to_string()
    );

    let (status, maximum_heatmap) = json_request(
        app.clone(),
        Method::GET,
        "/learning-review/activity-heatmap?start_date=2025-07-15&end_date=2026-07-15&timezone_offset_minutes=480",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{maximum_heatmap}");
    assert_eq!(maximum_heatmap["days"].as_array().unwrap().len(), 366);

    for (uri, code) in [
        (
            "/learning-review/activity-heatmap?start_date=2025-07-14&end_date=2026-07-15",
            "activity_heatmap_range_too_large",
        ),
        (
            "/learning-review/activity-heatmap?start_date=2026-07-16&end_date=2026-07-15",
            "invalid_activity_heatmap_range",
        ),
    ] {
        let (status, error) =
            json_request(app.clone(), Method::GET, uri, Value::Null, Some(&token_a)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");
        assert_eq!(error["error"]["code"], code);
    }
}

async fn insert_event(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    material_id: Option<Uuid>,
    event_type: &str,
    occurred_at: &str,
) {
    let occurred_at = DateTime::parse_from_rfc3339(occurred_at)
        .unwrap()
        .with_timezone(&Utc);
    sqlx::query(
        r#"
        INSERT INTO learning_activity_events (
            id, user_id, material_id, event_type, metadata, payload_sha256, occurred_at
        ) VALUES ($1, $2, $3, $4, '{}'::JSONB, $5, $6)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(material_id)
    .bind(event_type)
    .bind("0".repeat(64))
    .bind(occurred_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn create_material(app: Router, token: &str, title: &str) -> Uuid {
    let (status, body) = json_request(
        app,
        Method::POST,
        "/materials",
        json!({
            "title": title,
            "content": format!("Heatmap source {title}"),
            "source_type": "article"
        }),
        Some(token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    Uuid::parse_str(body["id"].as_str().unwrap()).unwrap()
}

async fn register_user(app: Router, label: &str) -> (String, Uuid) {
    let email = format!("{label}-{}@example.com", Uuid::new_v4());
    let (status, body) = json_request(
        app,
        Method::POST,
        "/auth/register",
        json!({"email": email, "password": "correct horse battery staple"}),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (
        body["token"].as_str().unwrap().to_string(),
        Uuid::parse_str(body["user"]["id"].as_str().unwrap()).unwrap(),
    )
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
    (status, serde_json::from_slice(&bytes).unwrap())
}
