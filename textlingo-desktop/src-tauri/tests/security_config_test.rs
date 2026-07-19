use serde_json::Value;
use std::{fs, path::Path};

fn tauri_manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn tauri_security_config_has_csp_and_narrow_asset_scope() {
    let config_path = tauri_manifest_dir().join("tauri.conf.json");
    let config: Value = serde_json::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();

    let csp = config["app"]["security"]["csp"]
        .as_str()
        .expect("CSP must be a non-null string");
    assert!(!csp.trim().is_empty());
    assert_ne!(csp.trim(), "null");
    assert!(csp.contains("media-src 'self' http://127.0.0.1:19420"));
    assert!(csp.contains("connect-src 'self' http://127.0.0.1:19421"));

    let scope = config["app"]["security"]["assetProtocol"]["scope"]
        .as_array()
        .expect("asset protocol scope must be an array");
    let scope_values: Vec<&str> = scope.iter().filter_map(Value::as_str).collect();

    assert!(!scope_values.contains(&"$APPDATA/**"));
    assert!(scope_values.contains(&"$APPDATA/videos/**"));
    assert!(scope_values.contains(&"$APPDATA/books/**"));
}

#[test]
fn default_capability_does_not_grant_frontend_shell_execution() {
    let capability_path = tauri_manifest_dir().join("capabilities/default.json");
    let capability: Value =
        serde_json::from_str(&fs::read_to_string(capability_path).unwrap()).unwrap();

    let permissions = capability["permissions"]
        .as_array()
        .expect("permissions must be an array");

    let permissions_text = serde_json::to_string(permissions).unwrap();
    assert!(!permissions_text.contains("shell:default"));
    assert!(!permissions_text.contains("shell:allow-execute"));
}

#[test]
fn default_bundle_does_not_require_external_sidecars() {
    let config_path = tauri_manifest_dir().join("tauri.conf.json");
    let config: Value = serde_json::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();

    let external_bins = config["bundle"]["externalBin"]
        .as_array()
        .expect("externalBin must be present as an array for phase 1");

    assert!(external_bins.is_empty());
}
