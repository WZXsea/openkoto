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
    document_editing::initialize_pending_documents,
    routes::{build_router, AppState},
};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_SECRET: &str = "0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn startup_backfill_initializes_every_legacy_document_before_first_read() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .connect(&database_url)
        .await
        .unwrap();
    MIGRATOR.run(&pool).await.unwrap();
    let user_id = Uuid::new_v4();
    let material_id = Uuid::new_v4();
    let segment_id = Uuid::new_v4();
    let email = format!("startup-backfill-{user_id}@example.com");
    sqlx::query(
        "INSERT INTO users (id, email, email_normalized, password_hash) VALUES ($1, $2, $2, $3)",
    )
    .bind(user_id)
    .bind(email)
    .bind("test-password-hash")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO materials (id, user_id, title, content, source_type) VALUES ($1, $2, $3, $4, 'article')",
    )
    .bind(material_id)
    .bind(user_id)
    .bind("Legacy startup document")
    .bind("Legacy startup sentence.")
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO material_segments (id, user_id, material_id, segment_order, text, is_new_paragraph) VALUES ($1, $2, $3, 0, $4, TRUE)",
    )
    .bind(segment_id)
    .bind(user_id)
    .bind(material_id)
    .bind("Legacy startup sentence.")
    .execute(&pool)
    .await
    .unwrap();

    assert!(initialize_pending_documents(&pool).await.unwrap() >= 1);
    assert_eq!(initialize_pending_documents(&pool).await.unwrap(), 0);
    let counts = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        SELECT
          (SELECT COUNT(*) FROM material_blocks WHERE user_id = $1 AND material_id = $2 AND deleted_at IS NULL),
          (SELECT COUNT(*) FROM material_segments WHERE user_id = $1 AND material_id = $2 AND id = $3 AND block_id IS NOT NULL),
          (SELECT COUNT(*) FROM material_revisions WHERE user_id = $1 AND material_id = $2 AND revision = 1),
          (SELECT COUNT(*) FROM material_document_migration_reports WHERE user_id = $1 AND material_id = $2)
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .bind(segment_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1, 1, 1));
}

#[tokio::test]
async fn structured_document_editing_preserves_identity_versions_and_user_isolation() {
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
        std::env::temp_dir().join(format!("openkoto-document-test-{}", Uuid::new_v4()));
    let app = test_app(pool.clone(), database_url, storage_dir.clone());
    let token_a = register_user(app.clone(), "document-a").await;
    let token_b = register_user(app.clone(), "document-b").await;

    let (status, material) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "HIF review",
            "content": "HIF signaling shapes tumors. It also changes metabolism.",
            "source_type": "article"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let material_id = material["id"].as_str().unwrap();

    let (status, deprecated_write) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/materials/{material_id}"),
        json!({"content": "bypass the revision chain"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        deprecated_write["error"]["code"],
        "deprecated_document_write"
    );

    let (status, document) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}/document"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(document["current_revision"], 1);
    assert_eq!(document["migration_report"]["content_mismatch"], false);
    let block_id = document["blocks"][0]["id"].as_str().unwrap();
    let first_segment_id = document["blocks"][0]["segments"][0]["id"].as_str().unwrap();
    let first_segment_hash = document["blocks"][0]["segments"][0]["text_sha256"]
        .as_str()
        .unwrap();
    let second_segment_id = document["blocks"][0]["segments"][1]["id"].as_str().unwrap();
    let second_segment_hash = document["blocks"][0]["segments"][1]["text_sha256"]
        .as_str()
        .unwrap();
    let (status, material_with_hashes) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        material_with_hashes["segments"][0]["text_sha256"],
        first_segment_hash
    );
    let (status, derived_document) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/materials/{material_id}/segments/derived"),
        json!({
            "updates": [{
                "segment_id": first_segment_id,
                "expected_text_sha256": first_segment_hash,
                "reading_text": "HIF signaling shapes tumors.",
                "translation": "HIF 信号塑造肿瘤。"
            }]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        derived_document["blocks"][0]["segments"][0]["translation_status"],
        "current"
    );

    let (status, learning_item) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/from-selection",
        json!({
            "material_id": material_id,
            "segment_id": first_segment_id,
            "selected_text": "HIF",
            "source_sentence": "HIF signaling shapes tumors."
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let learning_item_id = learning_item["id"].as_str().unwrap();

    let annotation_request_id = format!("annotation-{}", Uuid::new_v4());
    let (status, annotation) = json_request(
        app.clone(),
        Method::POST,
        "/annotations",
        json!({
            "material_id": material_id,
            "segment_id": first_segment_id,
            "kind": "highlight",
            "locator": {
                "version": 1,
                "reader_kind": "article",
                "kind": "text_range",
                "segment_id": first_segment_id,
                "segment_order": 0,
                "start_offset": 0,
                "end_offset": 3,
                "quote": {"exact": "HIF"}
            },
            "source_text": "HIF signaling shapes tumors.",
            "client_request_id": annotation_request_id
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let annotation_id = annotation["id"].as_str().unwrap();

    let blocks = json!([
        {
            "id": block_id,
            "block_type": "paragraph",
            "block_order": 0,
            "text": "HIF signaling strongly shapes tumors. It also changes metabolism.",
            "attrs": {}
        },
        {
            "block_type": "quote",
            "block_order": 1,
            "text": "Hypoxia remains clinically important.",
            "attrs": {}
        }
    ]);
    let (status, preview) = json_request(
        app.clone(),
        Method::POST,
        &format!("/materials/{material_id}/document/preview"),
        json!({"base_revision": 1, "blocks": blocks}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["impact"]["inserted_blocks"], 1);
    assert_eq!(preview["impact"]["affected_annotations"], 1);
    assert_eq!(preview["impact"]["affected_learning_items"], 1);
    assert_eq!(preview["impact"]["stale_translations"], 1);
    assert_eq!(preview["impact"]["annotation_reanchors"]["exact"], 1);
    let stale_preview_token = preview["preview_token"].as_str().unwrap().to_string();

    let (status, _) = json_request(
        app.clone(),
        Method::POST,
        "/annotations",
        json!({
            "material_id": material_id,
            "segment_id": first_segment_id,
            "kind": "highlight",
            "locator": {
                "version": 1,
                "reader_kind": "article",
                "kind": "text_range",
                "segment_id": first_segment_id,
                "segment_order": 0,
                "start_offset": 0,
                "end_offset": 3,
                "quote": {"exact": "HIF"}
            },
            "source_text": "HIF signaling shapes tumors.",
            "client_request_id": format!("annotation-after-preview-{}", Uuid::new_v4())
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, changed_preview) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/document"),
        json!({
            "base_revision": 1,
            "client_request_id": format!("changed-preview-{}", Uuid::new_v4()),
            "preview_token": stale_preview_token,
            "blocks": blocks
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(changed_preview["error"]["code"], "document_preview_changed");

    let (status, preview) = json_request(
        app.clone(),
        Method::POST,
        &format!("/materials/{material_id}/document/preview"),
        json!({"base_revision": 1, "blocks": blocks}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preview["impact"]["annotation_reanchors"]["exact"], 2);
    let preview_token = preview["preview_token"].as_str().unwrap().to_string();
    let (status, _) = json_request(
        app.clone(),
        Method::PATCH,
        &format!("/materials/{material_id}/segments/derived"),
        json!({
            "updates": [{
                "segment_id": second_segment_id,
                "expected_text_sha256": second_segment_hash,
                "translation": "它也会改变代谢。"
            }]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let edit_request_id = format!("document-edit-{}", Uuid::new_v4());
    let (status, committed) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/document"),
        json!({
            "base_revision": 1,
            "client_request_id": edit_request_id,
            "preview_token": preview_token,
            "blocks": blocks
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(committed["document"]["current_revision"], 2);
    assert_eq!(
        committed["document"]["blocks"][0]["segments"][0]["id"],
        first_segment_id
    );
    assert_eq!(
        committed["document"]["blocks"][0]["segments"][0]["translation_status"],
        "stale"
    );
    assert_eq!(
        committed["document"]["blocks"][0]["segments"][1]["translation"],
        "它也会改变代谢。"
    );
    assert_eq!(
        committed["document"]["blocks"][0]["segments"][1]["translation_status"],
        "current"
    );
    let quote_segment_id = committed["document"]["blocks"][1]["segments"][0]["id"]
        .as_str()
        .unwrap();
    let (status, quote_learning_item) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items/from-selection",
        json!({
            "material_id": material_id,
            "segment_id": quote_segment_id,
            "selected_text": "Hypoxia",
            "source_sentence": "Hypoxia remains clinically important."
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let quote_learning_item_id = quote_learning_item["id"].as_str().unwrap();
    let (status, idempotent_retry) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/document"),
        json!({
            "base_revision": 1,
            "client_request_id": edit_request_id,
            "preview_token": preview_token,
            "blocks": blocks
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(idempotent_retry["document"]["current_revision"], 2);

    let (status, learning_item) = json_request(
        app.clone(),
        Method::GET,
        &format!("/learning-items/{learning_item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(learning_item["segment_id"], first_segment_id);
    assert_eq!(learning_item["source_status"], "changed");
    let (status, annotation) = json_request(
        app.clone(),
        Method::GET,
        &format!("/annotations/{annotation_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(annotation["segment_id"], first_segment_id);

    let (status, conflict) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/document"),
        json!({
            "base_revision": 1,
            "client_request_id": format!("stale-{}", Uuid::new_v4()),
            "preview_token": preview_token,
            "blocks": blocks
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(conflict["error"]["code"], "material_revision_conflict");

    let (status, no_draft) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}/document/draft"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(no_draft.is_null());
    let (status, draft) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/document/draft"),
        json!({"base_revision": 2, "blocks": blocks}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(draft["is_stale"], false);
    let (status, deleted_draft) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/materials/{material_id}/document/draft"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted_draft["deleted"], true);
    let (status, _) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/document/draft"),
        json!({"base_revision": 2, "blocks": blocks}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, restored) = json_request(
        app.clone(),
        Method::POST,
        &format!("/materials/{material_id}/revisions/1/restore"),
        json!({
            "base_revision": 2,
            "client_request_id": format!("restore-{}", Uuid::new_v4()),
            "preserve_draft": true
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["document"]["current_revision"], 3);
    assert_eq!(restored["document"]["blocks"].as_array().unwrap().len(), 1);
    assert_eq!(
        restored["document"]["blocks"][0]["text"],
        "HIF signaling shapes tumors. It also changes metabolism."
    );
    let (status, retained_draft) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}/document/draft"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retained_draft["base_revision"], 2);
    assert_eq!(retained_draft["is_stale"], true);
    let deleted_segment_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM material_segments WHERE material_id = $1 AND deleted_at IS NOT NULL",
    )
    .bind(Uuid::parse_str(material_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(deleted_segment_count >= 1);
    let lineage_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM material_segment_lineage WHERE material_id = $1 AND revision IN (2, 3)",
    )
    .bind(Uuid::parse_str(material_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(lineage_count >= 4);
    let (status, restored_learning_item) = json_request(
        app.clone(),
        Method::GET,
        &format!("/learning-items/{learning_item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored_learning_item["source_status"], "current");
    let (status, deleted_source_item) = json_request(
        app.clone(),
        Method::GET,
        &format!("/learning-items/{quote_learning_item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted_source_item["source_status"], "deleted");

    let (status, orphan_preview) = json_request(
        app.clone(),
        Method::POST,
        &format!("/materials/{material_id}/document/preview"),
        json!({
            "base_revision": 3,
            "blocks": [{
                "id": restored["document"]["blocks"][0]["id"],
                "block_type": "paragraph",
                "block_order": 0,
                "text": "Oxygen biology was removed from this draft.",
                "attrs": {}
            }]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        orphan_preview["impact"]["annotation_reanchors"]["orphaned"],
        2
    );

    let (status, revisions) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}/revisions"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(revisions.as_array().unwrap().len(), 3);

    let (status, _) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}/document"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, book) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Immutable PDF",
            "content": "Extracted PDF text.",
            "source_type": "book",
            "book_type": "pdf",
            "book_path": "/tmp/example.pdf"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let book_id = book["id"].as_str().unwrap();
    let derivative_uri = format!("/materials/{book_id}/editable-derivative");
    let (first_derivative, second_derivative) = tokio::join!(
        json_request(
            app.clone(),
            Method::POST,
            &derivative_uri,
            json!({}),
            Some(&token_a),
        ),
        json_request(
            app.clone(),
            Method::POST,
            &derivative_uri,
            json!({}),
            Some(&token_a),
        )
    );
    assert_eq!(first_derivative.0, StatusCode::OK);
    assert_eq!(second_derivative.0, StatusCode::OK);
    assert_eq!(
        first_derivative.1["created"].as_bool().unwrap() as u8
            + second_derivative.1["created"].as_bool().unwrap() as u8,
        1
    );
    assert_eq!(
        first_derivative.1["derivative_material_id"],
        second_derivative.1["derivative_material_id"]
    );
    let derivative = if first_derivative.1["created"] == true {
        &first_derivative.1
    } else {
        &second_derivative.1
    };
    assert_eq!(derivative["source_material_id"], book_id);
    assert_eq!(derivative["created"], true);
    let (status, same_derivative) = json_request(
        app.clone(),
        Method::POST,
        &derivative_uri,
        json!({}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(same_derivative["created"], false);
    assert_eq!(
        same_derivative["derivative_material_id"],
        derivative["derivative_material_id"]
    );
    let relation_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM material_relations WHERE source_material_id = $1 AND relation_type = 'editable_derivative'",
    )
    .bind(Uuid::parse_str(book_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(relation_count, 1);

    let (status, disposable) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Disposable source",
            "content": format!("Disposable {}", Uuid::new_v4()),
            "source_type": "article"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let disposable_id = disposable["id"].as_str().unwrap();
    let (status, unanchored_item) = json_request(
        app.clone(),
        Method::POST,
        "/learning-items",
        json!({
            "material_id": disposable_id,
            "text": "Disposable",
            "source_sentence": "Disposable source."
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(unanchored_item["source_status"], "current");
    let unanchored_item_id = unanchored_item["id"].as_str().unwrap();
    let (status, _) = json_request(
        app.clone(),
        Method::DELETE,
        &format!("/materials/{disposable_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, missing_source_item) = json_request(
        app.clone(),
        Method::GET,
        &format!("/learning-items/{unanchored_item_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(missing_source_item["source_status"], "material_missing");

    let (status, cleared_derived) = json_request(
        app,
        Method::PATCH,
        &format!("/materials/{material_id}/segments/derived"),
        json!({
            "updates": [{
                "segment_id": first_segment_id,
                "translation": null
            }]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(cleared_derived["blocks"][0]["segments"][0]["translation"].is_null());
    assert_eq!(
        cleared_derived["blocks"][0]["segments"][0]["translation_status"],
        "missing"
    );
    assert_eq!(
        cleared_derived["blocks"][0]["segments"][0]["reading_text"],
        "HIF signaling shapes tumors."
    );
    let _ = tokio::fs::remove_dir_all(storage_dir).await;
}

#[tokio::test]
async fn stable_segment_ids_survive_cross_block_split_and_merge() {
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
        std::env::temp_dir().join(format!("openkoto-split-merge-test-{}", Uuid::new_v4()));
    let app = test_app(pool.clone(), database_url, storage_dir.clone());
    let token = register_user(app.clone(), "document-split-merge").await;
    let (status, material) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Split merge",
            "content": "Alpha stays stable. Beta stays stable.",
            "source_type": "article"
        }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let material_id = material["id"].as_str().unwrap();
    let (status, original) = json_request(
        app.clone(),
        Method::GET,
        &format!("/materials/{material_id}/document"),
        Value::Null,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let original_block_id = original["blocks"][0]["id"].as_str().unwrap();
    let alpha_id = original["blocks"][0]["segments"][0]["id"].as_str().unwrap();
    let beta_id = original["blocks"][0]["segments"][1]["id"].as_str().unwrap();
    let split_blocks = json!([
        {
            "id": original_block_id,
            "block_type": "paragraph",
            "block_order": 0,
            "text": "Alpha stays stable.",
            "attrs": {}
        },
        {
            "block_type": "paragraph",
            "block_order": 1,
            "text": "Beta stays stable.",
            "attrs": {}
        }
    ]);
    let (status, split_preview) = json_request(
        app.clone(),
        Method::POST,
        &format!("/materials/{material_id}/document/preview"),
        json!({"base_revision": 1, "blocks": split_blocks}),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, split) = json_request(
        app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/document"),
        json!({
            "base_revision": 1,
            "client_request_id": format!("split-{}", Uuid::new_v4()),
            "preview_token": split_preview["preview_token"],
            "blocks": split_blocks
        }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        split["document"]["blocks"][0]["segments"][0]["id"],
        alpha_id
    );
    assert_eq!(split["document"]["blocks"][1]["segments"][0]["id"], beta_id);

    let merge_blocks = json!([{
        "id": split["document"]["blocks"][0]["id"],
        "block_type": "paragraph",
        "block_order": 0,
        "text": "Alpha stays stable. Beta stays stable.",
        "attrs": {}
    }]);
    let (status, merge_preview) = json_request(
        app.clone(),
        Method::POST,
        &format!("/materials/{material_id}/document/preview"),
        json!({"base_revision": 2, "blocks": merge_blocks}),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, merged) = json_request(
        app,
        Method::PUT,
        &format!("/materials/{material_id}/document"),
        json!({
            "base_revision": 2,
            "client_request_id": format!("merge-{}", Uuid::new_v4()),
            "preview_token": merge_preview["preview_token"],
            "blocks": merge_blocks
        }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        merged["document"]["blocks"][0]["segments"][0]["id"],
        alpha_id
    );
    assert_eq!(
        merged["document"]["blocks"][0]["segments"][1]["id"],
        beta_id
    );
    let deleted_segments = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM material_segments WHERE material_id = $1 AND deleted_at IS NOT NULL",
    )
    .bind(Uuid::parse_str(material_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(deleted_segments, 0);
    let _ = tokio::fs::remove_dir_all(storage_dir).await;
}

#[tokio::test]
async fn legacy_segment_mismatch_is_persisted_and_recoverable() {
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
        std::env::temp_dir().join(format!("openkoto-mismatch-test-{}", Uuid::new_v4()));
    let app = test_app(pool.clone(), database_url, storage_dir.clone());
    let token = register_user(app.clone(), "document-mismatch").await;
    let legacy_content = format!("legacy content {}", Uuid::new_v4());
    let (status, material) = json_request(
        app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Legacy mismatch",
            "content": legacy_content,
            "source_type": "article",
            "segments": [{
                "text": "Segments are authoritative.",
                "is_new_paragraph": true
            }]
        }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let material_id = material["id"].as_str().unwrap();
    let (status, document) = json_request(
        app,
        Method::GET,
        &format!("/materials/{material_id}/document"),
        Value::Null,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(document["blocks"][0]["text"], "Segments are authoritative.");
    assert_eq!(document["migration_report"]["source_authority"], "segments");
    assert_eq!(document["migration_report"]["content_mismatch"], true);
    assert_eq!(
        document["migration_report"]["legacy_content_backup"],
        legacy_content
    );
    let persisted = sqlx::query_as::<_, (bool, Option<String>)>(
        "SELECT content_mismatch, legacy_content_backup FROM material_document_migration_reports WHERE material_id = $1",
    )
    .bind(Uuid::parse_str(material_id).unwrap())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(persisted.0);
    assert_eq!(persisted.1.as_deref(), Some(legacy_content.as_str()));
    let canonical_content =
        sqlx::query_scalar::<_, String>("SELECT content FROM materials WHERE id = $1")
            .bind(Uuid::parse_str(material_id).unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(canonical_content, "Segments are authoritative.");
    let _ = tokio::fs::remove_dir_all(storage_dir).await;
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
    let body = serde_json::from_slice(&bytes).unwrap();
    (status, body)
}
