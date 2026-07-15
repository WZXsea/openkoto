use serde::Serialize;
use tauri::{AppHandle, State};

use crate::{
    ai_service::{get_or_create_ai_service, AIServiceCache},
    backend_client::{BackendClient, BackendHealthResponse, BackendUser},
    storage::{ensure_app_dirs, load_config, save_config},
    types::{AppConfig, ModelConfig},
};

use super::service::{
    activate_model_config, backend_error_to_string, is_invalid_backend_token, remove_model_config,
    save_backend_auth_result, upsert_legacy_api_key, upsert_model_config,
};

pub const CONFIG_AUTH_COMMAND_NAMES: [&str; 13] = [
    "init_app",
    "get_config",
    "save_config_cmd",
    "backend_check_session_cmd",
    "backend_health_cmd",
    "backend_login_cmd",
    "backend_register_cmd",
    "backend_logout_cmd",
    "save_model_config",
    "delete_model_config",
    "set_active_model_config",
    "get_active_model_config",
    "set_api_key",
];

#[derive(Debug, Clone, Serialize)]
pub struct BackendSessionCheck {
    pub configured: bool,
    pub connected: bool,
    pub authenticated: bool,
    pub backend_url: Option<String>,
    pub user: Option<BackendUser>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackendAuthResult {
    pub config: AppConfig,
    pub user: BackendUser,
    pub expires_at: String,
}

#[tauri::command]
pub async fn init_app(app_handle: AppHandle) -> Result<String, String> {
    ensure_app_dirs(&app_handle)?;
    Ok("App initialized successfully".to_string())
}

#[tauri::command]
pub async fn get_config(
    app_handle: AppHandle,
    state: State<'_, AIServiceCache>,
) -> Result<Option<AppConfig>, String> {
    let config = load_config(&app_handle)?;
    if let Some(app_config) = config.as_ref() {
        if let Some(model_config) = app_config
            .active_model_id
            .as_deref()
            .and_then(|active_id| app_config.get_config(active_id))
        {
            let _ = get_or_create_ai_service(
                &state,
                model_config.api_key.clone(),
                model_config.api_provider.clone(),
                model_config.model.clone(),
                model_config.base_url.clone(),
            )
            .await;
        }
    }
    Ok(config)
}

#[tauri::command]
pub async fn save_config_cmd(app_handle: AppHandle, config: AppConfig) -> Result<String, String> {
    save_config(&app_handle, &config)?;
    Ok("Configuration saved".to_string())
}

#[tauri::command]
pub async fn backend_check_session_cmd(
    app_handle: AppHandle,
) -> Result<BackendSessionCheck, String> {
    let mut config = load_config(&app_handle)?.unwrap_or_default();
    let backend_url = config
        .backend_url
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let Some(backend_url) = backend_url else {
        return Ok(BackendSessionCheck {
            configured: false,
            connected: false,
            authenticated: false,
            backend_url: None,
            user: None,
            error: None,
        });
    };

    let health_client =
        BackendClient::for_base_url(&backend_url).map_err(backend_error_to_string)?;
    let health = match health_client.health().await {
        Ok(health) => health,
        Err(error) => {
            return Ok(BackendSessionCheck {
                configured: true,
                connected: false,
                authenticated: false,
                backend_url: Some(backend_url),
                user: None,
                error: Some(backend_error_to_string(error)),
            });
        }
    };

    if !health.auth.configured {
        return Ok(BackendSessionCheck {
            configured: true,
            connected: true,
            authenticated: false,
            backend_url: Some(backend_url),
            user: None,
            error: Some("Backend authentication is not configured. Set OPENKOTO_JWT_SECRET and restart the backend.".to_string()),
        });
    }

    if config
        .auth_token
        .as_ref()
        .map(|value| value.trim().is_empty())
        .unwrap_or(true)
    {
        return Ok(BackendSessionCheck {
            configured: true,
            connected: true,
            authenticated: false,
            backend_url: Some(backend_url),
            user: None,
            error: None,
        });
    }

    match BackendClient::from_app_config(&config)
        .map_err(backend_error_to_string)?
        .me()
        .await
    {
        Ok(current) => Ok(BackendSessionCheck {
            configured: true,
            connected: true,
            authenticated: true,
            backend_url: Some(backend_url),
            user: Some(current.user),
            error: None,
        }),
        Err(error) => {
            let message = if is_invalid_backend_token(&error) {
                config.auth_token = None;
                save_config(&app_handle, &config)?;
                "登录已失效，请重新登录。".to_string()
            } else {
                backend_error_to_string(error)
            };
            Ok(BackendSessionCheck {
                configured: true,
                connected: true,
                authenticated: false,
                backend_url: Some(backend_url),
                user: None,
                error: Some(message),
            })
        }
    }
}

#[tauri::command]
pub async fn backend_health_cmd(backend_url: String) -> Result<BackendHealthResponse, String> {
    BackendClient::for_base_url(&backend_url)
        .map_err(backend_error_to_string)?
        .health()
        .await
        .map_err(backend_error_to_string)
}

#[tauri::command]
pub async fn backend_login_cmd(
    app_handle: AppHandle,
    backend_url: String,
    email: String,
    password: String,
) -> Result<BackendAuthResult, String> {
    let client = BackendClient::for_base_url(&backend_url).map_err(backend_error_to_string)?;
    let health = client.health().await.map_err(backend_error_to_string)?;
    if !health.auth.configured {
        return Err("Backend authentication is not configured. Set OPENKOTO_JWT_SECRET and restart the backend.".to_string());
    }
    let auth = client
        .login(email.trim(), &password)
        .await
        .map_err(backend_error_to_string)?;
    let (config, user, expires_at) = save_backend_auth_result(&app_handle, backend_url, auth)?;
    Ok(BackendAuthResult {
        config,
        user,
        expires_at,
    })
}

#[tauri::command]
pub async fn backend_register_cmd(
    app_handle: AppHandle,
    backend_url: String,
    email: String,
    password: String,
    display_name: Option<String>,
) -> Result<BackendAuthResult, String> {
    let client = BackendClient::for_base_url(&backend_url).map_err(backend_error_to_string)?;
    let health = client.health().await.map_err(backend_error_to_string)?;
    if !health.auth.configured {
        return Err("Backend authentication is not configured. Set OPENKOTO_JWT_SECRET and restart the backend.".to_string());
    }
    let display_name = display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let auth = client
        .register(email.trim(), &password, display_name)
        .await
        .map_err(backend_error_to_string)?;
    let (config, user, expires_at) = save_backend_auth_result(&app_handle, backend_url, auth)?;
    Ok(BackendAuthResult {
        config,
        user,
        expires_at,
    })
}

#[tauri::command]
pub async fn backend_logout_cmd(app_handle: AppHandle) -> Result<AppConfig, String> {
    let mut config = load_config(&app_handle)?.unwrap_or_default();
    config.auth_token = None;
    save_config(&app_handle, &config)?;
    Ok(config)
}

#[tauri::command]
pub async fn save_model_config(
    app_handle: AppHandle,
    state: State<'_, AIServiceCache>,
    config: ModelConfig,
) -> Result<ModelConfig, String> {
    let mut app_config = load_config(&app_handle)?.unwrap_or_default();
    upsert_model_config(&mut app_config, config.clone());
    save_config(&app_handle, &app_config)?;
    if app_config.active_model_id.as_ref() == Some(&config.id) {
        get_or_create_ai_service(
            &state,
            config.api_key.clone(),
            config.api_provider.clone(),
            config.model.clone(),
            config.base_url.clone(),
        )
        .await?;
    }
    Ok(config)
}

#[tauri::command]
pub async fn delete_model_config(app_handle: AppHandle, config_id: String) -> Result<(), String> {
    let mut app_config = load_config(&app_handle)?.unwrap_or_default();
    remove_model_config(&mut app_config, &config_id)?;
    save_config(&app_handle, &app_config)
}

#[tauri::command]
pub async fn set_active_model_config(
    app_handle: AppHandle,
    state: State<'_, AIServiceCache>,
    config_id: String,
) -> Result<ModelConfig, String> {
    let mut app_config = load_config(&app_handle)?.unwrap_or_default();
    let config = activate_model_config(&mut app_config, &config_id)?;
    save_config(&app_handle, &app_config)?;
    get_or_create_ai_service(
        &state,
        config.api_key.clone(),
        config.api_provider.clone(),
        config.model.clone(),
        config.base_url.clone(),
    )
    .await?;
    Ok(config)
}

#[tauri::command]
pub async fn get_active_model_config(app_handle: AppHandle) -> Result<Option<ModelConfig>, String> {
    let app_config = load_config(&app_handle)?.unwrap_or_default();
    Ok(app_config.get_active_config().cloned())
}

#[tauri::command]
pub async fn set_api_key(
    app_handle: AppHandle,
    state: State<'_, AIServiceCache>,
    api_key: String,
    provider: String,
    model: String,
) -> Result<String, String> {
    let mut app_config = load_config(&app_handle)?.unwrap_or_default();
    let config = upsert_legacy_api_key(&mut app_config, api_key, provider, model);
    save_config(&app_handle, &app_config)?;
    get_or_create_ai_service(
        &state,
        config.api_key,
        config.api_provider,
        config.model,
        config.base_url,
    )
    .await?;
    Ok("API key saved successfully".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_name_contract_remains_stable() {
        assert_eq!(
            CONFIG_AUTH_COMMAND_NAMES,
            [
                "init_app",
                "get_config",
                "save_config_cmd",
                "backend_check_session_cmd",
                "backend_health_cmd",
                "backend_login_cmd",
                "backend_register_cmd",
                "backend_logout_cmd",
                "save_model_config",
                "delete_model_config",
                "set_active_model_config",
                "get_active_model_config",
                "set_api_key",
            ]
        );
    }

    #[test]
    fn session_and_auth_dtos_keep_frontend_field_names() {
        let session = BackendSessionCheck {
            configured: true,
            connected: true,
            authenticated: false,
            backend_url: Some("http://127.0.0.1:8787".to_string()),
            user: None,
            error: Some("sign in".to_string()),
        };
        let value = serde_json::to_value(session).unwrap();
        assert_eq!(value["backend_url"], "http://127.0.0.1:8787");
        assert_eq!(value["authenticated"], false);
        assert_eq!(value["error"], "sign in");

        let auth = BackendAuthResult {
            config: AppConfig::default(),
            user: BackendUser {
                id: "user-1".to_string(),
                email: "reader@example.com".to_string(),
                display_name: Some("Reader".to_string()),
                created_at: "2026-07-15T00:00:00Z".to_string(),
                updated_at: "2026-07-15T00:00:00Z".to_string(),
            },
            expires_at: "2026-07-16T00:00:00Z".to_string(),
        };
        let value = serde_json::to_value(auth).unwrap();
        assert_eq!(value["user"]["email"], "reader@example.com");
        assert_eq!(value["expires_at"], "2026-07-16T00:00:00Z");
        assert!(value.get("config").is_some());
    }
}
