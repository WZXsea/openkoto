use std::{sync::Arc, time::Instant};

use axum::{
    extract::{DefaultBodyLimit, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;

use crate::{auth, config::AppConfig, error::AppError, files, learning, legacy_imports, materials};

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
            "/materials/{id}",
            get(materials::get_material)
                .patch(materials::patch_material)
                .delete(materials::delete_material),
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
