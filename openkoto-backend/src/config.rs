use std::{env, net::SocketAddr, path::PathBuf};

use thiserror::Error;

const DEFAULT_DATABASE_URL: &str =
    "postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev";
const DEFAULT_BIND_ADDR: &str = "127.0.0.1:4000";
const DEFAULT_JWT_SECRET: &str = "openkoto-dev-insecure-change-me";
const DEFAULT_FILE_STORAGE_DIR: &str = ".data/files";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub jwt_secret: String,
    pub file_storage_dir: PathBuf,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid OPENKOTO_BACKEND_BIND: {0}")]
    InvalidBindAddr(String),
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| env::var(key).ok())
    }

    pub fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let database_url =
            lookup("DATABASE_URL").unwrap_or_else(|| DEFAULT_DATABASE_URL.to_string());
        let bind_addr = lookup("OPENKOTO_BACKEND_BIND")
            .unwrap_or_else(|| DEFAULT_BIND_ADDR.to_string())
            .parse::<SocketAddr>()
            .map_err(|error| ConfigError::InvalidBindAddr(error.to_string()))?;
        let jwt_secret =
            lookup("OPENKOTO_JWT_SECRET").unwrap_or_else(|| DEFAULT_JWT_SECRET.to_string());
        let file_storage_dir = lookup("OPENKOTO_FILE_STORAGE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_FILE_STORAGE_DIR));

        Ok(Self {
            database_url,
            bind_addr,
            jwt_secret,
            file_storage_dir,
        })
    }

    pub fn auth_is_configured(&self) -> bool {
        let secret = self.jwt_secret.trim();

        !secret.is_empty() && secret != DEFAULT_JWT_SECRET && secret.as_bytes().len() >= 32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_uses_development_defaults() {
        let config = AppConfig::from_lookup(|_| None).unwrap();

        assert_eq!(config.database_url, DEFAULT_DATABASE_URL);
        assert_eq!(config.bind_addr.to_string(), DEFAULT_BIND_ADDR);
        assert_eq!(config.jwt_secret, DEFAULT_JWT_SECRET);
        assert_eq!(
            config.file_storage_dir,
            PathBuf::from(DEFAULT_FILE_STORAGE_DIR)
        );
        assert!(!config.auth_is_configured());
    }

    #[test]
    fn config_reads_environment_values() {
        let config = AppConfig::from_lookup(|key| match key {
            "DATABASE_URL" => Some("postgres://user:pass@localhost:5432/db".to_string()),
            "OPENKOTO_BACKEND_BIND" => Some("127.0.0.1:4500".to_string()),
            "OPENKOTO_JWT_SECRET" => Some("secret".to_string()),
            "OPENKOTO_FILE_STORAGE_DIR" => Some("/tmp/openkoto-files".to_string()),
            _ => None,
        })
        .unwrap();

        assert_eq!(
            config.database_url,
            "postgres://user:pass@localhost:5432/db"
        );
        assert_eq!(config.bind_addr.to_string(), "127.0.0.1:4500");
        assert_eq!(config.jwt_secret, "secret");
        assert_eq!(
            config.file_storage_dir,
            PathBuf::from("/tmp/openkoto-files")
        );
        assert!(!config.auth_is_configured());
    }

    #[test]
    fn auth_requires_non_default_secret_with_minimum_length() {
        let config = AppConfig::from_lookup(|key| match key {
            "OPENKOTO_JWT_SECRET" => Some("0123456789abcdef0123456789abcdef".to_string()),
            _ => None,
        })
        .unwrap();

        assert!(config.auth_is_configured());
    }
}
