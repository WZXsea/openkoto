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
async fn pr10_learning_domain_is_transactional_filtered_migratable_and_persistent() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(6)
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    sqlx::raw_sql(include_str!(
        "../migrations/20260714001100_learning_domain_activity.sql"
    ))
    .execute(&pool)
    .await
    .unwrap();

    let storage_dir = std::env::temp_dir().join(format!("openkoto-pr10-test-{}", Uuid::new_v4()));
    let app = test_app(pool.clone(), database_url.clone(), storage_dir.clone());
    let (token_a, user_a) = register_user(app.clone(), "pr10-a").await;
    let (token_b, _) = register_user(app.clone(), "pr10-b").await;

    let (status, material) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "PR-10 oncology source",
            "content": "Attenuate the response in context.",
            "source_type": "article"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let material_id = material["id"].as_str().unwrap();

    let (status, candidate) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({
            "material_id": material_id,
            "item_type": "word",
            "text": "attenuate",
            "source_sentence": "Attenuate the response in context.",
            "tags": ["oncology"],
            "quality_flags": ["needs_verification"]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let item_id = candidate["id"].as_str().unwrap();
    assert_eq!(candidate["quality_flags"], json!(["needs_verification"]));

    let (status, filtered) = json_request(
        app.clone(),
        Method::GET,
        "/learning-items?status=candidate&item_type=word&source_type=article&source=oncology&tag=oncology&quality_flag=needs_verification",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(filtered.as_array().unwrap().len(), 1);

    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-status",
        json!({"ids": [item_id], "status": "rejected"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, invalid_accept) = json_request(
        app.clone(),
        Method::POST,
        &format!("/learning-items/{item_id}/accept"),
        json!({"favorite_type": "vocabulary", "pack_ids": []}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        invalid_accept["error"]["code"],
        "invalid_learning_item_transition"
    );
    let (status, restored) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-status",
        json!({"ids": [item_id], "status": "candidate"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["items"][0]["status"], "candidate");

    let pack_id = format!("pr10-pack-{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/word-packs",
        json!({
            "id": pack_id,
            "name": "PR-10 pack",
            "tags": [],
            "created_at": now,
            "updated_at": now
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, accepted) = json_request(
        app.clone(),
        Method::POST,
        &format!("/learning-items/{item_id}/accept"),
        json!({"favorite_type": "vocabulary", "pack_ids": [pack_id]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(accepted["learning_item"]["status"], "accepted");
    let (status, immutable_type) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/learning-items/{item_id}"),
        json!({"item_type": "grammar"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        immutable_type["error"]["code"],
        "accepted_learning_item_type_immutable"
    );
    let (status, edited) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/learning-items/{item_id}"),
        json!({
            "text": "attenuate edited",
            "meaning_in_context": "减弱",
            "definition_zh": "降低强度"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{edited}");
    let favorite_id = format!("learning-item-{item_id}");
    let (status, projected) = json_request(
        app.clone(),
        Method::GET,
        &format!("/favorite-vocabularies/{favorite_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(projected["word"], "attenuate edited");
    assert_eq!(projected["meaning"], "减弱");
    let (status, cleared) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/learning-items/{item_id}"),
        json!({"meaning_in_context": null, "definition_zh": null}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{cleared}");
    assert!(cleared["meaning_in_context"].is_null());
    assert!(cleared["definition_zh"].is_null());
    let (status, forbidden_delete) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/learning-items/{item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        forbidden_delete["error"]["code"],
        "accepted_learning_item_delete_forbidden"
    );
    let (status, preview_candidate) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({"item_type": "word", "text": "preview candidate"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, invalid_preview) = json_request(
        app.clone(),
        Method::POST,
        &format!(
            "/learning-items/{}/local-preview",
            preview_candidate["id"].as_str().unwrap()
        ),
        json!({"metadata": {"mode": "compact"}}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        invalid_preview["error"]["code"],
        "learning_item_preview_requires_accepted"
    );
    let (status, merge_target) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({"item_type": "word", "text": "merge target"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, forbidden_merge) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-organize",
        json!({"items": [{
            "id": item_id,
            "merge_into_id": merge_target["id"]
        }]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(forbidden_merge["failed"], 1);
    assert_eq!(
        forbidden_merge["results"][0]["error"]["code"],
        "learning_item_merge_accepted_source"
    );
    let (status, merge_source) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({"item_type": "word", "text": "merge source"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let merge_source_id = merge_source["id"].as_str().unwrap();
    let merge_target_id = merge_target["id"].as_str().unwrap();
    let (status, merged) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-organize",
        json!({"items": [{"id": merge_source_id, "merge_into_id": merge_target_id}]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{merged}");
    assert_eq!(merged["succeeded"], 1);
    for (id, code) in [
        (merge_source_id, "merged_learning_item_delete_forbidden"),
        (
            merge_target_id,
            "learning_item_merge_target_delete_forbidden",
        ),
    ] {
        let (status, body) = json_request(
            app.clone(),
            Method::DELETE,
            &format!("/learning-items/{id}"),
            Value::Null,
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], code);
    }
    let (status, restore_merged) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-status",
        json!({"ids": [merge_source_id], "status": "candidate"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restore_merged["updated"], 0);
    assert_eq!(
        restore_merged["results"][0]["error"]["code"],
        "merged_learning_item_restore_forbidden"
    );
    let canonical_pack_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM word_pack_learning_items WHERE user_id = $1 AND learning_item_id = $2",
    )
    .bind(user_a)
    .bind(Uuid::parse_str(item_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(canonical_pack_count, 1);

    let preview_key = format!("preview-{}", Uuid::new_v4());
    for metadata in [json!({"mode": "compact"}), json!({"mode": "compact"})] {
        let (status, _) = json_request(
            app.clone(),
            Method::POST,
            &format!("/learning-items/{item_id}/local-preview"),
            json!({"metadata": metadata, "idempotency_key": preview_key}),
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, idempotency_conflict) = json_request(
        app.clone(),
        Method::POST,
        &format!("/learning-items/{item_id}/local-preview"),
        json!({"metadata": {"mode": "expanded"}, "idempotency_key": preview_key}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        idempotency_conflict["error"]["code"],
        "learning_activity_idempotency_conflict"
    );

    let (status, archived) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-status",
        json!({"ids": [item_id], "status": "archived"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(archived["items"][0]["status_before_archive"], "accepted");
    let (status, visible_favorites) = json_request(
        app.clone(),
        Method::GET,
        "/favorite-vocabularies",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(visible_favorites.as_array().unwrap().is_empty());
    let (status, mut archived_projection) = json_request(
        app.clone(),
        Method::GET,
        &format!("/favorite-vocabularies/{favorite_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    archived_projection["meaning"] = json!("归档后兼容更新");
    let (status, _) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/favorite-vocabularies/{favorite_id}"),
        archived_projection,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, still_archived) = json_request(
        app.clone(),
        Method::GET,
        &format!("/learning-items/{item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(still_archived["status"], "archived");
    let (status, archived_delete) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/learning-items/{item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        archived_delete["error"]["code"],
        "accepted_learning_item_delete_forbidden"
    );
    let (status, reaccepted) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-organize",
        json!({"items": [{"id": item_id, "status": "accepted", "pack_ids": []}]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reaccepted["succeeded"], 1);
    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/word-packs/{pack_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let canonical_pack_id = sqlx::query_scalar::<_, String>(
        "SELECT pack_id FROM word_pack_learning_items WHERE user_id = $1 AND learning_item_id = $2",
    )
    .bind(user_a)
    .bind(Uuid::parse_str(item_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(canonical_pack_id, "system-ungrouped");
    let projected_pack_id = sqlx::query_scalar::<_, String>(
        "SELECT pack_id FROM favorite_vocabulary_packs WHERE user_id = $1 AND vocabulary_id = $2",
    )
    .bind(user_a)
    .bind(&favorite_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(projected_pack_id, "system-ungrouped");
    let canonical_pack_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM word_pack_learning_items WHERE user_id = $1 AND learning_item_id = $2",
    )
    .bind(user_a)
    .bind(Uuid::parse_str(item_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(canonical_pack_count, 1);

    let (status, partial) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/bulk-organize",
        json!({"items": [
            {"id": item_id, "quality_flags": []},
            {"id": Uuid::new_v4(), "quality_flags": ["needs_verification"]}
        ]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(partial["succeeded"], 1);
    assert_eq!(partial["failed"], 1);

    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/favorite-vocabularies/{favorite_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, archived_after_projection_delete) = json_request(
        app.clone(),
        Method::GET,
        &format!("/learning-items/{item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(archived_after_projection_delete["status"], "archived");
    let canonical_still_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM learning_items WHERE user_id = $1 AND id = $2)",
    )
    .bind(user_a)
    .bind(Uuid::parse_str(item_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(canonical_still_exists);

    for progress in [0.11, 0.12] {
        let (status, _) = json_request(
            app.clone(),
            Method::PUT,
            &format!("/materials/{material_id}/reading-progress"),
            json!({
                "reader_kind": "article",
                "locator": {"paragraph": 1},
                "progress_ratio": progress,
                "status": "reading"
            }),
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let read_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM learning_activity_events WHERE user_id = $1 AND material_id = $2 AND event_type = 'read'",
    )
    .bind(user_a)
    .bind(Uuid::parse_str(material_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(read_count, 1);

    let (status, other_events) = json_request(
        app.clone(),
        Method::GET,
        "/learning-activity-events",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(other_events.as_array().unwrap().is_empty());

    let legacy_id = format!("legacy-pr10-{}", Uuid::new_v4());
    sqlx::query(
        r#"
        INSERT INTO favorite_vocabularies (
            user_id, id, word, meaning, due_date, created_at
        ) VALUES ($1, $2, 'legacy term', 'legacy meaning', '2026-07-14', $3)
        "#,
    )
    .bind(user_a)
    .bind(&legacy_id)
    .bind(Utc::now().to_rfc3339())
    .execute(&pool)
    .await
    .unwrap();
    let (status, preview) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/compatibility-migration",
        json!({"dry_run": true}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["planned"], 1);
    let (status, applied) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/compatibility-migration",
        json!({"dry_run": false}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{applied}");
    assert_eq!(applied["migrated"], 1);
    let (status, repeated) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/compatibility-migration",
        json!({"dry_run": false}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(repeated["migrated"], 0);

    let mut non_accepted_ids = Vec::new();
    for (suffix, target_status) in [
        ("candidate", None),
        ("rejected", Some("rejected")),
        ("archived", Some("archived")),
    ] {
        let (status, item) = json_request(
            app.clone(),
            Method::POST,
            "/learning-items",
            json!({
                "material_id": material_id,
                "item_type": "word",
                "text": format!("preserve {suffix}"),
                "source_sentence": format!("Preserve the {suffix} item after deleting its source.")
            }),
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let id = item["id"].as_str().unwrap().to_string();
        if let Some(target_status) = target_status {
            let (status, changed) = json_request(
                app.clone(),
                Method::POST,
                "/learning-items/bulk-status",
                json!({"ids": [&id], "status": target_status}),
                Some(&token_a),
            )
            .await;
            assert_eq!(status, StatusCode::OK, "{changed}");
        }
        non_accepted_ids.push(id);
    }

    let (status, renamed_material) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/materials/{material_id}"),
        json!({"title": "PR-10 oncology source renamed"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{renamed_material}");

    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let restarted = test_app(pool.clone(), database_url, storage_dir.clone());
    let (status, preserved) = json_request(
        restarted.clone(),
        Method::GET,
        &format!("/learning-items/{item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(preserved["material_id"].is_null());
    assert_eq!(
        preserved["source_material_title"],
        "PR-10 oncology source renamed"
    );
    assert_eq!(preserved["source_type"], "article");
    for id in non_accepted_ids {
        let (status, preserved_item) = json_request(
            restarted.clone(),
            Method::GET,
            &format!("/learning-items/{id}"),
            Value::Null,
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{preserved_item}");
        assert!(preserved_item["material_id"].is_null());
        assert_eq!(
            preserved_item["source_material_title"],
            "PR-10 oncology source renamed"
        );
    }
    let (status, snapshot_filtered) = json_request(
        restarted,
        Method::GET,
        "/learning-items?source_type=article&source=oncology",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(snapshot_filtered.as_array().unwrap().len(), 4);

    let _ = tokio::fs::remove_dir_all(storage_dir).await;
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
    assert_eq!(status, StatusCode::OK);
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
