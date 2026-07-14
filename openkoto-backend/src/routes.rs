use std::{sync::Arc, time::Instant};

use axum::{
    extract::{DefaultBodyLimit, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::{
    annotations, auth, config::AppConfig, error::AppError, files, learning, learning_activity,
    learning_items, legacy_imports, material_library, materials,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<AppConfig>,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub service: &'static str,
    pub version: &'static str,
    pub database: DatabaseHealth,
    pub file_storage: FileStorageHealth,
    pub auth: AuthHealth,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseHealth {
    pub connected: bool,
    pub latency_ms: u128,
}

#[derive(Debug, Serialize)]
pub struct FileStorageHealth {
    pub ready: bool,
}

#[derive(Debug, Serialize)]
pub struct AuthHealth {
    pub configured: bool,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/me", get(auth::me))
        .route(
            "/materials",
            get(materials::list_materials).post(materials::create_material),
        )
        .route(
            "/materials/duplicate-check",
            post(material_library::check_material_duplicates),
        )
        .route(
            "/materials/bulk-archive",
            post(materials::bulk_archive_materials),
        )
        .route(
            "/materials/bulk-unarchive",
            post(materials::bulk_unarchive_materials),
        )
        .route(
            "/materials/bulk-delete",
            post(materials::bulk_delete_materials),
        )
        .route(
            "/materials/bulk-tags",
            post(material_library::bulk_material_tags),
        )
        .route(
            "/materials/{id}/tags",
            get(material_library::get_material_tags).put(material_library::set_material_tags),
        )
        .route(
            "/materials/{id}/reading-progress",
            get(material_library::get_reading_progress)
                .put(material_library::upsert_reading_progress),
        )
        .route(
            "/materials/{id}/learning-review",
            get(learning_activity::get_material_learning_review),
        )
        .route(
            "/materials/{id}",
            get(materials::get_material)
                .patch(materials::patch_material)
                .delete(materials::delete_material),
        )
        .route(
            "/material-tags",
            get(material_library::list_material_tags).post(material_library::create_material_tag),
        )
        .route(
            "/material-tags/{id}/merge",
            post(material_library::merge_material_tag),
        )
        .route(
            "/material-tags/{id}",
            get(material_library::get_material_tag)
                .patch(material_library::patch_material_tag)
                .delete(material_library::delete_material_tag),
        )
        .route(
            "/material-import-jobs",
            get(material_library::list_material_import_jobs)
                .post(material_library::create_material_import_job),
        )
        .route(
            "/material-import-jobs/{id}",
            get(material_library::get_material_import_job)
                .patch(material_library::patch_material_import_job)
                .delete(material_library::delete_material_import_job),
        )
        .route("/files", post(files::upload_file))
        .route("/files/{id}", get(files::download_file))
        .route(
            "/word-packs",
            get(learning::list_word_packs).post(learning::upsert_word_pack),
        )
        .route(
            "/word-packs/{id}",
            get(learning::get_word_pack)
                .patch(learning::patch_word_pack)
                .delete(learning::delete_word_pack),
        )
        .route(
            "/favorite-vocabularies",
            get(learning::list_favorite_vocabularies).post(learning::upsert_favorite_vocabulary),
        )
        .route(
            "/favorite-vocabularies/{id}",
            get(learning::get_favorite_vocabulary)
                .patch(learning::patch_favorite_vocabulary)
                .delete(learning::delete_favorite_vocabulary),
        )
        .route(
            "/favorite-grammars",
            get(learning::list_favorite_grammars).post(learning::upsert_favorite_grammar),
        )
        .route(
            "/favorite-grammars/{id}",
            get(learning::get_favorite_grammar).delete(learning::delete_favorite_grammar),
        )
        .route(
            "/bookmarks",
            get(learning::list_bookmarks).post(learning::upsert_bookmark),
        )
        .route(
            "/bookmarks/{id}",
            get(learning::get_bookmark)
                .patch(learning::patch_bookmark)
                .delete(learning::delete_bookmark),
        )
        .route(
            "/learning-items",
            get(learning_items::list_learning_items).post(learning_items::create_learning_item),
        )
        .route(
            "/learning-items/from-selection",
            post(learning_items::create_learning_item_from_selection),
        )
        .route(
            "/learning-items/bulk-status",
            post(learning_items::bulk_learning_item_status),
        )
        .route(
            "/learning-items/bulk-organize",
            post(learning_items::bulk_organize_learning_items),
        )
        .route(
            "/learning-items/compatibility-migration",
            post(learning_items::migrate_legacy_learning_items),
        )
        .route(
            "/learning-items/{id}/accept",
            post(learning_items::accept_learning_item),
        )
        .route(
            "/learning-items/{id}/local-preview",
            post(learning_activity::record_local_preview),
        )
        .route(
            "/learning-items/{id}",
            get(learning_items::get_learning_item)
                .patch(learning_items::patch_learning_item)
                .delete(learning_items::delete_learning_item),
        )
        .route(
            "/annotations",
            get(annotations::list_annotations).post(annotations::create_annotation),
        )
        .route(
            "/annotations/{id}/convert-to-learning-item",
            post(annotations::convert_to_learning_item),
        )
        .route(
            "/annotations/{id}",
            get(annotations::get_annotation)
                .patch(annotations::patch_annotation)
                .delete(annotations::delete_annotation),
        )
        .route(
            "/learning-activity-events",
            get(learning_activity::list_learning_activity_events),
        )
        .route(
            "/learning-review/daily",
            get(learning_activity::get_daily_learning_review),
        )
        .route(
            "/learning-review/activity-heatmap",
            get(learning_activity::get_activity_heatmap),
        )
        .route(
            "/agent-tasks/{id}",
            get(learning::get_agent_task).put(learning::upsert_agent_task),
        )
        .route("/artifacts/{id}", put(learning::upsert_artifact))
        .route("/artifacts/{article_id}/{id}", get(learning::get_artifact))
        .route(
            "/legacy-imports",
            post(legacy_imports::create_legacy_import),
        )
        .route(
            "/legacy-imports/{id}",
            get(legacy_imports::get_legacy_import),
        )
        .layer(DefaultBodyLimit::max(200 * 1024 * 1024))
        .with_state(state)
}

pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    let started = Instant::now();
    sqlx::query_scalar::<_, i64>("SELECT 1::BIGINT")
        .fetch_one(&state.pool)
        .await?;

    Ok(Json(HealthResponse {
        status: "ok",
        service: "openkoto-backend",
        version: env!("CARGO_PKG_VERSION"),
        database: DatabaseHealth {
            connected: true,
            latency_ms: started.elapsed().as_millis(),
        },
        file_storage: FileStorageHealth {
            ready: state.config.file_storage_dir.exists(),
        },
        auth: AuthHealth {
            configured: state.config.auth_is_configured(),
        },
        started_at: state.started_at,
    }))
}
