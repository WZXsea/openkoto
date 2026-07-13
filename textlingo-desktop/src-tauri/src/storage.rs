use crate::platform::safe_paths::validate_storage_id;
use crate::types::{
    merge_missing_builtin_prompt_features, AgentTask, AppConfig, Article, Artifact,
};
use serde_json;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const CONFIG_FILE: &str = "config.json";
const ARTICLES_DIR: &str = "articles";
// Online task and artifact records live in Backend/PostgreSQL. These files are
// only worker-local recovery checkpoints and must never be used as UI/API data.
const WORKER_CHECKPOINTS_DIR: &str = "agent_worker_checkpoints";
const WORKER_TASK_CHECKPOINTS_DIR: &str = "agent_worker_checkpoints/tasks";
const WORKER_ARTIFACT_CHECKPOINTS_DIR: &str = "agent_worker_checkpoints/artifacts";
const LEGACY_AGENT_TASKS_DIR: &str = "agent_tasks";
const LEGACY_ARTIFACTS_DIR: &str = "artifacts/articles";

fn validate_id(id: &str, label: &str) -> Result<(), String> {
    validate_storage_id(id).map_err(|error| format!("Invalid {label}: {error}"))
}

fn data_file_path(
    data_dir: &Path,
    relative_dir: &str,
    id: &str,
    label: &str,
) -> Result<PathBuf, String> {
    validate_id(id, label)?;
    Ok(data_dir.join(relative_dir).join(id))
}

fn data_json_file_path(
    data_dir: &Path,
    relative_dir: &str,
    id: &str,
    label: &str,
) -> Result<PathBuf, String> {
    validate_id(id, label)?;
    Ok(data_dir.join(relative_dir).join(format!("{id}.json")))
}

fn valid_file_id(file_name: String, suffix: Option<&str>) -> Option<String> {
    let id = match suffix {
        Some(suffix) => file_name.strip_suffix(suffix)?.to_string(),
        None => file_name,
    };
    validate_storage_id(&id).ok()?;
    Some(id)
}

pub fn get_app_data_dir(app_handle: &AppHandle) -> Result<PathBuf, String> {
    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))
}

pub fn ensure_app_dirs(app_handle: &AppHandle) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let articles_dir = data_dir.join(ARTICLES_DIR);
    let worker_checkpoints_dir = data_dir.join(WORKER_CHECKPOINTS_DIR);
    let worker_task_checkpoints_dir = data_dir.join(WORKER_TASK_CHECKPOINTS_DIR);
    let worker_artifact_checkpoints_dir = data_dir.join(WORKER_ARTIFACT_CHECKPOINTS_DIR);

    fs::create_dir_all(&articles_dir)
        .map_err(|e| format!("Failed to create articles directory: {}", e))?;
    fs::create_dir_all(&worker_checkpoints_dir)
        .map_err(|e| format!("Failed to create worker checkpoint directory: {}", e))?;
    fs::create_dir_all(&worker_task_checkpoints_dir)
        .map_err(|e| format!("Failed to create worker task checkpoint directory: {}", e))?;
    fs::create_dir_all(&worker_artifact_checkpoints_dir).map_err(|e| {
        format!(
            "Failed to create worker artifact checkpoint directory: {}",
            e
        )
    })?;

    Ok(())
}

pub fn save_config(app_handle: &AppHandle, config: &AppConfig) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let config_path = data_dir.join(CONFIG_FILE);

    let config_json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(config_path, config_json).map_err(|e| format!("Failed to write config: {}", e))?;

    Ok(())
}

pub fn load_config(app_handle: &AppHandle) -> Result<Option<AppConfig>, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let config_path = data_dir.join(CONFIG_FILE);

    if !config_path.exists() {
        return Ok(None);
    }

    let config_content =
        fs::read_to_string(config_path).map_err(|e| format!("Failed to read config: {}", e))?;

    let mut deserializer = serde_json::Deserializer::from_str(&config_content);
    let mut config: AppConfig = match serde::Deserialize::deserialize(&mut deserializer) {
        Ok(c) => c,
        Err(e) => {
            return Err(format!("FATAL_CONFIG_CORRUPTION: {}", e));
        }
    };

    config.prompt_features = merge_missing_builtin_prompt_features(config.prompt_features);

    Ok(Some(config))
}

pub fn save_article(app_handle: &AppHandle, article_id: &str, content: &str) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let article_path = data_file_path(&data_dir, ARTICLES_DIR, article_id, "article id")?;

    fs::write(article_path, content).map_err(|e| format!("Failed to save article: {}", e))?;

    Ok(())
}

pub fn load_article(app_handle: &AppHandle, article_id: &str) -> Result<String, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let article_path = data_file_path(&data_dir, ARTICLES_DIR, article_id, "article id")?;

    if !article_path.exists() {
        return Err("Article not found".to_string());
    }

    fs::read_to_string(article_path).map_err(|e| format!("Failed to read article: {}", e))
}

pub fn list_articles(app_handle: &AppHandle) -> Result<Vec<String>, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let articles_dir = data_dir.join(ARTICLES_DIR);

    if !articles_dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(articles_dir)
        .map_err(|e| format!("Failed to read articles directory: {}", e))?;

    let article_ids: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|file_name| valid_file_id(file_name, None))
        .collect();

    Ok(article_ids)
}

pub fn delete_article(app_handle: &AppHandle, article_id: &str) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let article_path = data_file_path(&data_dir, ARTICLES_DIR, article_id, "article id")?;

    if article_path.exists() {
        fs::remove_file(article_path).map_err(|e| format!("Failed to delete article: {}", e))?;
    }

    Ok(())
}

fn ensure_dir(path: &Path, name: &str) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("Failed to create {}: {}", name, e))
}

pub fn save_legacy_agent_task_in_dir(data_dir: &Path, task: &AgentTask) -> Result<(), String> {
    save_task_in_dir(data_dir, LEGACY_AGENT_TASKS_DIR, task, "legacy agent task")
}

pub fn save_worker_task_checkpoint_in_dir(data_dir: &Path, task: &AgentTask) -> Result<(), String> {
    save_task_in_dir(
        data_dir,
        WORKER_TASK_CHECKPOINTS_DIR,
        task,
        "worker task checkpoint",
    )
}

fn save_task_in_dir(
    data_dir: &Path,
    directory: &str,
    task: &AgentTask,
    label: &str,
) -> Result<(), String> {
    let dir = data_dir.join(directory);
    ensure_dir(&dir, "agent task directory")?;
    validate_id(&task.id, "agent task id")?;
    let content = serde_json::to_string(task)
        .map_err(|e| format!("Failed to serialize agent task: {}", e))?;
    fs::write(
        data_json_file_path(data_dir, directory, &task.id, "agent task id")?,
        content,
    )
    .map_err(|e| format!("Failed to save {label}: {}", e))?;
    Ok(())
}

pub fn load_legacy_agent_task_in_dir(data_dir: &Path, task_id: &str) -> Result<AgentTask, String> {
    load_task_in_dir(
        data_dir,
        LEGACY_AGENT_TASKS_DIR,
        task_id,
        "legacy agent task",
    )
}

pub fn load_worker_task_checkpoint_in_dir(
    data_dir: &Path,
    task_id: &str,
) -> Result<AgentTask, String> {
    load_task_in_dir(
        data_dir,
        WORKER_TASK_CHECKPOINTS_DIR,
        task_id,
        "worker task checkpoint",
    )
}

fn load_task_in_dir(
    data_dir: &Path,
    directory: &str,
    task_id: &str,
    label: &str,
) -> Result<AgentTask, String> {
    let path = data_json_file_path(data_dir, directory, task_id, "agent task id")?;
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read {label}: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse {label}: {}", e))
}

pub fn list_legacy_agent_tasks_in_dir(data_dir: &Path) -> Result<Vec<String>, String> {
    list_tasks_in_dir(data_dir, LEGACY_AGENT_TASKS_DIR, "legacy agent task")
}

pub fn list_worker_task_checkpoints_in_dir(data_dir: &Path) -> Result<Vec<String>, String> {
    list_tasks_in_dir(
        data_dir,
        WORKER_TASK_CHECKPOINTS_DIR,
        "worker task checkpoint",
    )
}

pub fn persist_worker_task_checkpoint_after_backend(
    data_dir: &Path,
    backend_result: Result<AgentTask, String>,
) -> Result<AgentTask, String> {
    let task = backend_result?;
    save_worker_task_checkpoint_in_dir(data_dir, &task)?;
    Ok(task)
}

fn list_tasks_in_dir(data_dir: &Path, directory: &str, label: &str) -> Result<Vec<String>, String> {
    let dir = data_dir.join(directory);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to read {label} directory: {}", e))?;
    let mut ids: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|file_name| valid_file_id(file_name, Some(".json")))
        .collect();
    ids.sort();
    Ok(ids)
}

pub fn save_legacy_agent_task(app_handle: &AppHandle, task: &AgentTask) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    save_legacy_agent_task_in_dir(&data_dir, task)
}

pub fn load_legacy_agent_task(app_handle: &AppHandle, task_id: &str) -> Result<AgentTask, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    load_legacy_agent_task_in_dir(&data_dir, task_id)
}

pub fn save_legacy_artifact_in_dir(data_dir: &Path, artifact: &Artifact) -> Result<(), String> {
    save_artifact_in_directory(data_dir, LEGACY_ARTIFACTS_DIR, artifact, "legacy artifact")
}

pub fn save_worker_artifact_checkpoint_in_dir(
    data_dir: &Path,
    artifact: &Artifact,
) -> Result<(), String> {
    save_artifact_in_directory(
        data_dir,
        WORKER_ARTIFACT_CHECKPOINTS_DIR,
        artifact,
        "worker artifact checkpoint",
    )
}

fn save_artifact_in_directory(
    data_dir: &Path,
    directory: &str,
    artifact: &Artifact,
    label: &str,
) -> Result<(), String> {
    validate_id(&artifact.article_id, "artifact article id")?;
    validate_id(&artifact.id, "artifact id")?;
    let dir = data_dir.join(directory).join(&artifact.article_id);
    ensure_dir(&dir, "artifact directory")?;
    let content = serde_json::to_string(artifact)
        .map_err(|e| format!("Failed to serialize artifact: {}", e))?;
    fs::write(dir.join(format!("{}.json", artifact.id)), content)
        .map_err(|e| format!("Failed to save {label}: {}", e))?;
    Ok(())
}

pub fn load_legacy_artifact_in_dir(
    data_dir: &Path,
    article_id: &str,
    artifact_id: &str,
) -> Result<Artifact, String> {
    load_artifact_in_directory(
        data_dir,
        LEGACY_ARTIFACTS_DIR,
        article_id,
        artifact_id,
        "legacy artifact",
    )
}

pub fn load_worker_artifact_checkpoint_in_dir(
    data_dir: &Path,
    article_id: &str,
    artifact_id: &str,
) -> Result<Artifact, String> {
    load_artifact_in_directory(
        data_dir,
        WORKER_ARTIFACT_CHECKPOINTS_DIR,
        article_id,
        artifact_id,
        "worker artifact checkpoint",
    )
}

pub fn persist_worker_artifact_checkpoint_after_backend(
    data_dir: &Path,
    backend_result: Result<Artifact, String>,
) -> Result<Artifact, String> {
    let artifact = backend_result?;
    save_worker_artifact_checkpoint_in_dir(data_dir, &artifact)?;
    Ok(artifact)
}

fn load_artifact_in_directory(
    data_dir: &Path,
    directory: &str,
    article_id: &str,
    artifact_id: &str,
    label: &str,
) -> Result<Artifact, String> {
    validate_id(article_id, "artifact article id")?;
    validate_id(artifact_id, "artifact id")?;
    let path = data_dir
        .join(directory)
        .join(article_id)
        .join(format!("{}.json", artifact_id));
    let content = fs::read_to_string(path).map_err(|e| format!("Failed to read {label}: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse {label}: {}", e))
}

pub fn save_legacy_artifact(app_handle: &AppHandle, artifact: &Artifact) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    save_legacy_artifact_in_dir(&data_dir, artifact)
}

pub fn load_legacy_artifact(
    app_handle: &AppHandle,
    article_id: &str,
    artifact_id: &str,
) -> Result<Artifact, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    load_legacy_artifact_in_dir(&data_dir, article_id, artifact_id)
}

pub fn update_article_active_mind_map_artifact_in_dir(
    data_dir: &Path,
    article_id: &str,
    artifact_id: Option<String>,
) -> Result<Article, String> {
    let path = data_file_path(data_dir, ARTICLES_DIR, article_id, "article id")?;
    if let Some(artifact_id) = artifact_id.as_deref() {
        validate_id(artifact_id, "artifact id")?;
    }
    let content =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read article: {}", e))?;
    let mut article: Article =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse article: {}", e))?;
    article.active_mind_map_artifact_id = artifact_id;
    let updated = serde_json::to_string(&article)
        .map_err(|e| format!("Failed to serialize article: {}", e))?;
    fs::write(path, updated).map_err(|e| format!("Failed to save article: {}", e))?;
    Ok(article)
}

pub fn update_article_active_mind_map_artifact(
    app_handle: &AppHandle,
    article_id: &str,
    artifact_id: Option<String>,
) -> Result<Article, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    update_article_active_mind_map_artifact_in_dir(&data_dir, article_id, artifact_id)
}

// ============================================================================
// Favorites Storage - 独立于文章存储，删除文章不会影响收藏
// ============================================================================

const FAVORITES_VOCAB_DIR: &str = "favorites/vocabulary";
const FAVORITES_GRAMMAR_DIR: &str = "favorites/grammar";
const FAVORITES_PACKS_DIR: &str = "favorites/packs";

/// 确保收藏夹目录存在
pub fn ensure_favorites_dirs(app_handle: &AppHandle) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let vocab_dir = data_dir.join(FAVORITES_VOCAB_DIR);
    let grammar_dir = data_dir.join(FAVORITES_GRAMMAR_DIR);
    let packs_dir = data_dir.join(FAVORITES_PACKS_DIR);

    fs::create_dir_all(&vocab_dir)
        .map_err(|e| format!("Failed to create vocabulary favorites directory: {}", e))?;
    fs::create_dir_all(&grammar_dir)
        .map_err(|e| format!("Failed to create grammar favorites directory: {}", e))?;
    fs::create_dir_all(&packs_dir)
        .map_err(|e| format!("Failed to create word packs directory: {}", e))?;

    Ok(())
}

/// 保存单词收藏
pub fn save_favorite_vocabulary(
    app_handle: &AppHandle,
    id: &str,
    content: &str,
) -> Result<(), String> {
    ensure_favorites_dirs(app_handle)?;
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_VOCAB_DIR, id, "vocabulary favorite id")?;

    fs::write(path, content).map_err(|e| format!("Failed to save vocabulary favorite: {}", e))?;

    Ok(())
}

/// 加载单词收藏
pub fn load_favorite_vocabulary(app_handle: &AppHandle, id: &str) -> Result<String, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_VOCAB_DIR, id, "vocabulary favorite id")?;

    if !path.exists() {
        return Err("Vocabulary favorite not found".to_string());
    }

    fs::read_to_string(path).map_err(|e| format!("Failed to read vocabulary favorite: {}", e))
}

/// 列出所有单词收藏ID
pub fn list_favorite_vocabularies(app_handle: &AppHandle) -> Result<Vec<String>, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let dir = data_dir.join(FAVORITES_VOCAB_DIR);

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read vocabulary favorites directory: {}", e))?;

    let ids: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|file_name| valid_file_id(file_name, None))
        .collect();

    Ok(ids)
}

/// 删除单词收藏
pub fn delete_favorite_vocabulary(app_handle: &AppHandle, id: &str) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_VOCAB_DIR, id, "vocabulary favorite id")?;

    if path.exists() {
        fs::remove_file(path)
            .map_err(|e| format!("Failed to delete vocabulary favorite: {}", e))?;
    }

    Ok(())
}

/// 保存语法收藏
pub fn save_favorite_grammar(
    app_handle: &AppHandle,
    id: &str,
    content: &str,
) -> Result<(), String> {
    ensure_favorites_dirs(app_handle)?;
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_GRAMMAR_DIR, id, "grammar favorite id")?;

    fs::write(path, content).map_err(|e| format!("Failed to save grammar favorite: {}", e))?;

    Ok(())
}

/// 加载语法收藏
pub fn load_favorite_grammar(app_handle: &AppHandle, id: &str) -> Result<String, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_GRAMMAR_DIR, id, "grammar favorite id")?;

    if !path.exists() {
        return Err("Grammar favorite not found".to_string());
    }

    fs::read_to_string(path).map_err(|e| format!("Failed to read grammar favorite: {}", e))
}

/// 列出所有语法收藏ID
pub fn list_favorite_grammars(app_handle: &AppHandle) -> Result<Vec<String>, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let dir = data_dir.join(FAVORITES_GRAMMAR_DIR);

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read grammar favorites directory: {}", e))?;

    let ids: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|file_name| valid_file_id(file_name, None))
        .collect();

    Ok(ids)
}

/// 删除语法收藏
pub fn delete_favorite_grammar(app_handle: &AppHandle, id: &str) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_GRAMMAR_DIR, id, "grammar favorite id")?;

    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("Failed to delete grammar favorite: {}", e))?;
    }

    Ok(())
}

/// 保存单词包
pub fn save_word_pack(app_handle: &AppHandle, id: &str, content: &str) -> Result<(), String> {
    ensure_favorites_dirs(app_handle)?;
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_PACKS_DIR, id, "word pack id")?;
    fs::write(path, content).map_err(|e| format!("Failed to save word pack: {}", e))?;
    Ok(())
}

/// 加载单词包
pub fn load_word_pack(app_handle: &AppHandle, id: &str) -> Result<String, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_PACKS_DIR, id, "word pack id")?;

    if !path.exists() {
        return Err("Word pack not found".to_string());
    }

    fs::read_to_string(path).map_err(|e| format!("Failed to read word pack: {}", e))
}

/// 列出所有单词包ID
pub fn list_word_packs(app_handle: &AppHandle) -> Result<Vec<String>, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let dir = data_dir.join(FAVORITES_PACKS_DIR);

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to read word packs directory: {}", e))?;

    let ids: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|file_name| valid_file_id(file_name, None))
        .collect();

    Ok(ids)
}

/// 删除单词包
pub fn delete_word_pack(app_handle: &AppHandle, id: &str) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, FAVORITES_PACKS_DIR, id, "word pack id")?;

    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("Failed to delete word pack: {}", e))?;
    }

    Ok(())
}

// ============================================================================
// Bookmarks Storage - 书签存储
// ============================================================================

const BOOKMARKS_DIR: &str = "bookmarks";

/// 确保书签目录存在
pub fn ensure_bookmarks_dir(app_handle: &AppHandle) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let bookmarks_dir = data_dir.join(BOOKMARKS_DIR);

    fs::create_dir_all(&bookmarks_dir)
        .map_err(|e| format!("Failed to create bookmarks directory: {}", e))?;

    Ok(())
}

/// 保存书签
pub fn save_bookmark(app_handle: &AppHandle, id: &str, content: &str) -> Result<(), String> {
    ensure_bookmarks_dir(app_handle)?;
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, BOOKMARKS_DIR, id, "bookmark id")?;

    fs::write(path, content).map_err(|e| format!("Failed to save bookmark: {}", e))?;

    Ok(())
}

/// 加载书签
pub fn load_bookmark(app_handle: &AppHandle, id: &str) -> Result<String, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, BOOKMARKS_DIR, id, "bookmark id")?;

    if !path.exists() {
        return Err("Bookmark not found".to_string());
    }

    fs::read_to_string(path).map_err(|e| format!("Failed to read bookmark: {}", e))
}

/// 列出所有书签ID
pub fn list_bookmarks(app_handle: &AppHandle) -> Result<Vec<String>, String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let dir = data_dir.join(BOOKMARKS_DIR);

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries =
        fs::read_dir(dir).map_err(|e| format!("Failed to read bookmarks directory: {}", e))?;

    let ids: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|file_name| valid_file_id(file_name, None))
        .collect();

    Ok(ids)
}

/// 删除书签
pub fn delete_bookmark(app_handle: &AppHandle, id: &str) -> Result<(), String> {
    let data_dir = get_app_data_dir(app_handle)?;
    let path = data_file_path(&data_dir, BOOKMARKS_DIR, id, "bookmark id")?;

    if path.exists() {
        fs::remove_file(path).map_err(|e| format!("Failed to delete bookmark: {}", e))?;
    }

    Ok(())
}

/// 列出指定书籍的所有书签
pub fn list_bookmarks_for_book(
    app_handle: &AppHandle,
    book_path: &str,
) -> Result<Vec<String>, String> {
    let all_ids = list_bookmarks(app_handle)?;
    let mut matching_ids = Vec::new();

    for id in all_ids {
        if let Ok(json) = load_bookmark(app_handle, &id) {
            // 简单检查 JSON 中是否包含 book_path
            // 更准确的方法是反序列化，但这里为了性能使用字符串匹配
            if json.contains(book_path) {
                matching_ids.push(id);
            }
        }
    }

    Ok(matching_ids)
}
