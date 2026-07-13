use openkoto_desktop_lib::types::AppConfig;

#[test]
fn deserializing_old_config_injects_builtin_prompt_features() {
    let old_json = r#"{
        "onboarding_completed": true,
        "active_model_id": null,
        "model_configs": [],
        "target_language": "zh-CN",
        "interface_language": "en"
    }"#;

    let config: AppConfig = serde_json::from_str(old_json).expect("config should parse");

    assert!(config
        .prompt_features
        .iter()
        .any(|item| item.id == "chat.default"));
    assert!(config
        .prompt_features
        .iter()
        .any(|item| item.id == "selection.translate"));
    assert!(config
        .prompt_features
        .iter()
        .any(|item| item.id == "selection.explain"));
    assert!(config
        .prompt_features
        .iter()
        .any(|item| item.id == "selection.grammar"));
}

#[test]
fn deserializing_partial_prompt_features_restores_missing_builtins_and_keeps_custom() {
    let partial_json = r#"{
        "onboarding_completed": true,
        "active_model_id": null,
        "model_configs": [],
        "target_language": "zh-CN",
        "interface_language": "en",
        "prompt_features": [
            {
                "id": "custom.summary",
                "kind": "quick_action",
                "name": "Summary",
                "description": "Summarize selected text",
                "prompt_template": "Summarize: {text}",
                "requires_selection": true,
                "show_in_quick_actions": true,
                "icon": "sparkles",
                "sort_order": 99,
                "enabled": true,
                "is_builtin": false
            }
        ]
    }"#;

    let config: AppConfig = serde_json::from_str(partial_json).expect("config should parse");

    assert!(config
        .prompt_features
        .iter()
        .any(|item| item.id == "custom.summary"));
    assert!(config
        .prompt_features
        .iter()
        .any(|item| item.id == "chat.default"));
}
