use std::path::Path;

use reqwest::{multipart, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{AppConfig, Article, ArticleSegment};

#[derive(Debug, Clone)]
pub struct BackendClient {
    client: Client,
    base_url: String,
    auth_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendClientConfig {
    pub base_url: String,
    pub auth_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendHealthResponse {
    pub status: String,
    pub service: String,
    pub version: String,
    pub auth: BackendAuthHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendAuthHealth {
    pub configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendFile {
    pub id: String,
    pub original_name: String,
    pub content_type: Option<String>,
    pub byte_size: i64,
    pub sha256: String,
    pub download_url: String,
    pub metadata: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMaterialRequest {
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub media_path: Option<String>,
    #[serde(default)]
    pub book_path: Option<String>,
    #[serde(default)]
    pub book_type: Option<String>,
    #[serde(default)]
    pub translated: Option<bool>,
    #[serde(default)]
    pub active_mind_map_artifact_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub segments: Option<Vec<ArticleSegment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PatchMaterialRequest {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub media_path: Option<String>,
    #[serde(default)]
    pub book_path: Option<String>,
    #[serde(default)]
    pub book_type: Option<String>,
    #[serde(default)]
    pub translated: Option<bool>,
    #[serde(default)]
    pub active_mind_map_artifact_id: Option<String>,
    #[serde(default)]
    pub metadata: Option<Value>,
    #[serde(default)]
    pub segments: Option<Vec<ArticleSegment>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendErrorBody {
    error: BackendErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackendErrorDetail {
    code: String,
    message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BackendClientError {
    #[error("backend is not configured")]
    NotConfigured,
    #[error("backend request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("backend file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("backend returned {status}: {code}: {message}")]
    Backend {
        status: StatusCode,
        code: String,
        message: String,
    },
    #[error("file name is invalid")]
    InvalidFileName,
}

impl BackendClient {
    pub fn from_app_config(config: &AppConfig) -> Result<Self, BackendClientError> {
        let config = BackendClientConfig::from_app_config(config)?;
        Ok(Self::new(config))
    }

    pub fn new(config: BackendClientConfig) -> Self {
        Self {
            client: Client::new(),
            base_url: normalize_base_url(&config.base_url),
            auth_token: config.auth_token,
        }
    }

    pub async fn health(&self) -> Result<BackendHealthResponse, BackendClientError> {
        let response = self.client.get(self.url("/health")).send().await?;
        self.parse_response(response).await
    }

    pub async fn list_materials(&self) -> Result<Vec<Article>, BackendClientError> {
        let response = self
            .client
            .get(self.url("/materials"))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn get_material(&self, id: &str) -> Result<Article, BackendClientError> {
        let response = self
            .client
            .get(self.url(&format!("/materials/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn create_material(
        &self,
        payload: &CreateMaterialRequest,
    ) -> Result<Article, BackendClientError> {
        let response = self
            .client
            .post(self.url("/materials"))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn patch_material(
        &self,
        id: &str,
        payload: &PatchMaterialRequest,
    ) -> Result<Article, BackendClientError> {
        let response = self
            .client
            .patch(self.url(&format!("/materials/{id}")))
            .bearer_auth(&self.auth_token)
            .json(payload)
            .send()
            .await?;
        self.parse_response(response).await
    }

    pub async fn delete_material(&self, id: &str) -> Result<(), BackendClientError> {
        let response = self
            .client
            .delete(self.url(&format!("/materials/{id}")))
            .bearer_auth(&self.auth_token)
            .send()
            .await?;
        self.parse_empty_response(response).await
    }

    pub async fn upload_file_path(
        &self,
        path: &Path,
        metadata: Option<Value>,
    ) -> Result<BackendFile, BackendClientError> {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(BackendClientError::InvalidFileName)?
            .to_string();
        let bytes = tokio::fs::read(path).await?;
        let file_part = multipart::Part::bytes(bytes).file_name(file_name);
        let mut form = multipart::Form::new().part("file", file_part);
        if let Some(metadata) = metadata {
            form = form.text("metadata", metadata.to_string());
        }

        let response = self
            .client
            .post(self.url("/files"))
            .bearer_auth(&self.auth_token)
            .multipart(form)
            .send()
            .await?;
        self.parse_response(response).await
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn parse_response<T: for<'de> Deserialize<'de>>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, BackendClientError> {
        let status = response.status();
        if status.is_success() {
            return Ok(response.json::<T>().await?);
        }

        Err(parse_backend_error(status, response.text().await.ok()))
    }

    async fn parse_empty_response(
        &self,
        response: reqwest::Response,
    ) -> Result<(), BackendClientError> {
        let status = response.status();
        if status.is_success() {
            return Ok(());
        }

        Err(parse_backend_error(status, response.text().await.ok()))
    }
}

impl BackendClientConfig {
    pub fn from_app_config(config: &AppConfig) -> Result<Self, BackendClientError> {
        let base_url = config
            .backend_url
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .ok_or(BackendClientError::NotConfigured)?;
        let auth_token = config
            .auth_token
            .as_ref()
            .filter(|value| !value.trim().is_empty())
            .ok_or(BackendClientError::NotConfigured)?;

        Ok(Self {
            base_url: normalize_base_url(base_url),
            auth_token: auth_token.trim().to_string(),
        })
    }
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

fn parse_backend_error(status: StatusCode, body: Option<String>) -> BackendClientError {
    if let Some(body) = body {
        if let Ok(parsed) = serde_json::from_str::<BackendErrorBody>(&body) {
            return BackendClientError::Backend {
                status,
                code: parsed.error.code,
                message: parsed.error.message,
            };
        }
    }

    BackendClientError::Backend {
        status,
        code: "backend_error".to_string(),
        message: "backend request failed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_requires_url_and_token() {
        let config = AppConfig {
            backend_url: Some("http://127.0.0.1:4000/".to_string()),
            auth_token: Some(" token ".to_string()),
            ..Default::default()
        };
        let resolved = BackendClientConfig::from_app_config(&config).unwrap();

        assert_eq!(resolved.base_url, "http://127.0.0.1:4000");
        assert_eq!(resolved.auth_token, "token");

        let missing = AppConfig::default();
        assert!(matches!(
            BackendClientConfig::from_app_config(&missing),
            Err(BackendClientError::NotConfigured)
        ));
    }

    #[test]
    fn parses_structured_backend_error() {
        let error = parse_backend_error(
            StatusCode::UNAUTHORIZED,
            Some(r#"{"error":{"code":"invalid_token","message":"invalid bearer token"}}"#.into()),
        );

        match error {
            BackendClientError::Backend {
                status,
                code,
                message,
            } => {
                assert_eq!(status, StatusCode::UNAUTHORIZED);
                assert_eq!(code, "invalid_token");
                assert_eq!(message, "invalid bearer token");
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
