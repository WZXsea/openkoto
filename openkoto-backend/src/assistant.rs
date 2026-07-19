use std::collections::HashSet;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    learning::{AgentTaskDto, ArtifactDto},
    routes::AppState,
};

const TERMINAL_STATUSES: [&str; 3] = ["succeeded", "failed", "cancelled"];

#[derive(Debug, Clone, sqlx::FromRow)]
struct TaskRecord {
    id: String,
    task_type: String,
    status: String,
    article_id: String,
    input: Value,
    progress: f64,
    stage: Option<String>,
    message: Option<String>,
    error: Option<String>,
    worker_session_id: Option<String>,
    artifact_ids: Value,
    created_at: String,
    updated_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    root_task_id: String,
    retry_of_task_id: Option<String>,
    attempt: i32,
    input_snapshot: Value,
    output_version: i32,
    legacy_status: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct TimelineRecord {
    id: Uuid,
    task_id: String,
    external_event_id: Option<String>,
    event_type: String,
    from_status: Option<String>,
    to_status: Option<String>,
    stage: Option<String>,
    message: Option<String>,
    error: Option<String>,
    metadata: Value,
    payload_hash: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct ArtifactRecord {
    id: String,
    task_id: String,
    article_id: String,
    artifact_type: String,
    version: String,
    content: Value,
    metadata: Option<Value>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimelineEventDto {
    pub id: String,
    pub task_id: String,
    pub event_type: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub stage: Option<String>,
    pub message: Option<String>,
    pub error: Option<String>,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct IngestTimelineEventRequest {
    pub event_id: String,
    pub event_type: String,
    #[serde(default)]
    pub from_status: Option<String>,
    #[serde(default)]
    pub to_status: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default = "empty_object")]
    pub metadata: Value,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListTasksQuery {
    pub status: Option<String>,
    pub article_id: Option<String>,
    pub task_type: Option<String>,
    pub root_task_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct TaskPageDto {
    pub items: Vec<AgentTaskDto>,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListArtifactsQuery {
    pub task_id: Option<String>,
    pub article_id: Option<String>,
    pub artifact_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ActionAuditRequest {
    pub action_kind: String,
    pub status: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default = "empty_object")]
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ActionAuditDto {
    pub id: Uuid,
    pub task_id: String,
    pub action_kind: String,
    pub status: String,
    pub code: Option<String>,
    pub message: Option<String>,
    pub payload: Value,
    pub registry_scope: String,
    pub external_write: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionRegistryEntry {
    pub action_kind: &'static str,
    pub scope: &'static str,
    pub allowed: bool,
    pub requires_material_id: bool,
    pub reason: &'static str,
}

pub async fn list_agent_tasks(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(query): Query<ListTasksQuery>,
) -> Result<Json<TaskPageDto>, AppError> {
    if let Some(status) = query.status.as_deref() {
        validate_canonical_status(status)?;
    }
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    let offset = query.offset.unwrap_or(0).max(0);
    let total = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)::BIGINT FROM agent_tasks
        WHERE user_id = $1
          AND ($2::TEXT IS NULL OR status = $2)
          AND ($3::TEXT IS NULL OR article_id = $3)
          AND ($4::TEXT IS NULL OR task_type = $4)
          AND ($5::TEXT IS NULL OR root_task_id = $5)
        "#,
    )
    .bind(user.id)
    .bind(&query.status)
    .bind(&query.article_id)
    .bind(&query.task_type)
    .bind(&query.root_task_id)
    .fetch_one(&state.pool)
    .await?;

    let mut builder = QueryBuilder::<Postgres>::new(format!("{} WHERE user_id = ", task_select()));
    builder.push_bind(user.id);
    if let Some(status) = query.status {
        builder.push(" AND status = ").push_bind(status);
    }
    if let Some(article_id) = query.article_id {
        builder.push(" AND article_id = ").push_bind(article_id);
    }
    if let Some(task_type) = query.task_type {
        builder.push(" AND task_type = ").push_bind(task_type);
    }
    if let Some(root_task_id) = query.root_task_id {
        builder.push(" AND root_task_id = ").push_bind(root_task_id);
    }
    builder
        .push(" ORDER BY updated_at DESC, id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    let items = builder
        .build_query_as::<TaskRecord>()
        .fetch_all(&state.pool)
        .await?
        .into_iter()
        .map(task_to_dto)
        .collect();
    Ok(Json(TaskPageDto {
        items,
        total,
        limit,
        offset,
    }))
}

pub async fn get_agent_task(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<AgentTaskDto>, AppError> {
    Ok(Json(get_task(&state.pool, user.id, &id).await?))
}

pub async fn put_agent_task(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<AgentTaskDto>,
) -> Result<Json<AgentTaskDto>, AppError> {
    if id != payload.id {
        return Err(AppError::bad_request(
            "agent_task_id_mismatch",
            "agent task id does not match path",
        ));
    }
    Ok(Json(
        upsert_worker_task(&state.pool, user.id, payload).await?,
    ))
}

pub async fn get_task_timeline(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<TimelineEventDto>>, AppError> {
    ensure_task(&state.pool, user.id, &id).await?;
    let records = sqlx::query_as::<_, TimelineRecord>(
        r#"
        SELECT id, task_id, external_event_id, event_type, from_status, to_status, stage, message,
               error, metadata, payload_hash, created_at
        FROM agent_task_events
        WHERE user_id = $1 AND task_id = $2
        ORDER BY created_at ASC, sequence ASC
        "#,
    )
    .bind(user.id)
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(records.into_iter().map(timeline_to_dto).collect()))
}

pub async fn ingest_task_timeline(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(mut payload): ApiJson<IngestTimelineEventRequest>,
) -> Result<Json<TimelineEventDto>, AppError> {
    validate_nonempty(
        &payload.event_id,
        "invalid_timeline_event",
        "event_id is required",
    )?;
    validate_nonempty(
        &payload.event_type,
        "invalid_timeline_event",
        "event_type is required",
    )?;
    if payload.event_id.len() > 255 {
        return Err(AppError::bad_request(
            "invalid_timeline_event",
            "event_id must not exceed 255 characters",
        ));
    }
    if !payload.metadata.is_object() {
        return Err(AppError::bad_request(
            "invalid_timeline_event",
            "metadata must be an object",
        ));
    }
    if let Some(level) = payload.level.take() {
        payload
            .metadata
            .as_object_mut()
            .expect("metadata was validated")
            .insert("level".into(), Value::String(level));
    }
    let from_status = payload
        .from_status
        .as_deref()
        .map(normalize_status)
        .transpose()?
        .map(|value| value.0);
    let requested_to = payload.to_status.as_deref().or(payload.status.as_deref());
    let to_status = requested_to
        .map(normalize_status)
        .transpose()?
        .map(|value| value.0);
    if requested_to == Some("interrupted") || payload.from_status.as_deref() == Some("interrupted")
    {
        payload
            .metadata
            .as_object_mut()
            .expect("metadata was validated")
            .insert("legacy_status".into(), Value::String("interrupted".into()));
    }
    let created_at = payload
        .created_at
        .as_deref()
        .map(parse_event_time)
        .transpose()?;
    let mut tx = state.pool.begin().await?;
    ensure_task_tx(&mut tx, user.id, &id).await?;
    let event = record_event_tx(
        &mut tx,
        user.id,
        &id,
        Some(payload.event_id.clone()),
        &payload.event_type,
        from_status.as_deref(),
        to_status.as_deref(),
        payload.stage.as_deref(),
        payload.message.as_deref(),
        payload.error.as_deref(),
        payload.metadata,
        Some(format!("timeline:{id}:{}", payload.event_id)),
        created_at,
    )
    .await?;
    tx.commit().await?;
    Ok(Json(timeline_to_dto(event)))
}

pub async fn cancel_agent_task(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<AgentTaskDto>, AppError> {
    let mut tx = state.pool.begin().await?;
    let existing = fetch_task_for_update(&mut tx, user.id, &id).await?;
    if !matches!(existing.status.as_str(), "queued" | "running") {
        return Err(AppError::conflict(
            "agent_task_not_cancellable",
            "only queued or running tasks can be cancelled",
        ));
    }
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE agent_tasks
        SET status = 'cancelled', stage = 'cancelled', message = 'Cancelled by user',
            updated_at = $3, finished_at = $3
        WHERE user_id = $1 AND id = $2
        "#,
    )
    .bind(user.id)
    .bind(&id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    record_event_tx(
        &mut tx,
        user.id,
        &id,
        None,
        "cancelled",
        Some(&existing.status),
        Some("cancelled"),
        Some("cancelled"),
        Some("Cancelled by user"),
        None,
        json!({"source": "user"}),
        None,
        None,
    )
    .await?;
    let task = fetch_task_tx(&mut tx, user.id, &id).await?;
    tx.commit().await?;
    Ok(Json(task_to_dto(task)))
}

pub async fn retry_agent_task(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<AgentTaskDto>, AppError> {
    let mut tx = state.pool.begin().await?;
    let existing = fetch_task_for_update(&mut tx, user.id, &id).await?;
    if !matches!(existing.status.as_str(), "failed" | "cancelled") {
        return Err(AppError::conflict(
            "agent_task_not_retryable",
            "only failed, cancelled, or migrated interrupted tasks can be retried",
        ));
    }
    sqlx::query_scalar::<_, String>(
        "SELECT id FROM agent_tasks WHERE user_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(user.id)
    .bind(&existing.root_task_id)
    .fetch_one(&mut *tx)
    .await?;
    let attempt = sqlx::query_scalar::<_, i32>(
        "SELECT COALESCE(MAX(attempt), 0) + 1 FROM agent_tasks \
         WHERE user_id = $1 AND root_task_id = $2",
    )
    .bind(user.id)
    .bind(&existing.root_task_id)
    .fetch_one(&mut *tx)
    .await?;
    let new_id = format!("task-{}", Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO agent_tasks (
            user_id, id, task_type, status, article_id, input, progress, stage, message, error,
            worker_session_id, artifact_ids, created_at, updated_at, started_at, finished_at,
            input_snapshot, retry_of_task_id, root_task_id, attempt, output_version, legacy_status
        )
        VALUES ($1, $2, $3, 'queued', $4, $5, 0, 'queued', 'Retry queued', NULL,
                NULL, '[]'::jsonb, $6, $6, NULL, NULL, $5, $7, $8, $9, 0, NULL)
        "#,
    )
    .bind(user.id)
    .bind(&new_id)
    .bind(&existing.task_type)
    .bind(&existing.article_id)
    .bind(&existing.input_snapshot)
    .bind(&now)
    .bind(&existing.id)
    .bind(&existing.root_task_id)
    .bind(attempt)
    .execute(&mut *tx)
    .await?;
    record_event_tx(
        &mut tx,
        user.id,
        &existing.id,
        None,
        "retried",
        Some(&existing.status),
        Some(&existing.status),
        existing.stage.as_deref(),
        Some("Retry task created"),
        None,
        json!({"retry_task_id": new_id, "attempt": attempt}),
        None,
        None,
    )
    .await?;
    record_event_tx(
        &mut tx,
        user.id,
        &new_id,
        None,
        "created",
        None,
        Some("queued"),
        Some("queued"),
        Some("Retry queued"),
        None,
        json!({
            "retry_of_task_id": existing.id,
            "root_task_id": existing.root_task_id,
            "attempt": attempt
        }),
        None,
        None,
    )
    .await?;
    let task = fetch_task_tx(&mut tx, user.id, &new_id).await?;
    tx.commit().await?;
    Ok(Json(task_to_dto(task)))
}

pub async fn list_task_artifacts(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<ArtifactDto>>, AppError> {
    ensure_task(&state.pool, user.id, &id).await?;
    Ok(Json(
        fetch_artifacts(&state.pool, user.id, Some(&id), None, None)
            .await?
            .into_iter()
            .map(artifact_to_dto)
            .collect(),
    ))
}

pub async fn list_artifacts(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(query): Query<ListArtifactsQuery>,
) -> Result<Json<Vec<ArtifactDto>>, AppError> {
    Ok(Json(
        fetch_artifacts(
            &state.pool,
            user.id,
            query.task_id.as_deref(),
            query.article_id.as_deref(),
            query.artifact_type.as_deref(),
        )
        .await?
        .into_iter()
        .map(artifact_to_dto)
        .collect(),
    ))
}

pub async fn get_artifact_by_id(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<ArtifactDto>, AppError> {
    let record = sqlx::query_as::<_, ArtifactRecord>(&format!(
        "{} WHERE user_id = $1 AND id = $2",
        artifact_select()
    ))
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(artifact_not_found)?;
    Ok(Json(artifact_to_dto(record)))
}

pub async fn put_artifact(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<ArtifactDto>,
) -> Result<Json<ArtifactDto>, AppError> {
    if id != payload.id {
        return Err(AppError::bad_request(
            "artifact_id_mismatch",
            "artifact id does not match path",
        ));
    }
    Ok(Json(
        upsert_worker_artifact(&state.pool, user.id, payload).await?,
    ))
}

pub async fn list_task_actions(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<ActionAuditDto>>, AppError> {
    ensure_task(&state.pool, user.id, &id).await?;
    let records = sqlx::query_as::<_, ActionAuditDto>(
        r#"
        SELECT id, task_id, action_kind, status, code, message, payload, registry_scope,
               external_write, created_at
        FROM assistant_action_audits
        WHERE user_id = $1 AND task_id = $2
        ORDER BY created_at ASC, id ASC
        "#,
    )
    .bind(user.id)
    .bind(id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(records))
}

pub async fn audit_task_action(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<String>,
    ApiJson(payload): ApiJson<ActionAuditRequest>,
) -> Result<Json<TimelineEventDto>, AppError> {
    validate_nonempty(
        &payload.action_kind,
        "invalid_assistant_action",
        "action_kind is required",
    )?;
    if !matches!(payload.status.as_str(), "executed" | "rejected") {
        return Err(AppError::bad_request(
            "invalid_assistant_action",
            "action status must be executed or rejected",
        ));
    }
    if !payload.payload.is_object() {
        return Err(AppError::bad_request(
            "invalid_assistant_action",
            "action payload must be an object",
        ));
    }
    let policy = action_policy(&payload.action_kind);
    let mut status = payload.status.clone();
    let mut code = payload.code.clone();
    let mut message = payload.message.clone();
    if !policy.allowed {
        status = "rejected".into();
        code = Some(if policy.external_write {
            "external_write_forbidden".into()
        } else {
            "unregistered_action".into()
        });
        message = Some(policy.reason.into());
    } else if policy.requires_material_id {
        let material_id = payload
            .payload
            .get("material_id")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok());
        let exists = if let Some(material_id) = material_id {
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM materials WHERE user_id = $1 AND id = $2)",
            )
            .bind(user.id)
            .bind(material_id)
            .fetch_one(&state.pool)
            .await?
        } else {
            false
        };
        if !exists {
            status = "rejected".into();
            code = Some("material_not_found".into());
            message = Some("action requires a material owned by the current user".into());
        }
    }

    let mut tx = state.pool.begin().await?;
    ensure_task_tx(&mut tx, user.id, &id).await?;
    let event_type = if status == "executed" {
        "action_executed"
    } else {
        "action_rejected"
    };
    let event = record_event_tx(
        &mut tx,
        user.id,
        &id,
        None,
        event_type,
        None,
        None,
        None,
        message.as_deref(),
        if status == "rejected" {
            message.as_deref()
        } else {
            None
        },
        json!({
            "action_kind": payload.action_kind,
            "status": status,
            "code": code,
            "payload": payload.payload,
            "registry_scope": policy.scope,
            "external_write": policy.external_write,
        }),
        None,
        None,
    )
    .await?;
    sqlx::query(
        r#"
        INSERT INTO assistant_action_audits (
            id, user_id, task_id, event_id, action_kind, status, code, message, payload,
            registry_scope, external_write
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user.id)
    .bind(&id)
    .bind(event.id)
    .bind(&payload.action_kind)
    .bind(&status)
    .bind(&code)
    .bind(&message)
    .bind(&payload.payload)
    .bind(policy.scope)
    .bind(policy.external_write)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(timeline_to_dto(event)))
}

pub async fn get_action_registry(_auth: AuthenticatedUser) -> Json<Vec<ActionRegistryEntry>> {
    Json(vec![
        registry_entry("get_current_material", false),
        registry_entry("list_materials", false),
        registry_entry("open_material", true),
        registry_entry("open_source", true),
    ])
}

pub(crate) async fn upsert_worker_task(
    pool: &PgPool,
    user_id: Uuid,
    payload: AgentTaskDto,
) -> Result<AgentTaskDto, AppError> {
    validate_worker_task(&payload)?;
    let (status, legacy_status) = normalize_status(&payload.status)?;
    let mut tx = pool.begin().await?;
    let existing = fetch_task_optional_for_update(&mut tx, user_id, &payload.id).await?;
    let task = if let Some(existing) = existing {
        if worker_payload_same(&existing, &payload, &status) {
            tx.commit().await?;
            return Ok(task_to_dto(existing));
        }
        let stale = DateTime::parse_from_rfc3339(&existing.updated_at)
            .map(|existing_time| {
                DateTime::parse_from_rfc3339(&payload.updated_at)
                    .expect("worker timestamp was validated")
                    < existing_time
            })
            .unwrap_or(false);
        if stale {
            record_event_tx(
                &mut tx,
                user_id,
                &payload.id,
                None,
                "stale_update_ignored",
                Some(&existing.status),
                Some(&existing.status),
                payload.stage.as_deref(),
                Some("Out-of-order worker update ignored"),
                None,
                json!({"source_updated_at": payload.updated_at, "requested_status": status}),
                Some(format!(
                    "worker-stale:{}:{}",
                    payload.id, payload.updated_at
                )),
                None,
            )
            .await?;
            existing
        } else {
            if is_terminal(&existing.status) {
                if status != existing.status || terminal_payload_changed(&existing, &payload) {
                    return Err(AppError::conflict(
                        "terminal_agent_task_immutable",
                        "terminal agent tasks cannot be rewritten; create a retry instead",
                    ));
                }
                tx.commit().await?;
                return Ok(task_to_dto(existing));
            }
            validate_task_transition(&existing.status, &status)?;
            let artifact_ids = union_strings(&existing.artifact_ids, &payload.artifact_ids);
            sqlx::query(
                r#"
                UPDATE agent_tasks
                SET task_type = $3, status = $4, article_id = $5, input = $6, progress = $7,
                    stage = $8, message = $9, error = $10, worker_session_id = $11,
                    artifact_ids = $12, updated_at = $13,
                    started_at = COALESCE(started_at, $14), finished_at = $15,
                    legacy_status = COALESCE(legacy_status, $16)
                WHERE user_id = $1 AND id = $2
                "#,
            )
            .bind(user_id)
            .bind(&payload.id)
            .bind(&payload.task_type)
            .bind(&status)
            .bind(&payload.article_id)
            .bind(&payload.input)
            .bind(payload.progress)
            .bind(&payload.stage)
            .bind(&payload.message)
            .bind(&payload.error)
            .bind(&payload.worker_session_id)
            .bind(json!(artifact_ids))
            .bind(&payload.updated_at)
            .bind(&payload.started_at)
            .bind(&payload.finished_at)
            .bind(&legacy_status)
            .execute(&mut *tx)
            .await?;
            let event_type = if existing.status == status {
                "progress_updated"
            } else {
                "status_changed"
            };
            record_event_tx(
                &mut tx,
                user_id,
                &payload.id,
                None,
                event_type,
                Some(&existing.status),
                Some(&status),
                payload.stage.as_deref(),
                payload.message.as_deref(),
                payload.error.as_deref(),
                json!({
                    "progress": payload.progress,
                    "source_updated_at": payload.updated_at,
                    "legacy_status": legacy_status,
                }),
                Some(worker_task_event_idempotency_key(&payload)),
                None,
            )
            .await?;
            fetch_task_tx(&mut tx, user_id, &payload.id).await?
        }
    } else {
        sqlx::query(
            r#"
            INSERT INTO agent_tasks (
                user_id, id, task_type, status, article_id, input, progress, stage, message,
                error, worker_session_id, artifact_ids, created_at, updated_at, started_at,
                finished_at, input_snapshot, retry_of_task_id, root_task_id, attempt,
                output_version, legacy_status
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                    $15, $16, $6, NULL, $2, 1, 0, $17)
            "#,
        )
        .bind(user_id)
        .bind(&payload.id)
        .bind(&payload.task_type)
        .bind(&status)
        .bind(&payload.article_id)
        .bind(&payload.input)
        .bind(payload.progress)
        .bind(&payload.stage)
        .bind(&payload.message)
        .bind(&payload.error)
        .bind(&payload.worker_session_id)
        .bind(json!(payload.artifact_ids))
        .bind(&payload.created_at)
        .bind(&payload.updated_at)
        .bind(&payload.started_at)
        .bind(&payload.finished_at)
        .bind(&legacy_status)
        .execute(&mut *tx)
        .await?;
        record_event_tx(
            &mut tx,
            user_id,
            &payload.id,
            None,
            if legacy_status.is_some() {
                "legacy_interrupted_migrated"
            } else {
                "created"
            },
            None,
            Some(&status),
            payload.stage.as_deref(),
            payload.message.as_deref(),
            payload.error.as_deref(),
            json!({
                "progress": payload.progress,
                "source_updated_at": payload.updated_at,
                "initial_terminal_compatibility": is_terminal(&status),
                "legacy_status": legacy_status,
            }),
            Some(worker_task_event_idempotency_key(&payload)),
            None,
        )
        .await?;
        fetch_task_tx(&mut tx, user_id, &payload.id).await?
    };
    tx.commit().await?;
    Ok(task_to_dto(task))
}

pub(crate) async fn upsert_worker_artifact(
    pool: &PgPool,
    user_id: Uuid,
    payload: ArtifactDto,
) -> Result<ArtifactDto, AppError> {
    validate_artifact(&payload)?;
    let mut tx = pool.begin().await?;
    let task = fetch_task_for_update(&mut tx, user_id, &payload.task_id).await?;
    if matches!(task.status.as_str(), "failed" | "cancelled") {
        return Err(AppError::conflict(
            "artifact_task_terminal",
            "failed or cancelled tasks cannot accept artifacts",
        ));
    }
    if task.article_id != payload.article_id {
        return Err(AppError::conflict(
            "artifact_article_mismatch",
            "artifact article_id must match its task",
        ));
    }
    let existing = sqlx::query_as::<_, ArtifactRecord>(&format!(
        "{} WHERE user_id = $1 AND id = $2 FOR UPDATE",
        artifact_select()
    ))
    .bind(user_id)
    .bind(&payload.id)
    .fetch_optional(&mut *tx)
    .await?;
    if existing
        .as_ref()
        .is_some_and(|existing| existing.task_id != payload.task_id)
    {
        return Err(AppError::conflict(
            "artifact_task_mismatch",
            "artifact id is already owned by another task",
        ));
    }
    let changed = existing.as_ref().is_none_or(|existing| {
        existing.version != payload.version
            || existing.content != payload.content
            || existing.metadata != payload.metadata
    });
    let record = sqlx::query_as::<_, ArtifactRecord>(
        r#"
        INSERT INTO artifacts (
            user_id, id, task_id, article_id, artifact_type, version, content, metadata,
            created_at, updated_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (user_id, id) DO UPDATE
        SET article_id = EXCLUDED.article_id, artifact_type = EXCLUDED.artifact_type,
            version = EXCLUDED.version, content = EXCLUDED.content,
            metadata = EXCLUDED.metadata, updated_at = EXCLUDED.updated_at
        RETURNING id, task_id, article_id, artifact_type, version, content, metadata,
                  created_at, updated_at
        "#,
    )
    .bind(user_id)
    .bind(&payload.id)
    .bind(&payload.task_id)
    .bind(&payload.article_id)
    .bind(&payload.artifact_type)
    .bind(&payload.version)
    .bind(&payload.content)
    .bind(&payload.metadata)
    .bind(&payload.created_at)
    .bind(&payload.updated_at)
    .fetch_one(&mut *tx)
    .await?;
    if changed {
        sqlx::query(
            r#"
            UPDATE agent_tasks
            SET output_version = output_version + 1,
                artifact_ids = (
                    SELECT COALESCE(jsonb_agg(value ORDER BY value), '[]'::jsonb)
                    FROM (
                        SELECT DISTINCT value
                        FROM jsonb_array_elements_text(artifact_ids || jsonb_build_array($3::text)) values(value)
                    ) deduplicated
                )
            WHERE user_id = $1 AND id = $2
            "#,
        )
        .bind(user_id)
        .bind(&payload.task_id)
        .bind(&payload.id)
        .execute(&mut *tx)
        .await?;
        record_event_tx(
            &mut tx,
            user_id,
            &payload.task_id,
            None,
            if existing.is_some() {
                "artifact_updated"
            } else {
                "artifact_created"
            },
            Some(&task.status),
            Some(&task.status),
            task.stage.as_deref(),
            Some("Task artifact persisted"),
            None,
            json!({
                "artifact_id": payload.id,
                "artifact_type": payload.artifact_type,
                "version": payload.version,
            }),
            Some(format!(
                "artifact:{}:{}:{}:{}",
                payload.task_id, payload.id, payload.version, payload.updated_at
            )),
            None,
        )
        .await?;
    }
    tx.commit().await?;
    Ok(artifact_to_dto(record))
}

async fn get_task(pool: &PgPool, user_id: Uuid, id: &str) -> Result<AgentTaskDto, AppError> {
    let record = sqlx::query_as::<_, TaskRecord>(&format!(
        "{} WHERE user_id = $1 AND id = $2",
        task_select()
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(task_not_found)?;
    Ok(task_to_dto(record))
}

async fn ensure_task(pool: &PgPool, user_id: Uuid, id: &str) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM agent_tasks WHERE user_id = $1 AND id = $2)",
    )
    .bind(user_id)
    .bind(id)
    .fetch_one(pool)
    .await?;
    if !exists {
        return Err(task_not_found());
    }
    Ok(())
}

async fn ensure_task_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: &str,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM agent_tasks WHERE user_id = $1 AND id = $2)",
    )
    .bind(user_id)
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    if !exists {
        return Err(task_not_found());
    }
    Ok(())
}

async fn fetch_task_optional_for_update(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: &str,
) -> Result<Option<TaskRecord>, AppError> {
    sqlx::query_as::<_, TaskRecord>(&format!(
        "{} WHERE user_id = $1 AND id = $2 FOR UPDATE",
        task_select()
    ))
    .bind(user_id)
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(AppError::from)
}

async fn fetch_task_for_update(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: &str,
) -> Result<TaskRecord, AppError> {
    fetch_task_optional_for_update(tx, user_id, id)
        .await?
        .ok_or_else(task_not_found)
}

async fn fetch_task_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    id: &str,
) -> Result<TaskRecord, AppError> {
    sqlx::query_as::<_, TaskRecord>(&format!("{} WHERE user_id = $1 AND id = $2", task_select()))
        .bind(user_id)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(task_not_found)
}

#[allow(clippy::too_many_arguments)]
async fn record_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    task_id: &str,
    external_event_id: Option<String>,
    event_type: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    stage: Option<&str>,
    message: Option<&str>,
    error: Option<&str>,
    metadata: Value,
    idempotency_key: Option<String>,
    created_at: Option<DateTime<Utc>>,
) -> Result<TimelineRecord, AppError> {
    if !metadata.is_object() {
        return Err(AppError::bad_request(
            "invalid_timeline_event",
            "timeline metadata must be an object",
        ));
    }
    let payload_hash = sha256_json(&json!({
        "task_id": task_id,
        "external_event_id": external_event_id,
        "event_type": event_type,
        "from_status": from_status,
        "to_status": to_status,
        "stage": stage,
        "message": message,
        "error": error,
        "metadata": metadata,
        "created_at": created_at,
    }));
    let id = Uuid::new_v4();
    let inserted = sqlx::query_as::<_, TimelineRecord>(
        r#"
        INSERT INTO agent_task_events (
            id, external_event_id, user_id, task_id, event_type, from_status, to_status,
            stage, message, error, metadata, idempotency_key, payload_hash, created_at
        ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13,
                  COALESCE($14, NOW()))
        ON CONFLICT (user_id, idempotency_key) WHERE idempotency_key IS NOT NULL DO NOTHING
        RETURNING id, task_id, external_event_id, event_type, from_status, to_status, stage, message,
                  error, metadata, payload_hash, created_at
        "#,
    )
    .bind(id)
    .bind(&external_event_id)
    .bind(user_id)
    .bind(task_id)
    .bind(event_type)
    .bind(from_status)
    .bind(to_status)
    .bind(stage)
    .bind(message)
    .bind(error)
    .bind(&metadata)
    .bind(&idempotency_key)
    .bind(&payload_hash)
    .bind(created_at)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(event) = inserted {
        return Ok(event);
    }
    let key = idempotency_key.ok_or_else(|| {
        AppError::internal(
            "timeline_event_insert_failed",
            "timeline event was not inserted",
        )
    })?;
    let existing = sqlx::query_as::<_, TimelineRecord>(
        r#"
        SELECT id, task_id, external_event_id, event_type, from_status, to_status, stage, message,
               error, metadata, payload_hash, created_at
        FROM agent_task_events
        WHERE user_id = $1 AND idempotency_key = $2
        "#,
    )
    .bind(user_id)
    .bind(key)
    .fetch_one(&mut **tx)
    .await?;
    if existing.payload_hash != payload_hash {
        return Err(AppError::conflict(
            "timeline_event_idempotency_conflict",
            "event_id or idempotency key was reused with a different payload",
        ));
    }
    Ok(existing)
}

async fn fetch_artifacts(
    pool: &PgPool,
    user_id: Uuid,
    task_id: Option<&str>,
    article_id: Option<&str>,
    artifact_type: Option<&str>,
) -> Result<Vec<ArtifactRecord>, AppError> {
    sqlx::query_as::<_, ArtifactRecord>(&format!(
        "{} WHERE user_id = $1 \
         AND ($2::TEXT IS NULL OR task_id = $2) \
         AND ($3::TEXT IS NULL OR article_id = $3) \
         AND ($4::TEXT IS NULL OR artifact_type = $4) \
         ORDER BY updated_at DESC, id DESC",
        artifact_select()
    ))
    .bind(user_id)
    .bind(task_id)
    .bind(article_id)
    .bind(artifact_type)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

fn task_select() -> &'static str {
    "SELECT id, task_type, status, article_id, input, progress, stage, message, error, \
     worker_session_id, artifact_ids, created_at, updated_at, started_at, finished_at, \
     root_task_id, retry_of_task_id, attempt, input_snapshot, output_version, legacy_status \
     FROM agent_tasks"
}

fn artifact_select() -> &'static str {
    "SELECT id, task_id, article_id, artifact_type, version, content, metadata, \
     created_at, updated_at FROM artifacts"
}

fn task_to_dto(record: TaskRecord) -> AgentTaskDto {
    AgentTaskDto {
        id: record.id,
        task_type: record.task_type,
        status: record.status,
        article_id: record.article_id,
        input: record.input,
        progress: record.progress,
        stage: record.stage,
        message: record.message,
        error: record.error,
        worker_session_id: record.worker_session_id,
        artifact_ids: string_vec(record.artifact_ids),
        created_at: record.created_at,
        updated_at: record.updated_at,
        started_at: record.started_at,
        finished_at: record.finished_at,
        root_task_id: Some(record.root_task_id),
        retry_of_task_id: record.retry_of_task_id,
        attempt: record.attempt,
        input_snapshot: Some(record.input_snapshot),
        output_version: record.output_version,
        legacy_status: record.legacy_status,
    }
}

fn artifact_to_dto(record: ArtifactRecord) -> ArtifactDto {
    ArtifactDto {
        id: record.id,
        task_id: record.task_id,
        article_id: record.article_id,
        artifact_type: record.artifact_type,
        version: record.version,
        content: record.content,
        metadata: record.metadata,
        created_at: record.created_at,
        updated_at: record.updated_at,
    }
}

fn timeline_to_dto(record: TimelineRecord) -> TimelineEventDto {
    TimelineEventDto {
        id: record
            .external_event_id
            .unwrap_or_else(|| record.id.to_string()),
        task_id: record.task_id,
        event_type: record.event_type,
        from_status: record.from_status,
        to_status: record.to_status,
        stage: record.stage,
        message: record.message,
        error: record.error,
        metadata: record.metadata,
        created_at: record.created_at.to_rfc3339(),
    }
}

fn validate_worker_task(payload: &AgentTaskDto) -> Result<(), AppError> {
    validate_nonempty(
        &payload.id,
        "invalid_agent_task",
        "agent task id is required",
    )?;
    validate_nonempty(
        &payload.task_type,
        "invalid_agent_task",
        "agent task type is required",
    )?;
    validate_nonempty(
        &payload.article_id,
        "invalid_agent_task",
        "agent task article id is required",
    )?;
    validate_nonempty(
        &payload.created_at,
        "invalid_agent_task",
        "created_at is required",
    )?;
    validate_nonempty(
        &payload.updated_at,
        "invalid_agent_task",
        "updated_at is required",
    )?;
    if !payload.input.is_object() {
        return Err(AppError::bad_request(
            "invalid_agent_task",
            "agent task input must be an object",
        ));
    }
    if !payload.progress.is_finite() || !(0.0..=1.0).contains(&payload.progress) {
        return Err(AppError::bad_request(
            "invalid_agent_task",
            "agent task progress must be between 0 and 1",
        ));
    }
    normalize_status(&payload.status)?;
    validate_task_timestamp(&payload.created_at, "created_at")?;
    validate_task_timestamp(&payload.updated_at, "updated_at")?;
    for (value, field) in [
        (payload.started_at.as_deref(), "started_at"),
        (payload.finished_at.as_deref(), "finished_at"),
    ] {
        if let Some(value) = value {
            validate_task_timestamp(value, field)?;
        }
    }
    Ok(())
}

fn validate_artifact(payload: &ArtifactDto) -> Result<(), AppError> {
    for (value, message) in [
        (&payload.id, "artifact id is required"),
        (&payload.task_id, "artifact task id is required"),
        (&payload.article_id, "artifact article id is required"),
        (&payload.artifact_type, "artifact type is required"),
        (&payload.version, "artifact version is required"),
        (&payload.created_at, "artifact created_at is required"),
        (&payload.updated_at, "artifact updated_at is required"),
    ] {
        validate_nonempty(value, "invalid_artifact", message)?;
    }
    Ok(())
}

fn normalize_status(status: &str) -> Result<(String, Option<String>), AppError> {
    if status == "interrupted" {
        return Ok(("failed".into(), Some("interrupted".into())));
    }
    validate_canonical_status(status)?;
    Ok((status.to_string(), None))
}

fn validate_canonical_status(status: &str) -> Result<(), AppError> {
    if !matches!(
        status,
        "queued" | "running" | "succeeded" | "failed" | "cancelled"
    ) {
        return Err(AppError::bad_request(
            "invalid_agent_task_status",
            "task status must be queued, running, succeeded, failed, or cancelled",
        ));
    }
    Ok(())
}

fn validate_task_transition(from: &str, to: &str) -> Result<(), AppError> {
    let allowed = from == to
        || matches!(
            (from, to),
            ("queued", "running" | "succeeded" | "failed" | "cancelled")
                | ("running", "succeeded" | "failed" | "cancelled")
        );
    if !allowed {
        return Err(AppError::conflict(
            "invalid_agent_task_transition",
            format!("agent task cannot transition from {from} to {to}"),
        ));
    }
    Ok(())
}

fn terminal_payload_changed(existing: &TaskRecord, payload: &AgentTaskDto) -> bool {
    existing.task_type != payload.task_type
        || existing.article_id != payload.article_id
        || existing.input != payload.input
        || (existing.progress - payload.progress).abs() > f64::EPSILON
        || existing.stage != payload.stage
        || existing.message != payload.message
        || existing.error != payload.error
        || existing.worker_session_id != payload.worker_session_id
        || existing.finished_at != payload.finished_at
}

fn worker_payload_same(existing: &TaskRecord, payload: &AgentTaskDto, status: &str) -> bool {
    let existing_artifact_ids = string_vec(existing.artifact_ids.clone());
    existing.status == status
        && existing.updated_at == payload.updated_at
        && !terminal_payload_changed(existing, payload)
        && payload
            .artifact_ids
            .iter()
            .all(|id| existing_artifact_ids.contains(id))
}

fn worker_task_event_idempotency_key(payload: &AgentTaskDto) -> String {
    let fingerprint = serde_json::to_value(payload)
        .map(|value| sha256_json(&value))
        .unwrap_or_else(|_| {
            sha256_json(&json!({
                "id": payload.id,
                "status": payload.status,
                "updated_at": payload.updated_at,
            }))
        });
    format!("worker:{}:{fingerprint}", payload.id)
}

fn is_terminal(status: &str) -> bool {
    TERMINAL_STATUSES.contains(&status)
}

fn union_strings(existing: &Value, incoming: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    existing
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .chain(incoming.iter().map(String::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| seen.insert((*value).to_string()))
        .map(ToString::to_string)
        .collect()
}

fn string_vec(value: Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(ToString::to_string)
        .collect()
}

fn parse_event_time(value: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| {
            AppError::bad_request(
                "invalid_timeline_event",
                "created_at must be an RFC3339 timestamp",
            )
        })
}

fn validate_task_timestamp(value: &str, field: &str) -> Result<(), AppError> {
    DateTime::parse_from_rfc3339(value).map_err(|_| {
        AppError::bad_request(
            "invalid_agent_task_timestamp",
            format!("{field} must be an RFC3339 timestamp"),
        )
    })?;
    Ok(())
}

fn sha256_json(value: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

fn validate_nonempty(
    value: &str,
    code: &'static str,
    message: &'static str,
) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::bad_request(code, message));
    }
    Ok(())
}

fn empty_object() -> Value {
    json!({})
}

fn task_not_found() -> AppError {
    AppError::not_found("agent_task_not_found", "agent task not found")
}

fn artifact_not_found() -> AppError {
    AppError::not_found("artifact_not_found", "artifact not found")
}

struct ActionPolicy {
    allowed: bool,
    scope: &'static str,
    external_write: bool,
    requires_material_id: bool,
    reason: &'static str,
}

fn action_policy(action_kind: &str) -> ActionPolicy {
    match action_kind {
        "get_current_material" | "list_materials" => ActionPolicy {
            allowed: true,
            scope: "local_read",
            external_write: false,
            requires_material_id: false,
            reason: "Allowed local read action.",
        },
        "open_material" | "open_source" => ActionPolicy {
            allowed: true,
            scope: "local_read",
            external_write: false,
            requires_material_id: true,
            reason: "Allowed local read action.",
        },
        other if looks_like_external_write(other) => ActionPolicy {
            allowed: false,
            scope: "external_write",
            external_write: true,
            requires_material_id: false,
            reason: "External software writes are disabled during phase one.",
        },
        _ => ActionPolicy {
            allowed: false,
            scope: "unregistered",
            external_write: false,
            requires_material_id: false,
            reason: "Action is not registered for phase one.",
        },
    }
}

fn looks_like_external_write(action_kind: &str) -> bool {
    let normalized = action_kind.to_ascii_lowercase();
    [
        "anki", "zotero", "mineru", "import", "write", "delete", "sync", "mcp",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

fn registry_entry(action_kind: &'static str, requires_material_id: bool) -> ActionRegistryEntry {
    ActionRegistryEntry {
        action_kind,
        scope: "local_read",
        allowed: true,
        requires_material_id,
        reason: "Allowed local read action.",
    }
}
