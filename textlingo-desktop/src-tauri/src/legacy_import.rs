use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tauri::AppHandle;
use uuid::Uuid;

use crate::{
    backend_client::{
        BackendClient, BackendClientConfig, BackendClientError, CreateMaterialRequest,
        LegacyImportBatch, LegacyImportFailedItem, LegacyImportItem, LegacyImportRequest,
    },
    storage::get_app_data_dir,
    types::Article,
};

const CONFIG_FILE: &str = "config.json";
const ARTICLES_DIR: &str = "articles";
const FAVORITES_VOCAB_DIR: &str = "favorites/vocabulary";
const FAVORITES_GRAMMAR_DIR: &str = "favorites/grammar";
const FAVORITES_PACKS_DIR: &str = "favorites/packs";
const BOOKMARKS_DIR: &str = "bookmarks";
const AGENT_TASKS_DIR: &str = "agent_tasks";
const ARTIFACTS_DIR: &str = "artifacts/articles";
const LEGACY_SCHEMA_VERSION: &str = "legacy-import-v1";
const SOURCE_REPORT_KEY: &str = "source_report";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LegacyImportSourceReport {
    schema_version: String,
    source_sha256: String,
    total_items: usize,
    source_failed_items: usize,
    item_counts: BTreeMap<String, usize>,
}

#[tauri::command]
pub async fn run_legacy_import_cmd(app_handle: AppHandle) -> Result<LegacyImportBatch, String> {
    let mut request = build_legacy_import_request(&app_handle)?;
    let report = finalize_legacy_import_request(&mut request)?;
    let client = backend_client_for_app(&app_handle)?;
    let batch = client
        .create_legacy_import(&request)
        .await
        .map_err(backend_error_to_string)?;
    verify_legacy_import_batch(&request, &report, &batch)?;
    Ok(batch)
}

#[tauri::command]
pub async fn get_legacy_import_cmd(
    app_handle: AppHandle,
    id: String,
) -> Result<LegacyImportBatch, String> {
    backend_client_for_app(&app_handle)?
        .get_legacy_import(&id)
        .await
        .map_err(backend_error_to_string)
}

fn backend_client_for_app(app_handle: &AppHandle) -> Result<BackendClient, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let config_path = data_dir.join(CONFIG_FILE);
    let content = fs::read_to_string(&config_path)
        .map_err(|error| format!("Failed to read backend connection config: {error}"))?;
    let value: Value = serde_json::from_str(&content).map_err(|error| {
        format!("Failed to parse backend connection config. Repair config.json or sign in again: {error}")
    })?;
    let base_url = value
        .get("backend_url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            "Backend is required. Configure backend URL before importing legacy data.".to_string()
        })?;
    let auth_token = value
        .get("auth_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Backend sign-in is required before importing legacy data.".to_string())?;
    Ok(BackendClient::new(BackendClientConfig {
        base_url: base_url.to_string(),
        auth_token: auth_token.to_string(),
    }))
}

fn backend_error_to_string(error: BackendClientError) -> String {
    match error {
        BackendClientError::NotConfigured => {
            "Backend is required. Configure backend URL and sign in before importing legacy data."
                .to_string()
        }
        other => other.to_string(),
    }
}

fn build_legacy_import_request(app_handle: &AppHandle) -> Result<LegacyImportRequest, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let mut request = LegacyImportRequest {
        client_import_id: String::new(),
        schema_version: LEGACY_SCHEMA_VERSION.to_string(),
        source_label: Some("OpenKoto local legacy JSON".to_string()),
        metadata: json!({
            "app_data_dir": data_dir.to_string_lossy(),
            "schema_version": LEGACY_SCHEMA_VERSION
        }),
        config: None,
        failed_items: Vec::new(),
        materials: Vec::new(),
        word_packs: Vec::new(),
        favorite_vocabularies: Vec::new(),
        favorite_grammars: Vec::new(),
        bookmarks: Vec::new(),
        agent_tasks: Vec::new(),
        artifacts: Vec::new(),
    };

    collect_config(&data_dir, &mut request);
    collect_articles(&data_dir, &mut request)?;
    collect_json_dir(
        &data_dir,
        FAVORITES_PACKS_DIR,
        None,
        "word_pack",
        &mut request.word_packs,
        &mut request.failed_items,
    )?;
    collect_json_dir(
        &data_dir,
        FAVORITES_VOCAB_DIR,
        None,
        "favorite_vocabulary",
        &mut request.favorite_vocabularies,
        &mut request.failed_items,
    )?;
    collect_json_dir(
        &data_dir,
        FAVORITES_GRAMMAR_DIR,
        None,
        "favorite_grammar",
        &mut request.favorite_grammars,
        &mut request.failed_items,
    )?;
    collect_json_dir(
        &data_dir,
        BOOKMARKS_DIR,
        None,
        "bookmark",
        &mut request.bookmarks,
        &mut request.failed_items,
    )?;
    collect_json_dir(
        &data_dir,
        AGENT_TASKS_DIR,
        Some(".json"),
        "agent_task",
        &mut request.agent_tasks,
        &mut request.failed_items,
    )?;
    collect_artifacts(&data_dir, &mut request)?;

    Ok(request)
}

fn collect_config(data_dir: &Path, request: &mut LegacyImportRequest) {
    let path = data_dir.join(CONFIG_FILE);
    if !path.exists() {
        return;
    }

    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Ok(value) => request.config = Some(redact_sensitive_config(value)),
            Err(error) => request.failed_items.push(failed_item(
                "config",
                CONFIG_FILE,
                format!("Failed to parse config: {error}"),
                Some(redacted_config_error_payload(&path)),
            )),
        },
        Err(error) => request.failed_items.push(failed_item(
            "config",
            CONFIG_FILE,
            format!("Failed to read config: {error}"),
            None,
        )),
    }
}

fn collect_articles(data_dir: &Path, request: &mut LegacyImportRequest) -> Result<(), String> {
    for (source_id, path) in list_source_files(data_dir, ARTICLES_DIR, None)? {
        match fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Article>(&content) {
                Ok(article) => request.materials.push(LegacyImportItem {
                    source_id,
                    payload: material_payload_from_article(article),
                }),
                Err(error) => request.failed_items.push(failed_item(
                    "material",
                    &source_id,
                    format!("Failed to parse article: {error}"),
                    Some(raw_excerpt_payload(&path, &content)),
                )),
            },
            Err(error) => request.failed_items.push(failed_item(
                "material",
                &source_id,
                format!("Failed to read article: {error}"),
                None,
            )),
        }
    }
    Ok(())
}

fn collect_json_dir<T: DeserializeOwned>(
    data_dir: &Path,
    relative_dir: &str,
    suffix: Option<&str>,
    source_kind: &str,
    target: &mut Vec<LegacyImportItem<T>>,
    failed_items: &mut Vec<LegacyImportFailedItem>,
) -> Result<(), String> {
    for (source_id, path) in list_source_files(data_dir, relative_dir, suffix)? {
        collect_json_file(source_kind, source_id, path, target, failed_items);
    }
    Ok(())
}

fn collect_artifacts(data_dir: &Path, request: &mut LegacyImportRequest) -> Result<(), String> {
    let root = data_dir.join(ARTIFACTS_DIR);
    if !root.exists() {
        return Ok(());
    }

    let mut entries = Vec::new();
    for article_entry in fs::read_dir(&root)
        .map_err(|error| format!("Failed to read artifacts directory: {error}"))?
    {
        let article_entry = article_entry
            .map_err(|error| format!("Failed to read artifact article entry: {error}"))?;
        if !article_entry.path().is_dir() {
            continue;
        }
        let article_id = article_entry.file_name().to_string_lossy().to_string();
        for artifact_entry in fs::read_dir(article_entry.path())
            .map_err(|error| format!("Failed to read artifact directory: {error}"))?
        {
            let artifact_entry = artifact_entry
                .map_err(|error| format!("Failed to read artifact entry: {error}"))?;
            if !artifact_entry.path().is_file() {
                continue;
            }
            let file_name = artifact_entry.file_name().to_string_lossy().to_string();
            let Some(artifact_id) = strip_suffix(&file_name, ".json") else {
                continue;
            };
            entries.push((format!("{article_id}/{artifact_id}"), artifact_entry.path()));
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    for (source_id, path) in entries {
        collect_json_file(
            "artifact",
            source_id,
            path,
            &mut request.artifacts,
            &mut request.failed_items,
        );
    }

    Ok(())
}

fn collect_json_file<T: DeserializeOwned>(
    source_kind: &str,
    source_id: String,
    path: PathBuf,
    target: &mut Vec<LegacyImportItem<T>>,
    failed_items: &mut Vec<LegacyImportFailedItem>,
) {
    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<T>(&content) {
            Ok(payload) => target.push(LegacyImportItem { source_id, payload }),
            Err(error) => failed_items.push(failed_item(
                source_kind,
                &source_id,
                format!("Failed to parse {source_kind}: {error}"),
                Some(raw_excerpt_payload(&path, &content)),
            )),
        },
        Err(error) => failed_items.push(failed_item(
            source_kind,
            &source_id,
            format!("Failed to read {source_kind}: {error}"),
            None,
        )),
    }
}

fn list_source_files(
    data_dir: &Path,
    relative_dir: &str,
    suffix: Option<&str>,
) -> Result<Vec<(String, PathBuf)>, String> {
    let dir = data_dir.join(relative_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in
        fs::read_dir(&dir).map_err(|error| format!("Failed to read {relative_dir}: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("Failed to read {relative_dir} entry: {error}"))?;
        if !entry.path().is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().to_string();
        let source_id = match suffix {
            Some(suffix) => match strip_suffix(&file_name, suffix) {
                Some(value) => value,
                None => continue,
            },
            None => file_name,
        };
        files.push((source_id, entry.path()));
    }
    files.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(files)
}

fn material_payload_from_article(article: Article) -> CreateMaterialRequest {
    let id = Uuid::parse_str(&article.id)
        .ok()
        .map(|value| value.to_string());
    CreateMaterialRequest {
        id,
        title: article.title,
        content: article.content,
        source_type: article.source_type,
        source_url: article.source_url,
        media_path: article.media_path,
        book_path: article.book_path,
        book_type: article.book_type,
        translated: Some(article.translated),
        active_mind_map_artifact_id: article.active_mind_map_artifact_id,
        metadata: Some(json!({
            "legacy_article_id": article.id,
            "legacy_created_at": article.created_at
        })),
        segments: Some(article.segments),
    }
}

fn failed_item(
    source_kind: &str,
    source_id: &str,
    error: String,
    payload: Option<Value>,
) -> LegacyImportFailedItem {
    LegacyImportFailedItem {
        source_kind: source_kind.to_string(),
        source_id: source_id.to_string(),
        error,
        payload,
    }
}

fn raw_excerpt_payload(path: &Path, content: &str) -> Value {
    let excerpt = content.chars().take(512).collect::<String>();
    json!({
        "path": path.to_string_lossy(),
        "raw_excerpt": excerpt
    })
}

fn redacted_config_error_payload(path: &Path) -> Value {
    json!({
        "path": path.to_string_lossy(),
        "raw_excerpt": "[redacted invalid config]"
    })
}

fn strip_suffix(file_name: &str, suffix: &str) -> Option<String> {
    file_name
        .strip_suffix(suffix)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn assign_client_import_id(request: &mut LegacyImportRequest) -> Result<(), String> {
    let mut fingerprint_request = request.clone();
    fingerprint_request.client_import_id.clear();
    let bytes = serde_json::to_vec(&fingerprint_request)
        .map_err(|error| format!("Failed to fingerprint legacy import: {error}"))?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    request.client_import_id = format!("legacy-local-v1-{}", hex::encode(hasher.finalize()));
    Ok(())
}

fn finalize_legacy_import_request(
    request: &mut LegacyImportRequest,
) -> Result<LegacyImportSourceReport, String> {
    let report = build_source_report(request)?;
    let metadata = request.metadata.as_object_mut().ok_or_else(|| {
        "Legacy import metadata must be an object before source reporting".to_string()
    })?;
    metadata.insert(
        SOURCE_REPORT_KEY.to_string(),
        serde_json::to_value(&report)
            .map_err(|error| format!("Failed to serialize legacy import source report: {error}"))?,
    );
    assign_client_import_id(request)?;
    Ok(report)
}

fn build_source_report(request: &LegacyImportRequest) -> Result<LegacyImportSourceReport, String> {
    let mut canonical = request.clone();
    canonical.client_import_id.clear();
    if let Some(metadata) = canonical.metadata.as_object_mut() {
        metadata.remove(SOURCE_REPORT_KEY);
    }
    let source_sha256 = request_sha256(&canonical)?;
    let item_counts = BTreeMap::from([
        ("config".to_string(), usize::from(request.config.is_some())),
        ("materials".to_string(), request.materials.len()),
        ("word_packs".to_string(), request.word_packs.len()),
        (
            "favorite_vocabularies".to_string(),
            request.favorite_vocabularies.len(),
        ),
        (
            "favorite_grammars".to_string(),
            request.favorite_grammars.len(),
        ),
        ("bookmarks".to_string(), request.bookmarks.len()),
        ("agent_tasks".to_string(), request.agent_tasks.len()),
        ("artifacts".to_string(), request.artifacts.len()),
        ("failed_items".to_string(), request.failed_items.len()),
    ]);
    let total_items = item_counts.values().sum();
    Ok(LegacyImportSourceReport {
        schema_version: request.schema_version.clone(),
        source_sha256,
        total_items,
        source_failed_items: request.failed_items.len(),
        item_counts,
    })
}

fn request_sha256(request: &LegacyImportRequest) -> Result<String, String> {
    let bytes = serde_json::to_vec(request)
        .map_err(|error| format!("Failed to checksum legacy import request: {error}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn verify_legacy_import_batch(
    request: &LegacyImportRequest,
    report: &LegacyImportSourceReport,
    batch: &LegacyImportBatch,
) -> Result<(), String> {
    // Backend normalizes material segments into its own DTO before hashing, so a Desktop-side
    // reserialization is not byte-identical for requests containing segments. Verify the server
    // hash shape plus the exact client id/source report/counters/results instead. A future
    // end-to-end checksum must be an explicit Backend source-payload-checksum contract.
    let expected_report = serde_json::to_value(report)
        .map_err(|error| format!("Failed to verify legacy import source report: {error}"))?;
    let reported_source = batch.metadata.get(SOURCE_REPORT_KEY);
    let counters_total = batch.imported_items + batch.skipped_items + batch.failed_items;
    if batch.client_import_id != request.client_import_id
        || !is_sha256(&batch.request_sha256)
        || batch.schema_version != request.schema_version
        || batch.total_items != report.total_items as i32
        || counters_total != batch.total_items
        || batch.items.len() != report.total_items
        || reported_source != Some(&expected_report)
    {
        return Err(
            "Legacy import verification failed; source JSON was retained and must not be deleted. Compare client_import_id, request checksum, source report, counters, and item results before retrying."
                .to_string(),
        );
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn redact_sensitive_config(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| {
                    if is_sensitive_key(&key) {
                        (key, Value::String("[redacted]".to_string()))
                    } else {
                        (key, redact_sensitive_config(value))
                    }
                })
                .collect(),
        ),
        Value::Array(items) => {
            Value::Array(items.into_iter().map(redact_sensitive_config).collect())
        }
        other => other,
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "api_key"
        || key == "auth_token"
        || key == "token"
        || key == "access_token"
        || key == "refresh_token"
        || key == "authorization"
        || key == "cookie"
        || key == "session"
        || key == "secret"
        || key == "password"
        || key.ends_with("_secret")
        || key.ends_with("_key")
        || key.ends_with("_token")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend_client::LegacyImportItemResult;

    #[test]
    fn redacts_config_secrets_recursively() {
        let value = json!({
            "auth_token": "secret",
            "model_configs": [
                {
                    "api_key": "sk-test",
                    "access_token": "access-secret",
                    "name": "OpenAI"
                }
            ],
            "authorization": "Bearer secret",
            "target_language": "zh-CN"
        });

        let redacted = redact_sensitive_config(value);

        assert_eq!(redacted["auth_token"], "[redacted]");
        assert_eq!(redacted["model_configs"][0]["api_key"], "[redacted]");
        assert_eq!(redacted["model_configs"][0]["access_token"], "[redacted]");
        assert_eq!(redacted["authorization"], "[redacted]");
        assert_eq!(redacted["target_language"], "zh-CN");
    }

    #[test]
    fn invalid_config_payload_does_not_include_raw_content() {
        let payload = redacted_config_error_payload(Path::new("/tmp/config.json"));

        assert_eq!(payload["raw_excerpt"], "[redacted invalid config]");
    }

    #[test]
    fn non_uuid_article_id_is_not_sent_as_material_id() {
        let article = Article {
            id: "youtube-video-id".to_string(),
            title: "Title".to_string(),
            content: "Content".to_string(),
            source_type: Some("youtube".to_string()),
            source_url: None,
            media_path: None,
            book_path: None,
            book_type: None,
            created_at: "2026-07-09T00:00:00Z".to_string(),
            translated: false,
            active_mind_map_artifact_id: None,
            segments: Vec::new(),
            metadata: serde_json::json!({}),
            tags: Vec::new(),
            reading_progress: None,
            archived_at: None,
        };

        let payload = material_payload_from_article(article);

        assert!(payload.id.is_none());
        assert_eq!(
            payload.metadata.unwrap()["legacy_article_id"],
            "youtube-video-id"
        );
    }

    #[test]
    fn source_report_and_server_batch_are_checksum_verifiable() {
        let mut request = LegacyImportRequest {
            client_import_id: String::new(),
            schema_version: LEGACY_SCHEMA_VERSION.to_string(),
            source_label: Some("test".to_string()),
            metadata: json!({"app_data_dir": "/tmp/test"}),
            config: None,
            failed_items: vec![failed_item(
                "artifact",
                "broken",
                "invalid JSON".to_string(),
                None,
            )],
            materials: Vec::new(),
            word_packs: Vec::new(),
            favorite_vocabularies: Vec::new(),
            favorite_grammars: Vec::new(),
            bookmarks: Vec::new(),
            agent_tasks: Vec::new(),
            artifacts: Vec::new(),
        };
        let report = finalize_legacy_import_request(&mut request).unwrap();
        let metadata = json!({SOURCE_REPORT_KEY: report.clone()});
        let batch = LegacyImportBatch {
            id: Uuid::new_v4().to_string(),
            client_import_id: request.client_import_id.clone(),
            request_sha256: "a".repeat(64),
            schema_version: LEGACY_SCHEMA_VERSION.to_string(),
            source_label: Some("test".to_string()),
            status: "completed_with_failures".to_string(),
            total_items: 1,
            imported_items: 0,
            skipped_items: 0,
            failed_items: 1,
            metadata,
            created_at: "2026-07-15T00:00:00Z".to_string(),
            finished_at: Some("2026-07-15T00:00:01Z".to_string()),
            items: vec![LegacyImportItemResult {
                id: Uuid::new_v4().to_string(),
                source_kind: "artifact".to_string(),
                source_id: "broken".to_string(),
                target_kind: None,
                target_id: None,
                status: "failed".to_string(),
                error: Some("invalid JSON".to_string()),
                payload: Value::Null,
                created_at: "2026-07-15T00:00:00Z".to_string(),
            }],
        };

        verify_legacy_import_batch(&request, &report, &batch).unwrap();
        let mut tampered = batch;
        tampered.request_sha256 = "invalid".to_string();
        let error = verify_legacy_import_batch(&request, &report, &tampered).unwrap_err();
        assert!(error.contains("source JSON was retained"));
    }

    #[test]
    fn source_report_is_deterministic_and_does_not_include_secrets() {
        let mut request = LegacyImportRequest {
            client_import_id: String::new(),
            schema_version: LEGACY_SCHEMA_VERSION.to_string(),
            source_label: None,
            metadata: json!({"app_data_dir": "/tmp/test"}),
            config: Some(json!({"api_key": "[redacted]"})),
            failed_items: Vec::new(),
            materials: Vec::new(),
            word_packs: Vec::new(),
            favorite_vocabularies: Vec::new(),
            favorite_grammars: Vec::new(),
            bookmarks: Vec::new(),
            agent_tasks: Vec::new(),
            artifacts: Vec::new(),
        };
        let first = finalize_legacy_import_request(&mut request).unwrap();
        let first_id = request.client_import_id.clone();
        let second = finalize_legacy_import_request(&mut request).unwrap();

        assert_eq!(first, second);
        assert_eq!(first_id, request.client_import_id);
        assert_eq!(first.total_items, 1);
        assert!(!serde_json::to_string(&first).unwrap().contains("api_key"));
    }
}
