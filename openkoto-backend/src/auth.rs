use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::{rejection::JsonRejection, FromRequest, FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgDatabaseError, PgPool};
use uuid::Uuid;

use crate::{config::AppConfig, error::AppError, routes::AppState};

const TOKEN_TTL_DAYS: i64 = 30;
const MIN_PASSWORD_LEN: usize = 8;
const MAX_PASSWORD_LEN: usize = 256;
const MAX_EMAIL_LEN: usize = 320;

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token_type: &'static str,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub user: UserDto,
}

#[derive(Debug, Serialize)]
pub struct CurrentUserResponse {
    pub user: UserDto,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct UserDto {
    pub id: Uuid,
    pub email: String,
    pub display_name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub user: UserDto,
}

#[derive(Debug, sqlx::FromRow)]
struct UserWithPassword {
    id: Uuid,
    email: String,
    password_hash: String,
    display_name: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    jti: String,
    iat: usize,
    exp: usize,
}

pub struct ApiJson<T>(pub T);

impl<S, T> FromRequest<S> for ApiJson<T>
where
    Json<T>: FromRequest<S, Rejection = JsonRejection>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        Json::<T>::from_request(req, state)
            .await
            .map(|Json(value)| Self(value))
            .map_err(|_| {
                AppError::bad_request("invalid_request", "request body must be valid json")
            })
    }
}

pub async fn register(
    State(state): State<AppState>,
    ApiJson(payload): ApiJson<RegisterRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    require_auth_secret(&state.config)?;

    let email_normalized = normalize_email(&payload.email)?;
    validate_password(&payload.password)?;

    let email = payload.email.trim().to_string();
    let display_name = normalize_display_name(payload.display_name);
    let password_hash = hash_password(payload.password).await?;
    let user = insert_user(
        &state.pool,
        email,
        email_normalized,
        password_hash,
        display_name,
    )
    .await?;
    let token = create_session_token(&state.pool, &state.config, user.id).await?;

    Ok(Json(AuthResponse {
        token_type: "bearer",
        token: token.value,
        expires_at: token.expires_at,
        user,
    }))
}

pub async fn login(
    State(state): State<AppState>,
    ApiJson(payload): ApiJson<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    require_auth_secret(&state.config)?;

    let email_normalized = normalize_email(&payload.email)?;
    let user = sqlx::query_as::<_, UserWithPassword>(
        r#"
        SELECT id, email, password_hash, display_name, created_at, updated_at
        FROM users
        WHERE email_normalized = $1
        "#,
    )
    .bind(&email_normalized)
    .fetch_optional(&state.pool)
    .await?;

    let Some(user) = user else {
        return Err(invalid_credentials());
    };

    if !verify_password(payload.password, user.password_hash.clone()).await? {
        return Err(invalid_credentials());
    }

    let user = UserDto {
        id: user.id,
        email: user.email,
        display_name: user.display_name,
        created_at: user.created_at,
        updated_at: user.updated_at,
    };
    let token = create_session_token(&state.pool, &state.config, user.id).await?;

    Ok(Json(AuthResponse {
        token_type: "bearer",
        token: token.value,
        expires_at: token.expires_at,
        user,
    }))
}

pub async fn me(
    AuthenticatedUser { user }: AuthenticatedUser,
) -> Result<Json<CurrentUserResponse>, AppError> {
    Ok(Json(CurrentUserResponse { user }))
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = bearer_token(parts)?;
        let claims = decode_token(&state.config, token)?;
        let token_hash = token_hash(token);
        let user_id = Uuid::parse_str(&claims.sub)
            .map_err(|_| AppError::unauthorized("invalid_token", "invalid bearer token"))?;

        let user = sqlx::query_as::<_, UserDto>(
            r#"
            SELECT u.id, u.email, u.display_name, u.created_at, u.updated_at
            FROM sessions s
            JOIN users u ON u.id = s.user_id
            WHERE s.token_hash = $1
              AND s.user_id = $2
              AND s.revoked_at IS NULL
              AND s.expires_at > NOW()
            "#,
        )
        .bind(&token_hash)
        .bind(user_id)
        .fetch_optional(&state.pool)
        .await?
        .ok_or_else(|| AppError::unauthorized("invalid_token", "invalid bearer token"))?;

        sqlx::query(
            r#"
            UPDATE sessions
            SET last_used_at = NOW()
            WHERE token_hash = $1
            "#,
        )
        .bind(&token_hash)
        .execute(&state.pool)
        .await?;

        Ok(Self { user })
    }
}

struct CreatedToken {
    value: String,
    expires_at: DateTime<Utc>,
}

async fn insert_user(
    pool: &PgPool,
    email: String,
    email_normalized: String,
    password_hash: String,
    display_name: Option<String>,
) -> Result<UserDto, AppError> {
    let user = sqlx::query_as::<_, UserDto>(
        r#"
        INSERT INTO users (id, email, email_normalized, password_hash, display_name)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, email, display_name, created_at, updated_at
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(email)
    .bind(email_normalized)
    .bind(password_hash)
    .bind(display_name)
    .fetch_one(pool)
    .await
    .map_err(map_insert_user_error)?;

    Ok(user)
}

async fn create_session_token(
    pool: &PgPool,
    config: &AppConfig,
    user_id: Uuid,
) -> Result<CreatedToken, AppError> {
    let secret = require_auth_secret(config)?;
    let now = Utc::now();
    let expires_at = now + Duration::days(TOKEN_TTL_DAYS);
    let claims = Claims {
        sub: user_id.to_string(),
        jti: Uuid::new_v4().to_string(),
        iat: now.timestamp() as usize,
        exp: expires_at.timestamp() as usize,
    };
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AppError::internal("token_signing_failed", "failed to create auth token"))?;
    let token_hash = token_hash(&token);

    sqlx::query(
        r#"
        INSERT INTO sessions (id, user_id, token_hash, created_at, expires_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(&token_hash)
    .bind(now)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(CreatedToken {
        value: token,
        expires_at,
    })
}

fn decode_token(config: &AppConfig, token: &str) -> Result<Claims, AppError> {
    let secret = require_auth_secret(config)?;
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )
    .map(|data| data.claims)
    .map_err(|_| AppError::unauthorized("invalid_token", "invalid bearer token"))
}

fn require_auth_secret(config: &AppConfig) -> Result<&str, AppError> {
    if config.auth_is_configured() {
        Ok(config.jwt_secret.trim())
    } else {
        Err(AppError::internal(
            "auth_not_configured",
            "authentication is not configured",
        ))
    }
}

async fn hash_password(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|_| AppError::internal("password_hash_failed", "failed to hash password"))
    })
    .await
    .map_err(|_| AppError::internal("password_hash_failed", "failed to hash password"))?
}

async fn verify_password(password: String, password_hash: String) -> Result<bool, AppError> {
    tokio::task::spawn_blocking(move || {
        let parsed_hash = PasswordHash::new(&password_hash).map_err(|_| {
            AppError::internal("password_verify_failed", "failed to verify password")
        })?;

        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok())
    })
    .await
    .map_err(|_| AppError::internal("password_verify_failed", "failed to verify password"))?
}

fn normalize_email(email: &str) -> Result<String, AppError> {
    let email = email.trim().to_ascii_lowercase();

    if email.is_empty()
        || email.len() > MAX_EMAIL_LEN
        || !email.contains('@')
        || email.starts_with('@')
        || email.ends_with('@')
        || email.contains(char::is_whitespace)
    {
        return Err(AppError::bad_request(
            "invalid_email",
            "email address is invalid",
        ));
    }

    Ok(email)
}

fn normalize_display_name(display_name: Option<String>) -> Option<String> {
    display_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_password(password: &str) -> Result<(), AppError> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(AppError::bad_request(
            "weak_password",
            "password must be at least 8 characters",
        ));
    }
    if password.len() > MAX_PASSWORD_LEN {
        return Err(AppError::bad_request(
            "password_too_long",
            "password must not exceed 256 characters",
        ));
    }

    Ok(())
}

fn bearer_token(parts: &Parts) -> Result<&str, AppError> {
    let header = parts
        .headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| AppError::unauthorized("missing_bearer_token", "missing bearer token"))?;

    header
        .strip_prefix("Bearer ")
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| AppError::unauthorized("invalid_token", "invalid bearer token"))
}

fn token_hash(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

fn invalid_credentials() -> AppError {
    AppError::unauthorized("invalid_credentials", "invalid email or password")
}

fn map_insert_user_error(error: sqlx::Error) -> AppError {
    if is_unique_violation(&error, "users_email_normalized_key") {
        AppError::conflict("email_already_registered", "email is already registered")
    } else {
        AppError::Database(error)
    }
}

fn is_unique_violation(error: &sqlx::Error, constraint: &str) -> bool {
    error
        .as_database_error()
        .and_then(|database_error| database_error.try_downcast_ref::<PgDatabaseError>())
        .is_some_and(|database_error| {
            database_error.code() == "23505" && database_error.constraint() == Some(constraint)
        })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::{
        body::{to_bytes, Body},
        http::{header::CONTENT_TYPE, Method, Request, StatusCode},
        Router,
    };
    use serde_json::{json, Value};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    use super::*;
    use crate::{database::MIGRATOR, routes::build_router};

    const TEST_SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn normalizes_email_for_lookup() {
        assert_eq!(
            normalize_email("  User.Name+Tag@Example.COM  ").unwrap(),
            "user.name+tag@example.com"
        );
    }

    #[test]
    fn rejects_invalid_email() {
        assert!(normalize_email("not-an-email").is_err());
        assert!(normalize_email("@example.com").is_err());
        assert!(normalize_email("user@example.com other").is_err());
    }

    #[test]
    fn rejects_weak_password() {
        assert!(validate_password("1234567").is_err());
        assert!(validate_password("12345678").is_ok());
    }

    #[tokio::test]
    async fn hashes_and_verifies_password() {
        let hash = hash_password("correct horse battery staple".to_string())
            .await
            .unwrap();

        assert!(
            verify_password("correct horse battery staple".to_string(), hash.clone())
                .await
                .unwrap()
        );
        assert!(!verify_password("wrong password".to_string(), hash)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn auth_flow_with_database_when_configured() {
        let Some(database_url) = std::env::var("OPENKOTO_TEST_DATABASE_URL").ok() else {
            return;
        };
        let pool = PgPoolOptions::new()
            .max_connections(2)
            .connect(&database_url)
            .await
            .unwrap();
        MIGRATOR.run(&pool).await.unwrap();
        let app = test_app(pool, database_url);
        let email = format!("user-{}@example.com", Uuid::new_v4());

        let (status, body) = json_request(
            app.clone(),
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "password": "correct horse battery staple",
                "display_name": "Reader"
            }),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["token_type"], "bearer");
        assert_eq!(body["user"]["email"], email);
        assert!(body["user"].get("password_hash").is_none());
        let token = body["token"].as_str().unwrap().to_string();
        let user_id = body["user"]["id"].as_str().unwrap().to_string();

        let (status, body) = json_request(
            app.clone(),
            Method::POST,
            "/auth/register",
            json!({
                "email": email,
                "password": "correct horse battery staple"
            }),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(body["error"]["code"], "email_already_registered");

        let (status, body) = json_request(
            app.clone(),
            Method::POST,
            "/auth/register",
            json!({
                "email": "missing-password@example.com"
            }),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"]["code"], "invalid_request");

        let (status, body) = json_request(
            app.clone(),
            Method::POST,
            "/auth/login",
            json!({
                "email": email,
                "password": "wrong password"
            }),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "invalid_credentials");

        let (status, body) = json_request(
            app.clone(),
            Method::POST,
            "/auth/login",
            json!({
                "email": email,
                "password": "correct horse battery staple"
            }),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let login_token = body["token"].as_str().unwrap().to_string();

        let (status, body) =
            json_request(app.clone(), Method::GET, "/auth/me", Value::Null, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error"]["code"], "missing_bearer_token");

        let (status, body) = json_request(
            app.clone(),
            Method::GET,
            "/auth/me",
            Value::Null,
            Some(&token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["id"], user_id);

        let second_email = format!("user-{}@example.com", Uuid::new_v4());
        let (status, body) = json_request(
            app.clone(),
            Method::POST,
            "/auth/register",
            json!({
                "email": second_email,
                "password": "correct horse battery staple"
            }),
            None,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let second_token = body["token"].as_str().unwrap().to_string();

        let (status, body) = json_request(
            app.clone(),
            Method::GET,
            "/auth/me",
            Value::Null,
            Some(&second_token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_ne!(body["user"]["id"], user_id);

        let (status, body) = json_request(
            app,
            Method::GET,
            "/auth/me",
            Value::Null,
            Some(&login_token),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["user"]["email"], email);
    }

    fn test_app(pool: PgPool, database_url: String) -> Router {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some(database_url.clone()),
            "OPENKOTO_JWT_SECRET" => Some(TEST_SECRET.to_string()),
            "OPENKOTO_FILE_STORAGE_DIR" => Some(".data/test-files".to_string()),
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
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header(CONTENT_TYPE, "application/json");
        if let Some(token) = token {
            builder = builder.header(AUTHORIZATION, format!("Bearer {token}"));
        }

        let request_body = if body.is_null() {
            Body::empty()
        } else {
            Body::from(body.to_string())
        };
        let response = app
            .oneshot(builder.body(request_body).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };

        (status, value)
    }
}
