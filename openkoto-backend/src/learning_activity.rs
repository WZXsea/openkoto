use std::collections::BTreeMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Transaction};
use uuid::Uuid;

use crate::{
    auth::{ApiJson, AuthenticatedUser},
    error::AppError,
    routes::AppState,
};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LearningActivityEventDto {
    pub id: Uuid,
    pub learning_item_id: Option<Uuid>,
    pub material_id: Option<Uuid>,
    pub event_type: String,
    pub metadata: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct LearningActivityEventWithHash {
    id: Uuid,
    learning_item_id: Option<Uuid>,
    material_id: Option<Uuid>,
    event_type: String,
    metadata: Value,
    occurred_at: DateTime<Utc>,
    payload_sha256: String,
}

#[derive(Debug, Default, Deserialize)]
pub struct ListLearningActivityEventsQuery {
    pub event_type: Option<String>,
    pub learning_item_id: Option<Uuid>,
    pub material_id: Option<Uuid>,
    pub date: Option<String>,
    pub timezone_offset_minutes: Option<i32>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct RecordLocalPreviewRequest {
    #[serde(default = "empty_metadata")]
    pub metadata: Value,
    #[serde(default)]
    pub idempotency_key: Option<String>,
}

fn empty_metadata() -> Value {
    json!({})
}

#[derive(Debug, Serialize)]
pub struct LearningReviewDto {
    pub date: Option<String>,
    pub material_id: Option<Uuid>,
    pub total_events: i64,
    pub event_counts: BTreeMap<String, i64>,
    pub unique_learning_items: i64,
    pub events: Vec<LearningActivityEventDto>,
}

#[derive(Debug, Deserialize)]
pub struct DailyReviewQuery {
    pub date: Option<String>,
    pub timezone_offset_minutes: Option<i32>,
}

pub async fn list_learning_activity_events(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(query): Query<ListLearningActivityEventsQuery>,
) -> Result<Json<Vec<LearningActivityEventDto>>, AppError> {
    if let Some(event_type) = query.event_type.as_deref() {
        validate_event_type(event_type)?;
    }
    let timezone_offset_minutes = validate_timezone_offset(query.timezone_offset_minutes)?;
    let date = query.date.as_deref().map(parse_date).transpose()?;
    let bounds = date.map(|date| date_bounds(date, timezone_offset_minutes));
    let limit = query.limit.unwrap_or(200).clamp(1, 500);
    let offset = query.offset.unwrap_or(0).max(0);

    let events = fetch_events(
        &state.pool,
        user.id,
        query.event_type.as_deref(),
        query.learning_item_id,
        query.material_id,
        bounds,
        limit,
        offset,
    )
    .await?;
    Ok(Json(events))
}

pub async fn record_local_preview(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(id): Path<Uuid>,
    ApiJson(payload): ApiJson<RecordLocalPreviewRequest>,
) -> Result<Json<LearningActivityEventDto>, AppError> {
    validate_metadata(&payload.metadata)?;
    let idempotency_key = normalize_idempotency_key(payload.idempotency_key)?;
    let item = sqlx::query_as::<_, (Option<Uuid>, String)>(
        "SELECT material_id, status FROM learning_items WHERE user_id = $1 AND id = $2",
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or_else(|| AppError::not_found("learning_item_not_found", "learning item not found"))?;
    if item.1 != "accepted" {
        return Err(AppError::conflict(
            "learning_item_preview_requires_accepted",
            "local preview can only be recorded for accepted learning items",
        ));
    }

    let event = record_event(
        &state.pool,
        user.id,
        Some(id),
        item.0,
        "local_preview",
        payload.metadata,
        idempotency_key,
    )
    .await?;
    Ok(Json(event))
}

pub async fn get_daily_learning_review(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Query(query): Query<DailyReviewQuery>,
) -> Result<Json<LearningReviewDto>, AppError> {
    let timezone_offset_minutes = validate_timezone_offset(query.timezone_offset_minutes)?;
    let date = query
        .date
        .as_deref()
        .map(parse_date)
        .transpose()?
        .unwrap_or_else(|| {
            (Utc::now() + Duration::minutes(i64::from(timezone_offset_minutes))).date_naive()
        });
    let bounds = date_bounds(date, timezone_offset_minutes);
    let events = fetch_events(&state.pool, user.id, None, None, None, Some(bounds), 500, 0).await?;
    let (total_events, unique_learning_items, event_counts) =
        fetch_summary(&state.pool, user.id, None, Some(bounds)).await?;
    Ok(Json(LearningReviewDto {
        date: Some(date.to_string()),
        material_id: None,
        total_events,
        event_counts,
        unique_learning_items,
        events,
    }))
}

pub async fn get_material_learning_review(
    State(state): State<AppState>,
    AuthenticatedUser { user }: AuthenticatedUser,
    Path(material_id): Path<Uuid>,
) -> Result<Json<LearningReviewDto>, AppError> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM materials WHERE user_id = $1 AND id = $2)",
    )
    .bind(user.id)
    .bind(material_id)
    .fetch_one(&state.pool)
    .await?;
    if !exists {
        return Err(AppError::not_found(
            "material_not_found",
            "material not found",
        ));
    }
    let events = fetch_events(
        &state.pool,
        user.id,
        None,
        None,
        Some(material_id),
        None,
        500,
        0,
    )
    .await?;
    let (total_events, unique_learning_items, event_counts) =
        fetch_summary(&state.pool, user.id, Some(material_id), None).await?;
    Ok(Json(LearningReviewDto {
        date: None,
        material_id: Some(material_id),
        total_events,
        event_counts,
        unique_learning_items,
        events,
    }))
}

pub(crate) async fn record_event(
    pool: &PgPool,
    user_id: Uuid,
    learning_item_id: Option<Uuid>,
    material_id: Option<Uuid>,
    event_type: &str,
    metadata: Value,
    idempotency_key: Option<String>,
) -> Result<LearningActivityEventDto, AppError> {
    let mut tx = pool.begin().await?;
    let event = record_event_tx(
        &mut tx,
        user_id,
        learning_item_id,
        material_id,
        event_type,
        metadata,
        idempotency_key,
    )
    .await?;
    tx.commit().await?;
    Ok(event)
}

pub(crate) async fn record_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    learning_item_id: Option<Uuid>,
    material_id: Option<Uuid>,
    event_type: &str,
    metadata: Value,
    idempotency_key: Option<String>,
) -> Result<LearningActivityEventDto, AppError> {
    validate_event_type(event_type)?;
    validate_metadata(&metadata)?;
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let payload_sha256 = event_payload_sha256(learning_item_id, material_id, event_type, &metadata);
    let event_id = Uuid::new_v4();
    let inserted = sqlx::query_as::<_, LearningActivityEventDto>(
        r#"
        INSERT INTO learning_activity_events (
            id, user_id, learning_item_id, material_id, event_type, metadata, payload_sha256,
            idempotency_key
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (user_id, idempotency_key)
            WHERE idempotency_key IS NOT NULL
            DO NOTHING
        RETURNING id, learning_item_id, material_id, event_type, metadata, occurred_at
        "#,
    )
    .bind(event_id)
    .bind(user_id)
    .bind(learning_item_id)
    .bind(material_id)
    .bind(event_type)
    .bind(&metadata)
    .bind(&payload_sha256)
    .bind(&idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;

    if let Some(event) = inserted {
        return Ok(event);
    }
    let key = idempotency_key.ok_or_else(|| {
        AppError::internal(
            "activity_event_insert_failed",
            "activity event was not inserted",
        )
    })?;
    let stored = sqlx::query_as::<_, LearningActivityEventWithHash>(
        r#"
        SELECT id, learning_item_id, material_id, event_type, metadata, occurred_at,
               payload_sha256
        FROM learning_activity_events
        WHERE user_id = $1 AND idempotency_key = $2
        "#,
    )
    .bind(user_id)
    .bind(key)
    .fetch_one(&mut **tx)
    .await?;
    if stored.payload_sha256 != payload_sha256 {
        return Err(AppError::conflict(
            "learning_activity_idempotency_conflict",
            "idempotency_key was already used with a different activity payload",
        ));
    }
    Ok(LearningActivityEventDto {
        id: stored.id,
        learning_item_id: stored.learning_item_id,
        material_id: stored.material_id,
        event_type: stored.event_type,
        metadata: stored.metadata,
        occurred_at: stored.occurred_at,
    })
}

pub(crate) async fn record_read_event_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    material_id: Uuid,
    reader_kind: &str,
    locator: &Value,
    progress_ratio: f64,
    status: &str,
) -> Result<(), AppError> {
    let local_date = (Utc::now() + Duration::hours(8)).date_naive();
    let progress_bucket = ((progress_ratio.clamp(0.0, 1.0) * 20.0).floor() as i32).min(20);
    let key = format!("read:{material_id}:{local_date}:{reader_kind}:{status}:{progress_bucket}");
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM learning_activity_events \
         WHERE user_id = $1 AND idempotency_key = $2)",
    )
    .bind(user_id)
    .bind(&key)
    .fetch_one(&mut **tx)
    .await?;
    if exists {
        return Ok(());
    }
    record_event_tx(
        tx,
        user_id,
        None,
        Some(material_id),
        "read",
        reading_event_metadata(reader_kind, locator, progress_ratio, status),
        Some(key),
    )
    .await?;
    Ok(())
}

async fn fetch_events(
    pool: &PgPool,
    user_id: Uuid,
    event_type: Option<&str>,
    learning_item_id: Option<Uuid>,
    material_id: Option<Uuid>,
    bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
    limit: i64,
    offset: i64,
) -> Result<Vec<LearningActivityEventDto>, AppError> {
    let mut query = QueryBuilder::<Postgres>::new(
        "SELECT id, learning_item_id, material_id, event_type, metadata, occurred_at \
         FROM learning_activity_events WHERE user_id = ",
    );
    query.push_bind(user_id);
    if let Some(event_type) = event_type {
        query.push(" AND event_type = ").push_bind(event_type);
    }
    if let Some(learning_item_id) = learning_item_id {
        query
            .push(" AND learning_item_id = ")
            .push_bind(learning_item_id);
    }
    if let Some(material_id) = material_id {
        query.push(" AND material_id = ").push_bind(material_id);
    }
    if let Some((start, end)) = bounds {
        query
            .push(" AND occurred_at >= ")
            .push_bind(start)
            .push(" AND occurred_at < ")
            .push_bind(end);
    }
    query
        .push(" ORDER BY occurred_at DESC, id DESC LIMIT ")
        .push_bind(limit)
        .push(" OFFSET ")
        .push_bind(offset);
    query
        .build_query_as::<LearningActivityEventDto>()
        .fetch_all(pool)
        .await
        .map_err(AppError::from)
}

async fn fetch_summary(
    pool: &PgPool,
    user_id: Uuid,
    material_id: Option<Uuid>,
    bounds: Option<(DateTime<Utc>, DateTime<Utc>)>,
) -> Result<(i64, i64, BTreeMap<String, i64>), AppError> {
    let (start, end) = bounds.map_or((None, None), |(start, end)| (Some(start), Some(end)));
    let (total_events, unique_learning_items) = sqlx::query_as::<_, (i64, i64)>(
        r#"
        SELECT COUNT(*)::BIGINT,
               COUNT(DISTINCT learning_item_id)::BIGINT
        FROM learning_activity_events
        WHERE user_id = $1
          AND ($2::UUID IS NULL OR material_id = $2)
          AND ($3::TIMESTAMPTZ IS NULL OR occurred_at >= $3)
          AND ($4::TIMESTAMPTZ IS NULL OR occurred_at < $4)
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .bind(start)
    .bind(end)
    .fetch_one(pool)
    .await?;
    let counts = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT event_type, COUNT(*)::BIGINT
        FROM learning_activity_events
        WHERE user_id = $1
          AND ($2::UUID IS NULL OR material_id = $2)
          AND ($3::TIMESTAMPTZ IS NULL OR occurred_at >= $3)
          AND ($4::TIMESTAMPTZ IS NULL OR occurred_at < $4)
        GROUP BY event_type
        ORDER BY event_type
        "#,
    )
    .bind(user_id)
    .bind(material_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;
    Ok((
        total_events,
        unique_learning_items,
        counts.into_iter().collect(),
    ))
}

fn validate_event_type(event_type: &str) -> Result<(), AppError> {
    if !matches!(
        event_type,
        "read"
            | "create"
            | "accept"
            | "reject"
            | "archive"
            | "restore"
            | "organize"
            | "local_preview"
            | "merge"
            | "migrate"
    ) {
        return Err(AppError::bad_request(
            "invalid_learning_activity_event",
            "unsupported learning activity event type",
        ));
    }
    Ok(())
}

fn validate_metadata(metadata: &Value) -> Result<(), AppError> {
    if !metadata.is_object() {
        return Err(AppError::bad_request(
            "invalid_learning_activity_event",
            "activity metadata must be an object",
        ));
    }
    Ok(())
}

fn normalize_idempotency_key(value: Option<String>) -> Result<Option<String>, AppError> {
    let value = value.map(|value| value.trim().to_string());
    if value.as_deref() == Some("") {
        return Err(AppError::bad_request(
            "invalid_learning_activity_event",
            "idempotency_key must not be empty",
        ));
    }
    Ok(value)
}

fn parse_date(value: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").map_err(|_| {
        AppError::bad_request(
            "invalid_learning_review_date",
            "date must use YYYY-MM-DD format",
        )
    })
}

fn validate_timezone_offset(value: Option<i32>) -> Result<i32, AppError> {
    let value = value.unwrap_or(480);
    if !(-840..=840).contains(&value) {
        return Err(AppError::bad_request(
            "invalid_timezone_offset",
            "timezone_offset_minutes must be between -840 and 840",
        ));
    }
    Ok(value)
}

fn date_bounds(date: NaiveDate, timezone_offset_minutes: i32) -> (DateTime<Utc>, DateTime<Utc>) {
    let local_start = date.and_hms_opt(0, 0, 0).expect("midnight is valid");
    let utc_start = DateTime::<Utc>::from_naive_utc_and_offset(
        local_start - Duration::minutes(i64::from(timezone_offset_minutes)),
        Utc,
    );
    (utc_start, utc_start + Duration::days(1))
}

fn event_payload_sha256(
    learning_item_id: Option<Uuid>,
    material_id: Option<Uuid>,
    event_type: &str,
    metadata: &Value,
) -> String {
    let payload = json!({
        "learning_item_id": learning_item_id,
        "material_id": material_id,
        "event_type": event_type,
        "metadata": metadata,
    });
    let mut hasher = Sha256::new();
    hasher.update(payload.to_string().as_bytes());
    hex::encode(hasher.finalize())
}

pub(crate) fn reading_event_metadata(
    reader_kind: &str,
    locator: &Value,
    progress_ratio: f64,
    status: &str,
) -> Value {
    json!({
        "reader_kind": reader_kind,
        "locator": locator,
        "progress_ratio": progress_ratio,
        "status": status,
    })
}
