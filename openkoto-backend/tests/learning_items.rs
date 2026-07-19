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

    let (status, direct_accept_error) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/learning-items/{learning_item_id}"),
        json!({ "status": "accepted" }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        direct_accept_error["error"]["code"],
        "atomic_acceptance_required"
    );

    let (status, patched) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/learning-items/{learning_item_id}"),
        json!({
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
    assert_eq!(patched["status"], "candidate");
    assert_eq!(
        patched["meaning_in_context"],
        "to reduce or make less severe"
    );
    assert_eq!(patched["collocations"][0]["text"], "mitigate risk");
    assert!(patched["accepted_at"].is_null());

    let (status, direct_bulk_accept_error) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-status",
        json!({
            "ids": [learning_item_id],
            "status": "accepted"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        direct_bulk_accept_error["error"]["code"],
        "atomic_acceptance_required"
    );

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

#[tokio::test]
async fn learning_item_acceptance_is_atomic_idempotent_and_user_isolated() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/20260713000900_learning_item_acceptance.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let storage_dir =
        std::env::temp_dir().join(format!("openkoto-learning-accept-test-{}", Uuid::new_v4()));
    let app = test_app(pool.clone(), database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "learning-accept-a").await;
    let token_b = register_user(app.clone(), "learning-accept-b").await;
    let now = Utc::now().to_rfc3339();

    let pack_a = format!("accept-pack-a-{}", Uuid::new_v4());
    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/word-packs",
        json!({
            "id": pack_a,
            "name": "Acceptance pack A",
            "tags": ["pr8"],
            "created_at": now,
            "updated_at": now
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let pack_b = format!("accept-pack-b-{}", Uuid::new_v4());
    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/word-packs",
        json!({
            "id": pack_b,
            "name": "Acceptance pack B",
            "tags": ["private"],
            "created_at": now,
            "updated_at": now
        }),
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let direct_accept_item_id = Uuid::new_v4();
    let (status, direct_create_error) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({
            "id": direct_accept_item_id,
            "item_type": "word",
            "text": "bypass",
            "status": "accepted"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        direct_create_error["error"]["code"],
        "atomic_acceptance_required"
    );
    let direct_create_count =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_items WHERE id = $1")
            .bind(direct_accept_item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(direct_create_count, 0);

    let vocabulary_item_id = Uuid::new_v4();
    let (status, vocabulary_item) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({
            "id": vocabulary_item_id,
            "item_type": "word",
            "text": "attenuate",
            "source_sentence": "The intervention may attenuate the inflammatory response.",
            "meaning_in_context": "to reduce the strength of something",
            "definition_en": "to make less severe",
            "definition_zh": "减弱；缓和",
            "examples": [{"text": "Treatment attenuated the signal."}]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(vocabulary_item["status"], "candidate");

    let (status, cross_user_error) = json_request(
        app.clone(),
        Method::POST,
        &format!("/learning-items/{vocabulary_item_id}/accept"),
        json!({
            "favorite_type": "vocabulary",
            "pack_ids": [pack_b]
        }),
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(cross_user_error["error"]["code"], "learning_item_not_found");
    let status_after_cross_user =
        sqlx::query_scalar::<_, String>("SELECT status FROM learning_items WHERE id = $1")
            .bind(vocabulary_item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(status_after_cross_user, "candidate");

    let accept_payload = json!({
        "favorite_type": "vocabulary",
        "pack_ids": [pack_a, pack_a]
    });
    let (status, accepted) = json_request(
        app.clone(),
        Method::POST,
        &format!("/learning-items/{vocabulary_item_id}/accept"),
        accept_payload.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["learning_item"]["status"], "accepted");
    assert!(accepted["learning_item"]["accepted_at"].is_string());
    assert_eq!(accepted["favorite"]["type"], "vocabulary");
    assert_eq!(accepted["favorite"]["pack_ids"], json!([pack_a]));

    let (status, repeated) = json_request(
        app.clone(),
        Method::POST,
        &format!("/learning-items/{vocabulary_item_id}/accept"),
        accept_payload,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated, accepted);

    let vocabulary_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM favorite_vocabularies WHERE learning_item_id = $1",
    )
    .bind(vocabulary_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(vocabulary_count, 1);
    let membership_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM favorite_vocabulary_packs fvp
        JOIN favorite_vocabularies fv
          ON fv.user_id = fvp.user_id AND fv.id = fvp.vocabulary_id
        WHERE fv.learning_item_id = $1
        "#,
    )
    .bind(vocabulary_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(membership_count, 1);

    let grammar_item_id = Uuid::new_v4();
    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({
            "id": grammar_item_id,
            "item_type": "grammar",
            "text": "may have + past participle",
            "source_sentence": "The intervention may have attenuated the response.",
            "meaning_in_context": "expresses a possible past event"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, accepted_grammar) = json_request(
        app.clone(),
        Method::POST,
        &format!("/learning-items/{grammar_item_id}/accept"),
        json!({
            "favorite_type": "grammar",
            "pack_ids": []
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted_grammar["favorite"]["type"], "grammar");
    let grammar_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM favorite_grammars WHERE learning_item_id = $1",
    )
    .bind(grammar_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(grammar_count, 1);

    let rollback_item_id = Uuid::new_v4();
    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({
            "id": rollback_item_id,
            "item_type": "phrase",
            "text": "context dependent",
            "source_sentence": "The response is context dependent.",
            "meaning_in_context": "dependent on the surrounding conditions"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let invalid_pack = format!("missing-pack-{}", Uuid::new_v4());
    let (status, invalid_pack_error) = json_request(
        app.clone(),
        Method::POST,
        &format!("/learning-items/{rollback_item_id}/accept"),
        json!({
            "favorite_type": "vocabulary",
            "pack_ids": [pack_a, invalid_pack]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(invalid_pack_error["error"]["code"], "word_pack_not_found");

    let rollback_state = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT li.status,
               (SELECT COUNT(*) FROM favorite_vocabularies fv
                WHERE fv.learning_item_id = li.id)::BIGINT
        FROM learning_items li
        WHERE li.id = $1
        "#,
    )
    .bind(rollback_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rollback_state, ("candidate".to_string(), 0));

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
