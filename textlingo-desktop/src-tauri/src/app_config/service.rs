use reqwest::StatusCode;
use tauri::AppHandle;

use crate::{
    backend_client::{BackendAuthResponse, BackendClient, BackendClientError},
    storage::{load_config, save_config},
    types::{AppConfig, ModelConfig},
};

pub(crate) fn backend_client_for_app(app_handle: &AppHandle) -> Result<BackendClient, String> {
    let config = load_config(app_handle)?.unwrap_or_default();
    BackendClient::from_app_config(&config).map_err(backend_error_to_string)
}

pub(crate) fn backend_error_to_string(error: BackendClientError) -> String {
    match error {
        BackendClientError::NotConfigured => {
            "Backend is required. Configure backend URL and sign in before using materials."
                .to_string()
        }
        other => other.to_string(),
    }
}

pub(crate) fn is_invalid_backend_token(error: &BackendClientError) -> bool {
    matches!(
        error,
        BackendClientError::Backend {
            status,
            code,
            ..
        } if *status == StatusCode::UNAUTHORIZED && code == "invalid_token"
    )
}

pub(crate) fn save_backend_auth_result(
    app_handle: &AppHandle,
    backend_url: String,
    auth: BackendAuthResponse,
) -> Result<(AppConfig, crate::backend_client::BackendUser, String), String> {
    let mut config = load_config(app_handle)?.unwrap_or_default();
    config.backend_url = Some(backend_url.trim().trim_end_matches('/').to_string());
    config.auth_token = Some(auth.token);
    save_config(app_handle, &config)?;
    Ok((config, auth.user, auth.expires_at))
}

pub(crate) fn upsert_model_config(app_config: &mut AppConfig, config: ModelConfig) {
    if let Some(index) = app_config
        .model_configs
        .iter()
        .position(|candidate| candidate.id == config.id)
    {
        app_config.model_configs[index] = config.clone();
    } else {
        app_config.model_configs.push(config.clone());
    }

    if app_config.model_configs.len() == 1 || config.is_default {
        app_config.active_model_id = Some(config.id.clone());
        for candidate in &mut app_config.model_configs {
            if candidate.id != config.id {
                candidate.is_default = false;
            }
        }
    }
}

pub(crate) fn remove_model_config(
    app_config: &mut AppConfig,
    config_id: &str,
) -> Result<(), String> {
    let original_len = app_config.model_configs.len();
    app_config
        .model_configs
        .retain(|candidate| candidate.id != config_id);
    if app_config.model_configs.len() == original_len {
        return Err("Configuration not found".to_string());
    }
    if app_config.active_model_id.as_deref() == Some(config_id) {
        app_config.active_model_id = app_config
            .model_configs
            .first()
            .map(|config| config.id.clone());
    }
    Ok(())
}

pub(crate) fn activate_model_config(
    app_config: &mut AppConfig,
    config_id: &str,
) -> Result<ModelConfig, String> {
    let config = app_config
        .get_config(config_id)
        .ok_or("Configuration not found")?
        .clone();
    app_config.active_model_id = Some(config_id.to_string());
    Ok(config)
}

pub(crate) fn upsert_legacy_api_key(
    app_config: &mut AppConfig,
    api_key: String,
    provider: String,
    model: String,
) -> ModelConfig {
    let config = app_config
        .model_configs
        .iter()
        .find(|candidate| candidate.api_provider == provider && candidate.model == model)
        .map(|existing| ModelConfig {
            api_key: api_key.clone(),
            ..existing.clone()
        })
        .unwrap_or_else(|| {
            ModelConfig::new(
                format!("{} - {}", provider, model),
                api_key,
                provider,
                model,
            )
        });
    if let Some(index) = app_config
        .model_configs
        .iter()
        .position(|candidate| candidate.id == config.id)
    {
        app_config.model_configs[index] = config.clone();
    } else {
        app_config.model_configs.push(config.clone());
    }
    app_config.active_model_id = Some(config.id.clone());
    config
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(id: &str, provider: &str, name: &str, is_default: bool) -> ModelConfig {
        ModelConfig {
            id: id.to_string(),
            name: name.to_string(),
            api_key: format!("key-{id}"),
            api_provider: provider.to_string(),
            model: name.to_string(),
            is_default,
            created_at: Some("2026-07-15T00:00:00Z".to_string()),
            base_url: None,
        }
    }

    #[test]
    fn model_config_mutations_preserve_active_selection_contract() {
        let mut config = AppConfig::default();
        upsert_model_config(&mut config, model("first", "openai", "gpt-a", false));
        assert_eq!(config.active_model_id.as_deref(), Some("first"));

        upsert_model_config(&mut config, model("second", "google", "gemini", true));
        assert_eq!(config.active_model_id.as_deref(), Some("second"));
        assert!(!config.model_configs[0].is_default);

        remove_model_config(&mut config, "second").unwrap();
        assert_eq!(config.active_model_id.as_deref(), Some("first"));
        assert_eq!(
            remove_model_config(&mut config, "missing").unwrap_err(),
            "Configuration not found"
        );
    }

    #[test]
    fn legacy_api_key_updates_existing_provider_model_without_duplication() {
        let mut config = AppConfig::default();
        config
            .model_configs
            .push(model("existing", "openai", "gpt-a", false));

        let saved = upsert_legacy_api_key(
            &mut config,
            "replacement-key".to_string(),
            "openai".to_string(),
            "gpt-a".to_string(),
        );

        assert_eq!(saved.id, "existing");
        assert_eq!(saved.api_key, "replacement-key");
        assert_eq!(config.model_configs.len(), 1);
        assert_eq!(config.active_model_id.as_deref(), Some("existing"));
    }

    #[test]
    fn invalid_backend_token_detection_is_specific() {
        let invalid_token = BackendClientError::Backend {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_token".to_string(),
            message: "invalid bearer token".to_string(),
        };
        let wrong_password = BackendClientError::Backend {
            status: StatusCode::UNAUTHORIZED,
            code: "invalid_credentials".to_string(),
            message: "invalid credentials".to_string(),
        };

        assert!(is_invalid_backend_token(&invalid_token));
        assert!(!is_invalid_backend_token(&wrong_password));
    }
}
