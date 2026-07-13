use std::{path::PathBuf, sync::Arc, time::Duration};

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
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::time::sleep;
use tower::ServiceExt;
use uuid::Uuid;

const TEST_SECRET: &str = "0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn materials_list_filters_and_duplicate_detection_follow_the_material_library_contract() {
    let Some(ctx) = TestContext::new("material-library-list").await else {
        return;
    };

    let token_a = register_user(ctx.app.clone(), "material-library-list-a").await;
    let token_b = register_user(ctx.app.clone(), "material-library-list-b").await;

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials",
        Value::Null,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let alpha = create_material(
        ctx.app.clone(),
        &token_a,
        json!({
            "title": "Alpha Reading",
            "content": "immune pathway baseline",
            "source_type": "article",
            "source_url": "https://example.com/materials/alpha"
        }),
    )
    .await;
    let beta = create_material(
        ctx.app.clone(),
        &token_a,
        json!({
            "title": "Beta Web",
            "content": "web source beta",
            "source_type": "web",
            "source_url": "https://example.com/materials/beta"
        }),
    )
    .await;
    let gamma = create_material(
        ctx.app.clone(),
        &token_a,
        json!({
            "title": "Gamma Audio",
            "content": "audio transcript gamma",
            "source_type": "audio"
        }),
    )
    .await;
    let beta_b = create_material(
        ctx.app.clone(),
        &token_b,
        json!({
            "title": "Beta Web",
            "content": "web source beta",
            "source_type": "web",
            "source_url": "https://example.com/materials/beta"
        }),
    )
    .await;

    let alpha_id = alpha["id"].as_str().unwrap().to_string();
    let beta_id = beta["id"].as_str().unwrap().to_string();
    let gamma_id = gamma["id"].as_str().unwrap().to_string();
    assert_ne!(beta_b["id"], beta["id"]);

    let tag = create_tag(
        ctx.app.clone(),
        &token_a,
        json!({
            "name": "Research",
            "color": "#2255aa"
        }),
    )
    .await;
    let tag_id = tag["id"].as_str().unwrap().to_string();

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::PUT,
        &format!("/materials/{alpha_id}/tags"),
        json!({ "tag_ids": [tag_id.clone()] }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let alpha_segment_id = alpha["segments"][0]["id"].as_str().unwrap().to_string();
    let beta_segment_id = beta["segments"][0]["id"].as_str().unwrap().to_string();

    let (status, reading_alpha) = json_request(
        ctx.app.clone(),
        Method::PUT,
        &format!("/materials/{alpha_id}/reading-progress"),
        json!({
            "reader_kind": "article",
            "status": "reading",
            "progress_ratio": 0.35,
            "locator": {
                "segment_id": alpha_segment_id,
                "offset": 12
            }
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reading_alpha["status"], "reading");
    assert_eq!(reading_alpha["progress_ratio"], json!(0.35));
    assert_eq!(reading_alpha["reader_kind"], "article");
    assert!(reading_alpha["completed_at"].is_null());

    sleep(Duration::from_millis(20)).await;

    let (status, reading_beta) = json_request(
        ctx.app.clone(),
        Method::PUT,
        &format!("/materials/{beta_id}/reading-progress"),
        json!({
            "reader_kind": "article",
            "status": "completed",
            "progress_ratio": 1.0,
            "locator": {
                "segment_id": beta_segment_id,
                "offset": 48
            }
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reading_beta["status"], "completed");
    assert_eq!(reading_beta["progress_ratio"], json!(1.0));
    assert!(reading_beta["completed_at"].is_string());

    let (status, listed_a) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids_unordered(
        &listed_a,
        &[gamma_id.clone(), beta_id.clone(), alpha_id.clone()],
    );

    let (status, listed_b) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids(&listed_b, &[beta_b["id"].as_str().unwrap().to_string()]);

    let (status, queried) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?query=immune",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids(&queried, &[alpha_id.clone()]);

    let (status, by_source_type) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?source_type=web",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids(&by_source_type, &[beta_id.clone()]);

    let (status, by_tag) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/materials?tag={tag_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids(&by_tag, &[alpha_id.clone()]);

    let (status, by_reading_status) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?reading_status=reading",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids(&by_reading_status, &[alpha_id.clone()]);

    let (status, by_unread_status) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?reading_status=unread",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids(&by_unread_status, &[gamma_id.clone()]);

    let (status, sorted_by_title) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?sort=title_asc&limit=2&offset=1",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids(&sorted_by_title, &[beta_id.clone(), gamma_id.clone()]);

    let (status, sorted_by_last_read) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?sort=last_read_at_desc",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_material_ids(
        &sorted_by_last_read,
        &[beta_id.clone(), alpha_id.clone(), gamma_id.clone()],
    );

    let (status, duplicate_url) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Duplicate URL",
            "content": "different content same source",
            "source_type": "web",
            "source_url": "https://example.com/materials/beta"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&duplicate_url, &["duplicate", "conflict"]);

    let (status, kept_copy) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Explicit Duplicate Copy",
            "content": "different content same source",
            "source_type": "web",
            "source_url": "https://example.com/materials/beta",
            "duplicate_policy": "keep_copy"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(kept_copy["title"], "Explicit Duplicate Copy");

    let (status, duplicate_check) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials/duplicate-check",
        json!({ "source_url": "https://EXAMPLE.com:443/materials/beta#fragment" }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(duplicate_check["duplicate"], true);
    assert!(duplicate_check["matches"].as_array().unwrap().len() >= 2);
    assert!(duplicate_check["matches"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["title"].is_string()));

    let (status, _duplicate_content_seed) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Content Seed",
            "content": "same content hash body",
            "source_type": "article",
            "source_url": "https://example.com/materials/content-seed"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, duplicate_content) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Content Duplicate",
            "content": "same content hash body",
            "source_type": "article",
            "source_url": "https://example.com/materials/content-duplicate"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&duplicate_content, &["duplicate", "conflict"]);

    let repeated_file_hash = hex_sha256('a');
    let (status, file_seed) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "File Seed",
            "content": "first file-backed material",
            "source_type": "article",
            "file_sha256": repeated_file_hash
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(file_seed["title"], "File Seed");

    let (status, duplicate_file_hash) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "File Duplicate",
            "content": "second file-backed material",
            "source_type": "article",
            "file_sha256": hex_sha256('a')
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&duplicate_file_hash, &["duplicate", "conflict"]);

    let (status, hash_duplicate_check) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials/duplicate-check",
        json!({
            "content": "same content hash body",
            "file_sha256": hex_sha256('a')
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let matched_by = hash_duplicate_check["matches"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|item| item["matched_by"].as_array().unwrap().iter())
        .filter_map(Value::as_str)
        .collect::<Vec<_>>();
    assert!(matched_by.contains(&"content_hash"));
    assert!(matched_by.contains(&"file_hash"));

    let (status, _cross_user_duplicate_allowed) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        json!({
            "title": "Cross User Duplicate",
            "content": "same content hash body",
            "source_type": "article",
            "source_url": "https://example.com/materials/content-duplicate"
        }),
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    ctx.cleanup().await;
}

#[tokio::test]
async fn material_tags_assignment_and_reading_progress_are_user_isolated() {
    let Some(ctx) = TestContext::new("material-library-tags").await else {
        return;
    };

    let token_a = register_user(ctx.app.clone(), "material-library-tags-a").await;
    let token_b = register_user(ctx.app.clone(), "material-library-tags-b").await;

    let material = create_material(
        ctx.app.clone(),
        &token_a,
        json!({
            "title": "Tagged Material",
            "content": "Tagged material body. Another sentence.",
            "source_type": "article"
        }),
    )
    .await;
    let material_id = material["id"].as_str().unwrap().to_string();
    let segment_id = material["segments"][0]["id"].as_str().unwrap().to_string();

    let tag_research = create_tag(
        ctx.app.clone(),
        &token_a,
        json!({
            "name": "Research",
            "color": "#111111"
        }),
    )
    .await;
    let tag_review = create_tag(
        ctx.app.clone(),
        &token_a,
        json!({
            "name": "Review",
            "color": "#222222"
        }),
    )
    .await;
    let research_id = tag_research["id"].as_str().unwrap().to_string();
    let review_id = tag_review["id"].as_str().unwrap().to_string();

    let (status, listed_tags) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/material-tags",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed_tags.as_array().unwrap().len(), 2);

    let (status, duplicate_name) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-tags",
        json!({
            "name": "research"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&duplicate_name, &["exists", "duplicate", "conflict"]);

    let (status, patched_tag) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-tags/{research_id}"),
        json!({
            "name": "Research Updated",
            "color": "#334455"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(patched_tag["name"], "Research Updated");
    assert_eq!(patched_tag["color"], "#334455");

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/tags"),
        json!({
            "tag_ids": [research_id.clone(), review_id.clone()]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, material_with_tags) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_tag_names(&material_with_tags["tags"], &["Research Updated", "Review"]);

    let missing_tag_id = Uuid::new_v4().to_string();
    let (status, invalid_assignment) = json_request(
        ctx.app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/tags"),
        json!({
            "tag_ids": [research_id.clone(), missing_tag_id]
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_error_code_contains_any(&invalid_assignment, &["tag", "not_found"]);

    let (status, unchanged_material) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_tag_names(&unchanged_material["tags"], &["Research Updated", "Review"]);

    let (status, progress_reading) = json_request(
        ctx.app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/reading-progress"),
        json!({
            "reader_kind": "article",
            "status": "reading",
            "progress_ratio": 0.4,
            "locator": {
                "segment_id": segment_id.clone(),
                "offset": 9
            }
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(progress_reading["status"], "reading");
    assert_eq!(progress_reading["progress_ratio"], json!(0.4));
    assert!(progress_reading["last_opened_at"].is_string());
    assert!(progress_reading["completed_at"].is_null());

    let (status, progress_completed) = json_request(
        ctx.app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/reading-progress"),
        json!({
            "reader_kind": "article",
            "status": "completed",
            "progress_ratio": 1.0,
            "locator": {
                "segment_id": segment_id,
                "offset": 38
            }
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(progress_completed["status"], "completed");
    assert_eq!(progress_completed["progress_ratio"], json!(1.0));
    assert!(progress_completed["completed_at"].is_string());

    let (status, material_after_progress) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        material_after_progress["reading_progress"]["status"],
        "completed"
    );
    assert_eq!(
        material_after_progress["reading_progress"]["progress_ratio"],
        json!(1.0)
    );

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/material-tags/{research_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _foreign_assignment) = json_request(
        ctx.app.clone(),
        Method::PUT,
        &format!("/materials/{material_id}/tags"),
        json!({
            "tag_ids": [research_id.clone()]
        }),
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, deleted_tag) = json_request(
        ctx.app.clone(),
        Method::DELETE,
        &format!("/material-tags/{review_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted_tag["deleted"], true);

    let (status, material_after_delete) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/materials/{material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_tag_names(&material_after_delete["tags"], &["Research Updated"]);

    ctx.cleanup().await;
}

#[tokio::test]
async fn material_import_jobs_enforce_status_transitions_and_user_isolation() {
    let Some(ctx) = TestContext::new("material-library-import-jobs").await else {
        return;
    };

    let token_a = register_user(ctx.app.clone(), "material-library-import-a").await;
    let token_b = register_user(ctx.app.clone(), "material-library-import-b").await;

    let imported_material = create_material(
        ctx.app.clone(),
        &token_a,
        json!({
            "title": "Imported Target",
            "content": "Imported target body",
            "source_type": "article"
        }),
    )
    .await;
    let imported_material_id = imported_material["id"].as_str().unwrap().to_string();

    let (status, url_job) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        json!({
            "source_kind": "url",
            "source_uri": "https://example.com/import/url-source",
            "preview": { "title": "Preview" },
            "metadata": {
                "source": "integration-test"
            }
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(url_job["source_kind"], "url");
    assert_eq!(url_job["status"], "queued");
    assert_eq!(url_job["progress"], json!(0.0));
    assert_eq!(url_job["input_hash"].as_str().unwrap().len(), 64);
    let url_job_id = url_job["id"].as_str().unwrap().to_string();

    for (next_status, progress) in [
        ("validating", 0.1),
        ("parsing", 0.35),
        ("preview_ready", 0.6),
    ] {
        let (status, updated) = json_request(
            ctx.app.clone(),
            Method::PATCH,
            &format!("/material-import-jobs/{url_job_id}"),
            json!({ "status": next_status, "progress": progress }),
            Some(&token_a),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(updated["status"], next_status);
    }
    let (status, updated) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{url_job_id}"),
        json!({
            "status": "committing",
            "progress": 0.85,
            "metadata": {
                "source": "integration-test",
                "import_commit": {
                    "duplicate_policy": "keep_copy",
                    "commit_kind": "create",
                    "target_material_id": url_job_id
                }
            }
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["status"], "committing");
    let (status, succeeded_job) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{url_job_id}"),
        json!({ "status": "succeeded", "result_material_id": imported_material_id }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(succeeded_job["status"], "succeeded");
    assert_eq!(succeeded_job["progress"], json!(1.0));
    assert!(succeeded_job["finished_at"].is_string());

    let (status, invalid_transition) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{url_job_id}"),
        json!({
            "status": "validating",
            "progress": 0.5
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&invalid_transition, &["transition", "conflict"]);

    let (status, fetched_completed_job) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/material-import-jobs/{url_job_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(fetched_completed_job["status"], "succeeded");
    assert_eq!(
        fetched_completed_job["result_material_id"],
        imported_material_id
    );

    let (status, retry_job) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        json!({
            "source_kind": "text_file",
            "source_uri": "local://retry.txt",
            "input_hash": hex_sha256('b')
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retry_job["status"], "queued");
    let retry_job_id = retry_job["id"].as_str().unwrap().to_string();

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{retry_job_id}"),
        json!({ "status": "validating" }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, failed_job) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{retry_job_id}"),
        json!({
            "status": "failed_retryable",
            "progress": 0.8,
            "error_code": "parser_failed",
            "error_message": "parser exploded"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(failed_job["status"], "failed_retryable");
    assert_eq!(failed_job["error_code"], "parser_failed");
    assert!(failed_job["finished_at"].is_string());

    let (status, retried_job) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{retry_job_id}"),
        json!({ "status": "queued" }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(retried_job["status"], "queued");
    assert_eq!(retried_job["progress"], json!(0.0));
    assert!(retried_job["error_code"].is_null());
    assert!(retried_job["error_message"].is_null());
    assert!(retried_job["started_at"].is_null());
    assert!(retried_job["finished_at"].is_null());

    let (status, listed_jobs_a) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/material-import-jobs",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed_jobs_a.as_array().unwrap().len(), 2);

    let (status, listed_jobs_b) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/material-import-jobs",
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(listed_jobs_b.as_array().unwrap().is_empty());

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/material-import-jobs/{retry_job_id}"),
        Value::Null,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _cross_user_same_url_job) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        json!({
            "source_kind": "url",
            "source_uri": "https://example.com/import/url-source"
        }),
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::DELETE,
        &format!("/materials/{imported_material_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, audit_job) = json_request(
        ctx.app.clone(),
        Method::GET,
        &format!("/material-import-jobs/{url_job_id}"),
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(audit_job["result_material_id"].is_null());

    ctx.cleanup().await;
}

#[tokio::test]
async fn bulk_material_operations_and_batched_list_aggregation_are_isolated() {
    let Some(ctx) = TestContext::new("material-library-bulk").await else {
        return;
    };
    let token_a = register_user(ctx.app.clone(), "material-library-bulk-a").await;
    let token_b = register_user(ctx.app.clone(), "material-library-bulk-b").await;
    let mut materials = Vec::new();
    for index in 0..3 {
        materials.push(
            create_material(
                ctx.app.clone(),
                &token_a,
                json!({
                    "title": format!("Batch {index}"),
                    "content": format!("Unique batch content {index}. Second sentence {index}."),
                    "source_type": "article"
                }),
            )
            .await,
        );
    }
    let foreign = create_material(ctx.app.clone(), &token_b, json!({
        "title": "Foreign batch", "content": "Foreign unique batch content", "source_type": "article"
    })).await;
    let ids = materials
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let foreign_id = foreign["id"].as_str().unwrap().to_string();
    let tag = create_tag(ctx.app.clone(), &token_a, json!({"name":"Batch Tag"})).await;
    let second_tag = create_tag(ctx.app.clone(), &token_a, json!({"name":"Merge Source"})).await;
    let tag_id = tag["id"].as_str().unwrap().to_string();
    let second_tag_id = second_tag["id"].as_str().unwrap().to_string();

    let (status, bulk_tags) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials/bulk-tags",
        json!({"ids":ids,"tag_ids":[tag_id.clone(),second_tag_id.clone()],"mode":"add"}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bulk_tags["affected"], 3);
    let (status, _) = json_request(
        ctx.app.clone(),
        Method::POST,
        &format!("/material-tags/{second_tag_id}/merge"),
        json!({"target_tag_id":tag_id}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    for (index, id) in ids.iter().enumerate().take(2) {
        let (status, _) = json_request(ctx.app.clone(), Method::PUT, &format!("/materials/{id}/reading-progress"),
            json!({"reader_kind":"article","locator":{"segment":index},"progress_ratio":0.2 + index as f64 * 0.3,"status":"reading"}), Some(&token_a)).await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, listed) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?sort=title_asc",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed.as_array().unwrap().len(), 3);
    for (index, item) in listed.as_array().unwrap().iter().enumerate() {
        assert_eq!(item["segments"].as_array().unwrap().len(), 2);
        assert_eq!(item["tags"].as_array().unwrap().len(), 1);
        if index < 2 {
            assert!(item["reading_progress"].is_object());
        }
    }

    let (status, archived) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials/bulk-archive",
        json!({"ids":[ids[0].clone(),foreign_id.clone()]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(archived["affected"], 1);
    let (_, visible) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(visible.as_array().unwrap().len(), 2);
    let (_, archived_only) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?reading_status=archived",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_material_ids(&archived_only, &[ids[0].clone()]);
    let (_, all) = json_request(
        ctx.app.clone(),
        Method::GET,
        "/materials?include_archived=true",
        Value::Null,
        Some(&token_a),
    )
    .await;
    assert_eq!(all.as_array().unwrap().len(), 3);

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials/bulk-archive",
        json!({"ids":[]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let too_many = (0..101)
        .map(|_| Uuid::new_v4().to_string())
        .collect::<Vec<_>>();
    let (status, _) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials/bulk-delete",
        json!({"ids":too_many}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, restored) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials/bulk-unarchive",
        json!({"ids":[ids[0].clone()]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(restored["affected"], 1);
    let (status, deleted) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials/bulk-delete",
        json!({"ids":[ids[1].clone(),foreign_id]}),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(deleted["affected"], 1);
    ctx.cleanup().await;
}

#[tokio::test]
async fn duplicate_rejection_is_transactional_and_patch_cannot_bypass_it() {
    let Some(ctx) = TestContext::new("material-library-duplicate-race").await else {
        return;
    };
    let token = register_user(ctx.app.clone(), "material-library-duplicate-race").await;
    let unique = Uuid::new_v4();
    let text_file = create_material(
        ctx.app.clone(),
        &token,
        json!({
            "title": "Text file source type",
            "content": format!("text file source type {unique}"),
            "source_type": "text_file",
            "source_url": format!("file:///tmp/{unique}.md")
        }),
    )
    .await;
    assert_eq!(text_file["source_type"], "text_file");
    let payload_a = json!({
        "title": "Concurrent duplicate A",
        "content": format!("concurrent duplicate body {unique}"),
        "source_type": "web",
        "source_url": format!("https://example.com/concurrent/{unique}")
    });
    let mut payload_b = payload_a.clone();
    payload_b["title"] = json!("Concurrent duplicate B");

    let first = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        payload_a,
        Some(&token),
    );
    let second = json_request(
        ctx.app.clone(),
        Method::POST,
        "/materials",
        payload_b,
        Some(&token),
    );
    let ((first_status, _), (second_status, _)) = tokio::join!(first, second);
    let statuses = [first_status, second_status];
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::OK)
            .count(),
        1
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|status| **status == StatusCode::CONFLICT)
            .count(),
        1
    );

    let original = create_material(
        ctx.app.clone(),
        &token,
        json!({
            "title": "Patch target original",
            "content": format!("patch target original body {unique}"),
            "source_type": "article",
            "source_url": format!("https://example.com/patch/original/{unique}")
        }),
    )
    .await;
    let conflicting = create_material(
        ctx.app.clone(),
        &token,
        json!({
            "title": "Patch target conflict seed",
            "content": format!("patch target conflict body {unique}"),
            "source_type": "article",
            "source_url": format!("https://example.com/patch/conflict/{unique}")
        }),
    )
    .await;
    let (status, body) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/materials/{}", original["id"].as_str().unwrap()),
        json!({ "source_url": conflicting["source_url"] }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&body, &["duplicate", "conflict"]);

    ctx.cleanup().await;
}

#[tokio::test]
async fn material_patch_preserves_omitted_sources_and_clears_explicit_null_sources() {
    let Some(ctx) = TestContext::new("material-library-replace-sources").await else {
        return;
    };
    let token = register_user(ctx.app.clone(), "material-library-replace-sources").await;
    let material = create_material(
        ctx.app.clone(),
        &token,
        json!({
            "title": "Original book",
            "content": format!("original book {}", Uuid::new_v4()),
            "source_type": "book",
            "source_url": "https://example.com/original-book",
            "media_path": "/tmp/original.mp3",
            "book_path": "/tmp/original.epub",
            "book_type": "epub"
        }),
    )
    .await;
    let material_id = material["id"].as_str().unwrap();

    let (status, preserved) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/materials/{material_id}"),
        json!({ "title": "Title only" }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(preserved["source_url"], material["source_url"]);
    assert_eq!(preserved["media_path"], material["media_path"]);
    assert_eq!(preserved["book_path"], material["book_path"]);
    assert_eq!(preserved["book_type"], material["book_type"]);

    let (status, replaced) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/materials/{material_id}"),
        json!({
            "title": "Replacement article",
            "content": format!("replacement article {}", Uuid::new_v4()),
            "source_type": "article",
            "source_url": null,
            "media_path": null,
            "book_path": null,
            "book_type": null,
            "segments": []
        }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "replace failed: {replaced}");
    assert_eq!(replaced["source_type"], "article");
    assert!(replaced["source_url"].is_null());
    assert!(replaced["media_path"].is_null());
    assert!(replaced["book_path"].is_null());
    assert!(replaced["book_type"].is_null());

    let normalized_source_url: Option<String> =
        sqlx::query_scalar("SELECT normalized_source_url FROM materials WHERE id=$1")
            .bind(Uuid::parse_str(material_id).unwrap())
            .fetch_one(&ctx.pool)
            .await
            .unwrap();
    assert!(normalized_source_url.is_none());

    ctx.cleanup().await;
}

#[tokio::test]
async fn import_job_client_id_creation_is_idempotent_and_conflicts_on_mismatch() {
    let Some(ctx) = TestContext::new("material-library-job-idempotency").await else {
        return;
    };
    let token_a = register_user(ctx.app.clone(), "material-library-job-idempotency-a").await;
    let token_b = register_user(ctx.app.clone(), "material-library-job-idempotency-b").await;
    let job_id = Uuid::new_v4();
    let payload = json!({
        "id": job_id,
        "source_kind": "url",
        "source_uri": "https://example.com/idempotent-import",
        "preview": { "title": "Idempotent preview" },
        "metadata": { "client": "integration-test" }
    });

    let first = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        payload.clone(),
        Some(&token_a),
    );
    let replay = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        payload.clone(),
        Some(&token_a),
    );
    let ((first_status, first_body), (replay_status, replay_body)) = tokio::join!(first, replay);
    assert_eq!(
        first_status,
        StatusCode::OK,
        "first create failed: {first_body}"
    );
    assert_eq!(
        replay_status,
        StatusCode::OK,
        "idempotent replay failed: {replay_body}"
    );
    assert_eq!(first_body["id"], replay_body["id"]);
    assert_eq!(first_body["created_at"], replay_body["created_at"]);
    assert_eq!(first_body["metadata"], payload["metadata"]);

    let (status, advanced) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({
            "status": "validating",
            "progress": 0.2,
            "preview": { "title": "Updated preview" },
            "metadata": { "worker": "updated" }
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(advanced["status"], "validating");
    assert_eq!(advanced["metadata"], json!({ "worker": "updated" }));

    let (status, replay_after_update) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        payload.clone(),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(replay_after_update["id"], job_id.to_string());
    assert_eq!(replay_after_update["status"], "validating");
    assert_eq!(
        replay_after_update["metadata"],
        json!({ "worker": "updated" })
    );

    let (status, mismatched) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        json!({
            "id": job_id,
            "source_kind": "url",
            "source_uri": "https://example.com/different-import"
        }),
        Some(&token_a),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        mismatched["error"]["code"],
        "material_import_job_id_conflict"
    );

    let (status, cross_user) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        payload,
        Some(&token_b),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(
        cross_user["error"]["code"],
        "material_import_job_id_conflict"
    );

    ctx.cleanup().await;
}

#[tokio::test]
async fn import_job_terminal_state_and_network_url_validation_are_enforced() {
    let Some(ctx) = TestContext::new("material-library-job-terminal").await else {
        return;
    };
    let token = register_user(ctx.app.clone(), "material-library-job-terminal").await;
    let material = create_material(
        ctx.app.clone(),
        &token,
        json!({
            "title": "Import terminal result",
            "content": format!("terminal result {}", Uuid::new_v4()),
            "source_type": "article"
        }),
    )
    .await;

    let (status, dangerous_url) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        json!({
            "source_kind": "url",
            "source_uri": "file:///tmp/private.txt",
            "input_hash": hex_sha256('c')
        }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_error_code_contains_any(&dangerous_url, &["url", "invalid"]);

    let (status, job) = json_request(
        ctx.app.clone(),
        Method::POST,
        "/material-import-jobs",
        json!({
            "source_kind": "text_file",
            "source_uri": "local://terminal.txt",
            "input_hash": hex_sha256('d')
        }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let job_id = job["id"].as_str().unwrap();
    for (state, progress) in [
        ("validating", 0.1),
        ("parsing", 0.5),
        ("preview_ready", 0.75),
    ] {
        let (status, _) = json_request(
            ctx.app.clone(),
            Method::PATCH,
            &format!("/material-import-jobs/{job_id}"),
            json!({ "status": state, "progress": progress }),
            Some(&token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let (status, missing_intent) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({ "status": "committing", "progress": 0.9 }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_error_code_contains_any(&missing_intent, &["commit_intent", "invalid"]);

    let (status, committing) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({
            "status": "committing",
            "progress": 0.9,
            "metadata": {
                "import_commit": {
                    "duplicate_policy": "keep_copy",
                    "commit_kind": "create",
                    "target_material_id": job_id
                }
            }
        }),
        Some(&token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "atomic commit transition failed: {committing}"
    );
    assert_eq!(committing["status"], "committing");
    assert_eq!(
        committing["metadata"]["import_commit"]["commit_kind"],
        "create"
    );

    let (status, metadata_only) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({
            "metadata": {
                "import_commit": {
                    "duplicate_policy": "keep_copy",
                    "commit_kind": "create",
                    "target_material_id": job_id,
                    "result_material_id": material["id"]
                }
            }
        }),
        Some(&token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "metadata patch failed: {metadata_only}"
    );
    assert_eq!(metadata_only["status"], "committing");

    let (status, body) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({ "status": "committing", "progress": 0.95 }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&body, &["already_committing", "conflict"]);

    let (status, body) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({ "status": "cancelled" }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&body, &["transition", "conflict"]);

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({
            "status": "succeeded",
            "result_material_id": material["id"],
            "progress": 1.0
        }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({ "status": "succeeded", "metadata": { "mutated": true } }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&body, &["terminal", "conflict"]);

    let (status, body) = json_request(
        ctx.app.clone(),
        Method::PATCH,
        &format!("/material-import-jobs/{job_id}"),
        json!({ "metadata": { "mutated": true } }),
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_error_code_contains_any(&body, &["terminal", "conflict"]);

    let (status, _) = json_request(
        ctx.app.clone(),
        Method::DELETE,
        &format!("/material-import-jobs/{job_id}"),
        Value::Null,
        Some(&token),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    ctx.cleanup().await;
}

#[tokio::test]
async fn material_library_migrator_runs_twice_on_the_same_pool() {
    let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
        return;
    };

    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&database_url)
        .await
        .unwrap();

    MIGRATOR.run(&pool).await.unwrap();
    MIGRATOR.run(&pool).await.unwrap();
}

struct TestContext {
    app: Router,
    pool: PgPool,
    storage_dir: PathBuf,
}

impl TestContext {
    async fn new(label: &str) -> Option<Self> {
        let database_url = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok()?;
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .unwrap();
        MIGRATOR.run(&pool).await.unwrap();

        let storage_dir = std::env::temp_dir().join(format!("{label}-{}", Uuid::new_v4()));
        let app = test_app(pool.clone(), database_url, storage_dir.clone());

        Some(Self {
            app,
            pool,
            storage_dir,
        })
    }

    async fn cleanup(self) {
        let _ = tokio::fs::remove_dir_all(self.storage_dir).await;
    }
}

async fn create_material(app: Router, token: &str, payload: Value) -> Value {
    let (status, material) =
        json_request(app, Method::POST, "/materials", payload, Some(token)).await;
    assert_eq!(status, StatusCode::OK, "create material failed: {material}");
    material
}

async fn create_tag(app: Router, token: &str, payload: Value) -> Value {
    let (status, tag) =
        json_request(app, Method::POST, "/material-tags", payload, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    tag
}

fn assert_material_ids(body: &Value, expected_ids: &[String]) {
    let actual_ids = body
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(actual_ids, expected_ids);
}

fn assert_material_ids_unordered(body: &Value, expected_ids: &[String]) {
    let mut actual_ids = body
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let mut expected = expected_ids.to_vec();
    actual_ids.sort();
    expected.sort();
    assert_eq!(actual_ids, expected);
}

fn assert_tag_names(body: &Value, expected_names: &[&str]) {
    let mut actual = body
        .as_array()
        .unwrap()
        .iter()
        .map(|tag| tag["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    let mut expected = expected_names
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
}

fn assert_error_code_contains_any(body: &Value, needles: &[&str]) {
    let code = body["error"]["code"].as_str().unwrap();
    assert!(
        needles.iter().any(|needle| code.contains(needle)),
        "expected error code `{code}` to contain one of {:?}",
        needles
    );
}

fn hex_sha256(fill: char) -> String {
    std::iter::repeat(fill).take(64).collect()
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

fn test_app(pool: PgPool, database_url: String, storage_dir: PathBuf) -> Router {
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
