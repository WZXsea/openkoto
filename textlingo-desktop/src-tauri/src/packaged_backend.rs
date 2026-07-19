use std::{
    ffi::OsString,
    fs,
    io::{BufRead, BufReader},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    sync::Mutex,
    time::Duration,
};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{
    data_backup::{create_upgrade_backup_if_needed, set_private_file_permissions},
    logging::{LogLevel, LogStore},
    storage::{load_config, save_config},
};

const AUTOSTART_ENV: &str = "OPENKOTO_DESKTOP_AUTOSTART_BACKEND";
const BACKEND_BINARY_PATH_ENV: &str = "OPENKOTO_BACKEND_BINARY_PATH";
const PACKAGED_DATABASE_URL_ENV: &str = "OPENKOTO_PACKAGED_DATABASE_URL";
const DEFAULT_DATABASE_URL: &str =
    "postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev";
const PACKAGED_BACKEND_BIND: &str = "127.0.0.1:19421";
const MANAGED_DATABASE_NAME: &str = "openkoto";
const MANAGED_DATABASE_USER: &str = "openkoto";
const BACKEND_PID_FILE: &str = "backend.pid";
const BACKEND_SHA256_FILE: &str = "backend.sha256";
const POSTGRES_PID_FILE: &str = "postgres.pid";

#[derive(Default)]
pub struct PackagedBackendManager {
    child: Mutex<Option<Child>>,
    postgres_child: Mutex<Option<Child>>,
    status: Mutex<PackagedBackendStatus>,
}

impl Drop for PackagedBackendManager {
    fn drop(&mut self) {
        stop_child(&self.child);
        stop_child(&self.postgres_child);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct PackagedBackendStatus {
    pub enabled: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub backend_url: Option<String>,
    pub backend_binary: Option<String>,
    pub database_url_source: Option<String>,
    pub postgres_running: bool,
    pub postgres_pid: Option<u32>,
    pub postgres_runtime: Option<String>,
    pub message: Option<String>,
}

impl Default for PackagedBackendStatus {
    fn default() -> Self {
        Self {
            enabled: should_autostart_backend(),
            running: false,
            pid: None,
            backend_url: None,
            backend_binary: None,
            database_url_source: None,
            postgres_running: false,
            postgres_pid: None,
            postgres_runtime: None,
            message: None,
        }
    }
}

#[tauri::command]
pub async fn packaged_backend_status_cmd(
    app_handle: AppHandle,
) -> Result<PackagedBackendStatus, String> {
    Ok(app_handle
        .state::<PackagedBackendManager>()
        .status
        .lock()
        .map_err(|_| "packaged backend status lock poisoned".to_string())?
        .clone())
}

pub async fn start_packaged_backend_if_enabled(app_handle: AppHandle) {
    if !should_autostart_backend() {
        update_status(
            &app_handle,
            PackagedBackendStatus {
                enabled: false,
                message: Some("packaged backend autostart disabled".to_string()),
                ..PackagedBackendStatus::default()
            },
        );
        return;
    }

    if let Ok(Some(config)) = load_config(&app_handle) {
        if let Some(backend_url) = config
            .backend_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .filter(|value| !is_packaged_local_backend_url(value))
        {
            update_status(
                &app_handle,
                PackagedBackendStatus {
                    enabled: false,
                    backend_url: Some(backend_url.to_string()),
                    message: Some("external backend is configured".to_string()),
                    ..PackagedBackendStatus::default()
                },
            );
            return;
        }
    }

    match start_packaged_backend(app_handle.clone()).await {
        Ok(status) => {
            update_status(&app_handle, status.clone());
            LogStore::global().push(
                LogLevel::Info,
                "backend",
                format!(
                    "packaged backend started at {}",
                    status.backend_url.unwrap_or_default()
                ),
            );
        }
        Err(error) => {
            update_status(
                &app_handle,
                PackagedBackendStatus {
                    enabled: true,
                    running: false,
                    message: Some(error.clone()),
                    ..PackagedBackendStatus::default()
                },
            );
            LogStore::global().push(
                LogLevel::Warn,
                "backend",
                format!("packaged backend autostart failed: {error}"),
            );
        }
    }
}

async fn start_packaged_backend(app_handle: AppHandle) -> Result<PackagedBackendStatus, String> {
    let manager = app_handle.state::<PackagedBackendManager>();
    if let Some(existing) = manager
        .child
        .lock()
        .map_err(|_| "packaged backend process lock poisoned".to_string())?
        .as_ref()
    {
        let status = manager
            .status
            .lock()
            .map_err(|_| "packaged backend status lock poisoned".to_string())?
            .clone();
        return Ok(PackagedBackendStatus {
            running: true,
            pid: Some(existing.id()),
            ..status
        });
    }

    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve app data dir: {error}"))?;
    let backend_dir = app_data_dir.join("backend");
    let files_dir = backend_dir.join("files");
    fs::create_dir_all(&files_dir)
        .map_err(|error| format!("failed to create backend data directories: {error}"))?;

    let backend_binary = resolve_backend_binary(&app_handle)?;
    let backend_sha256 = file_sha256(&backend_binary)?;
    let backend_url = packaged_backend_url();
    let recorded_sha256 = fs::read_to_string(backend_dir.join(BACKEND_SHA256_FILE))
        .ok()
        .map(|value| value.trim().to_string());
    if recorded_sha256.as_deref() == Some(backend_sha256.as_str())
        && backend_health_has_expected_version(&backend_url).await
    {
        persist_backend_url(&app_handle, &backend_url)?;
        let backend_pid = read_pid_file(&backend_dir.join(BACKEND_PID_FILE));
        let postgres_pid = read_pid_file(&backend_dir.join(POSTGRES_PID_FILE));
        return Ok(PackagedBackendStatus {
            enabled: true,
            running: true,
            pid: backend_pid,
            backend_url: Some(backend_url),
            backend_binary: Some(backend_binary.to_string_lossy().to_string()),
            database_url_source: Some("existing-packaged-runtime".to_string()),
            postgres_running: postgres_pid.is_some_and(is_pid_running),
            postgres_pid,
            postgres_runtime: None,
            message: Some("existing packaged backend is healthy".to_string()),
        });
    }
    cleanup_recorded_runtime_processes(&backend_dir, &backend_binary)?;
    ensure_packaged_backend_port_available()?;
    let upgrade_backup_dir = create_upgrade_backup_if_needed(
        &app_data_dir,
        &backend_dir,
        recorded_sha256.as_deref(),
        &backend_sha256,
    )
    .map_err(|error| {
        upgrade_recovery_diagnostic(
            &format!("pre-upgrade backup validation failed: {error}"),
            None,
            &backend_dir,
        )
    })?;
    if let Some(backup_dir) = upgrade_backup_dir.as_deref() {
        LogStore::global().push(
            LogLevel::Info,
            "backup",
            format!(
                "created validated pre-upgrade backup at {}",
                backup_dir.display()
            ),
        );
    }
    let secret = read_or_create_secret(&backend_dir.join("jwt_secret")).map_err(|error| {
        upgrade_recovery_diagnostic(&error, upgrade_backup_dir.as_deref(), &backend_dir)
    })?;
    let bind_addr = PACKAGED_BACKEND_BIND.to_string();
    let database_runtime = prepare_database_runtime(&app_handle, &backend_dir)
        .await
        .map_err(|error| {
            upgrade_recovery_diagnostic(&error, upgrade_backup_dir.as_deref(), &backend_dir)
        })?;

    let mut command = Command::new(&backend_binary);
    command
        .env("DATABASE_URL", &database_runtime.url)
        .env("OPENKOTO_BACKEND_BIND", &bind_addr)
        .env("OPENKOTO_JWT_SECRET", &secret)
        .env("OPENKOTO_FILE_STORAGE_DIR", &files_dir)
        .env(
            "RUST_LOG",
            std::env::var("OPENKOTO_BACKEND_RUST_LOG")
                .unwrap_or_else(|_| "openkoto_backend=info,tower_http=info".to_string()),
        )
        .current_dir(&backend_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            if database_runtime.managed_postgres {
                stop_packaged_postgres_process(&manager);
            }
            return Err(upgrade_recovery_diagnostic(
                &format!("failed to spawn packaged backend: {error}"),
                upgrade_backup_dir.as_deref(),
                &backend_dir,
            ));
        }
    };
    let pid = child.id();
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(LogLevel::Info, stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(LogLevel::Warn, stderr);
    }

    manager
        .child
        .lock()
        .map_err(|_| "packaged backend process lock poisoned".to_string())?
        .replace(child);
    write_pid_file(&backend_dir.join(BACKEND_PID_FILE), pid);

    if let Err(error) = wait_for_backend_health_or_exit(&manager, &backend_url).await {
        stop_packaged_backend_process(&manager);
        if database_runtime.managed_postgres {
            stop_packaged_postgres_process(&manager);
        }
        return Err(upgrade_recovery_diagnostic(
            &error,
            upgrade_backup_dir.as_deref(),
            &backend_dir,
        ));
    }
    if let Err(error) = persist_backend_url(&app_handle, &backend_url) {
        stop_packaged_backend_process(&manager);
        if database_runtime.managed_postgres {
            stop_packaged_postgres_process(&manager);
        }
        return Err(upgrade_recovery_diagnostic(
            &format!("failed to persist packaged backend URL: {error}"),
            upgrade_backup_dir.as_deref(),
            &backend_dir,
        ));
    }
    if let Err(error) = write_atomic_text(&backend_dir.join(BACKEND_SHA256_FILE), &backend_sha256) {
        stop_packaged_backend_process(&manager);
        if database_runtime.managed_postgres {
            stop_packaged_postgres_process(&manager);
        }
        return Err(upgrade_recovery_diagnostic(
            &format!("failed to persist packaged backend fingerprint: {error}"),
            upgrade_backup_dir.as_deref(),
            &backend_dir,
        ));
    }

    Ok(PackagedBackendStatus {
        enabled: true,
        running: true,
        pid: Some(pid),
        backend_url: Some(backend_url),
        backend_binary: Some(backend_binary.to_string_lossy().to_string()),
        database_url_source: Some(database_runtime.source),
        postgres_running: database_runtime.managed_postgres,
        postgres_pid: database_runtime.postgres_pid,
        postgres_runtime: database_runtime.postgres_runtime,
        message: Some("packaged backend is healthy".to_string()),
    })
}

#[derive(Debug)]
struct DatabaseRuntime {
    url: String,
    source: String,
    managed_postgres: bool,
    postgres_pid: Option<u32>,
    postgres_runtime: Option<String>,
}

#[derive(Debug, Clone)]
struct BundledPostgresRuntime {
    root: PathBuf,
    bin_dir: PathBuf,
    lib_dir: PathBuf,
    pkglib_dir: PathBuf,
    share_dir: PathBuf,
}

impl BundledPostgresRuntime {
    fn postgres_binary(&self) -> PathBuf {
        self.bin_dir.join(binary_name("postgres"))
    }

    fn initdb_binary(&self) -> PathBuf {
        self.bin_dir.join(binary_name("initdb"))
    }

    fn createdb_binary(&self) -> PathBuf {
        self.bin_dir.join(binary_name("createdb"))
    }
}

async fn prepare_database_runtime(
    app_handle: &AppHandle,
    backend_dir: &Path,
) -> Result<DatabaseRuntime, String> {
    if let Some((url, source)) = resolve_explicit_database_url() {
        return Ok(DatabaseRuntime {
            url,
            source,
            managed_postgres: false,
            postgres_pid: None,
            postgres_runtime: None,
        });
    }

    if let Some(runtime) = resolve_bundled_postgres_runtime(app_handle)? {
        return start_bundled_postgres(app_handle, backend_dir, runtime).await;
    }

    if cfg!(debug_assertions) {
        return Ok(DatabaseRuntime {
            url: DEFAULT_DATABASE_URL.to_string(),
            source: "default".to_string(),
            managed_postgres: false,
            postgres_pid: None,
            postgres_runtime: None,
        });
    }

    Err("bundled PostgreSQL runtime not found and no DATABASE_URL was configured".to_string())
}

async fn start_bundled_postgres(
    app_handle: &AppHandle,
    backend_dir: &Path,
    runtime: BundledPostgresRuntime,
) -> Result<DatabaseRuntime, String> {
    let manager = app_handle.state::<PackagedBackendManager>();
    let data_dir = backend_dir.join("postgres-data");
    let socket_dir = backend_dir.join("postgres-socket");
    fs::create_dir_all(&socket_dir)
        .map_err(|error| format!("failed to create PostgreSQL socket directory: {error}"))?;

    match inspect_postgres_data_dir(&data_dir)? {
        PostgresDataDirState::Missing => initialize_postgres_data_dir(&runtime, &data_dir)?,
        PostgresDataDirState::Ready => {}
    }

    let port = find_free_loopback_port()?;
    let mut command = Command::new(runtime.postgres_binary());
    apply_postgres_env(&mut command, &runtime);
    command
        .arg("-D")
        .arg(&data_dir)
        .arg("-h")
        .arg("127.0.0.1")
        .arg("-p")
        .arg(port.to_string())
        .arg("-k")
        .arg(&socket_dir)
        .arg("-c")
        .arg("listen_addresses=127.0.0.1")
        .arg("-c")
        .arg("shared_buffers=32MB")
        .arg("-c")
        .arg("max_connections=20")
        .arg("-c")
        .arg("logging_collector=off")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to spawn bundled PostgreSQL: {error}"))?;
    let postgres_pid = child.id();
    if let Some(stdout) = child.stdout.take() {
        spawn_log_reader(LogLevel::Info, stdout);
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_log_reader(LogLevel::Warn, stderr);
    }

    manager
        .postgres_child
        .lock()
        .map_err(|_| "packaged PostgreSQL process lock poisoned".to_string())?
        .replace(child);
    write_pid_file(&backend_dir.join(POSTGRES_PID_FILE), postgres_pid);

    if let Err(error) = ensure_bundled_database(&runtime, port).await {
        stop_packaged_postgres_process(&manager);
        return Err(error);
    }

    Ok(DatabaseRuntime {
        url: format!(
            "postgres://{MANAGED_DATABASE_USER}@127.0.0.1:{port}/{MANAGED_DATABASE_NAME}?sslmode=disable"
        ),
        source: "bundled-postgresql".to_string(),
        managed_postgres: true,
        postgres_pid: Some(postgres_pid),
        postgres_runtime: Some(runtime.root.to_string_lossy().to_string()),
    })
}

fn run_initdb(runtime: &BundledPostgresRuntime, data_dir: &Path) -> Result<(), String> {
    if let Some(parent) = data_dir.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create PostgreSQL data parent: {error}"))?;
    }

    let mut command = Command::new(runtime.initdb_binary());
    apply_postgres_env(&mut command, runtime);
    let output = command
        .arg("-D")
        .arg(data_dir)
        .arg("-L")
        .arg(&runtime.share_dir)
        .arg("--username")
        .arg(MANAGED_DATABASE_USER)
        .arg("--auth-local=trust")
        .arg("--auth-host=trust")
        .output()
        .map_err(|error| format!("failed to run bundled initdb: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    Err(format!(
        "bundled initdb failed: {}",
        format_command_output(&output)
    ))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PostgresDataDirState {
    Missing,
    Ready,
}

fn inspect_postgres_data_dir(data_dir: &Path) -> Result<PostgresDataDirState, String> {
    if !data_dir.exists() {
        return Ok(PostgresDataDirState::Missing);
    }
    if !data_dir.is_dir() {
        return Err(format!(
            "packaged PostgreSQL data path is not a directory: {}",
            data_dir.display()
        ));
    }
    let version_path = data_dir.join("PG_VERSION");
    let version = fs::read_to_string(&version_path).map_err(|error| {
        format!(
            "packaged PostgreSQL data directory is incomplete; PG_VERSION cannot be read and existing data will not be overwritten: {error}"
        )
    })?;
    if version.trim().parse::<u32>().is_err()
        || !data_dir.join("base").is_dir()
        || !data_dir.join("global").is_dir()
        || !data_dir.join("postgresql.conf").is_file()
    {
        return Err(
            "packaged PostgreSQL data directory is incomplete; refusing to initialize or overwrite existing data"
                .to_string(),
        );
    }
    Ok(PostgresDataDirState::Ready)
}

fn initialize_postgres_data_dir(
    runtime: &BundledPostgresRuntime,
    data_dir: &Path,
) -> Result<(), String> {
    let parent = data_dir
        .parent()
        .ok_or_else(|| "packaged PostgreSQL data directory has no parent".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create PostgreSQL data parent: {error}"))?;
    let staging_dir = parent.join(format!(".postgres-data-init-{}", Uuid::new_v4().simple()));
    let result = (|| {
        run_initdb(runtime, &staging_dir)?;
        fs::rename(&staging_dir, data_dir)
            .map_err(|error| format!("failed to commit initialized PostgreSQL data: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    result
}

async fn ensure_bundled_database(
    runtime: &BundledPostgresRuntime,
    port: u16,
) -> Result<(), String> {
    let mut last_error = String::new();

    for _ in 0..80 {
        let mut command = Command::new(runtime.createdb_binary());
        apply_postgres_env(&mut command, runtime);
        let output = command
            .arg("-h")
            .arg("127.0.0.1")
            .arg("-p")
            .arg(port.to_string())
            .arg("-U")
            .arg(MANAGED_DATABASE_USER)
            .arg(MANAGED_DATABASE_NAME)
            .output()
            .map_err(|error| format!("failed to run bundled createdb: {error}"))?;

        if output.status.success() {
            return Ok(());
        }

        let details = format_command_output(&output);
        if details.contains("already exists") {
            return Ok(());
        }
        last_error = details;
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    Err(format!(
        "bundled PostgreSQL did not become ready for database creation: {last_error}"
    ))
}

fn resolve_bundled_postgres_runtime(
    app_handle: &AppHandle,
) -> Result<Option<BundledPostgresRuntime>, String> {
    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|error| format!("failed to resolve resource dir: {error}"))?;
    let root = resource_dir.join("postgres");
    let Some(share_dir) = resolve_bundled_postgres_share_dir(&root)? else {
        return Ok(None);
    };
    let runtime = BundledPostgresRuntime {
        bin_dir: root.join("bin"),
        lib_dir: root.join("lib"),
        pkglib_dir: root.join("lib").join("postgresql"),
        share_dir,
        root,
    };

    if runtime.postgres_binary().is_file()
        && runtime.initdb_binary().is_file()
        && runtime.createdb_binary().is_file()
    {
        Ok(Some(runtime))
    } else {
        Ok(None)
    }
}

fn resolve_bundled_postgres_share_dir(root: &Path) -> Result<Option<PathBuf>, String> {
    let share_root = root.join("share");
    if !share_root.is_dir() {
        return Ok(None);
    }

    let entries = fs::read_dir(&share_root).map_err(|error| {
        format!(
            "failed to inspect bundled PostgreSQL share directory {}: {error}",
            share_root.display()
        )
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("failed to inspect bundled PostgreSQL share entry: {error}"))?
            .path();
        if path.join("postgres.bki").is_file() && path.join("postgresql.conf.sample").is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn update_status(app_handle: &AppHandle, status: PackagedBackendStatus) {
    if let Ok(mut guard) = app_handle.state::<PackagedBackendManager>().status.lock() {
        *guard = status;
    }
}

fn persist_backend_url(app_handle: &AppHandle, backend_url: &str) -> Result<(), String> {
    let mut config = load_config(app_handle)?.unwrap_or_default();
    if should_persist_packaged_backend_url(config.backend_url.as_deref(), backend_url) {
        config.backend_url = Some(backend_url.to_string());
        save_config(app_handle, &config)?;
    }
    Ok(())
}

fn should_persist_packaged_backend_url(current: Option<&str>, packaged_url: &str) -> bool {
    let Some(current) = current.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    if current == packaged_url {
        return false;
    }

    is_packaged_local_backend_url(current)
}

fn is_packaged_local_backend_url(value: &str) -> bool {
    value.starts_with("http://127.0.0.1:") || value.starts_with("http://localhost:")
}

fn packaged_backend_url() -> String {
    format!("http://{PACKAGED_BACKEND_BIND}")
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("failed to read packaged backend for fingerprinting: {error}"))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn write_atomic_text(path: &Path, value: &str) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("path has no parent: {}", path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    let staging = parent.join(format!(".fingerprint-{}.tmp", Uuid::new_v4().simple()));
    let result = (|| {
        fs::write(&staging, value)
            .map_err(|error| format!("failed to write {}: {error}", staging.display()))?;
        set_private_file_permissions(&staging)?;
        fs::rename(&staging, path)
            .map_err(|error| format!("failed to replace {}: {error}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staging);
    }
    result
}

fn upgrade_recovery_diagnostic(
    error: &str,
    backup_dir: Option<&Path>,
    backend_dir: &Path,
) -> String {
    let backup = backup_dir.map_or_else(
        || "No validated pre-upgrade backup was committed by this attempt; preserve the current data directory and diagnose before retrying.".to_string(),
        |path| format!("Validated pre-upgrade backup: {}.", path.display()),
    );
    format!(
        "{error}. Existing application data was not deleted and the new backend fingerprint was not committed. {backup} Keep the application stopped before recovery; inspect Desktop logs and validate/dry-restore the backup before replacing {}.",
        backend_dir.display()
    )
}

fn write_pid_file(path: &Path, pid: u32) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, pid.to_string());
}

fn read_pid_file(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(unix)]
fn process_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(not(unix))]
fn process_command(_pid: u32) -> Option<String> {
    None
}

fn command_matches_binary(command: &str, binary: &Path, required_argument: Option<&Path>) -> bool {
    command_starts_with_executable(command, binary.to_string_lossy().as_ref())
        && required_argument
            .is_none_or(|path| has_bounded_text(command, path.to_string_lossy().as_ref()))
}

fn command_starts_with_executable(command: &str, executable: &str) -> bool {
    command
        .strip_prefix(executable)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
}

fn has_bounded_text(value: &str, expected: &str) -> bool {
    value.match_indices(expected).any(|(start, _)| {
        let before = value[..start].chars().next_back();
        let after = value[start + expected.len()..].chars().next();
        before.is_none_or(char::is_whitespace) && after.is_none_or(char::is_whitespace)
    })
}

fn cleanup_recorded_process(
    pid_file: &Path,
    binary: &Path,
    required_argument: Option<&Path>,
    label: &str,
) -> Result<(), String> {
    let Some(pid) = read_pid_file(pid_file) else {
        return Ok(());
    };
    if is_pid_running(pid) {
        let command = process_command(pid).ok_or_else(|| {
            format!("cannot inspect recorded {label} process {pid}; refusing to stop it")
        })?;
        if !command_matches_binary(&command, binary, required_argument) {
            return Err(format!(
                "recorded {label} PID {pid} does not match the packaged runtime; refusing to stop it"
            ));
        }
        terminate_pid(pid);
        if is_pid_running(pid) {
            return Err(format!("recorded {label} process {pid} did not stop"));
        }
    }
    let _ = fs::remove_file(pid_file);
    Ok(())
}

fn cleanup_recorded_runtime_processes(
    backend_dir: &Path,
    backend_binary: &Path,
) -> Result<(), String> {
    cleanup_recorded_process(
        &backend_dir.join(BACKEND_PID_FILE),
        backend_binary,
        None,
        "backend",
    )?;
    let postgres_binary = backend_binary
        .parent()
        .and_then(Path::parent)
        .map(|resources| {
            resources
                .join("postgres")
                .join("bin")
                .join(binary_name("postgres"))
        })
        .ok_or_else(|| "failed to resolve packaged PostgreSQL binary path".to_string())?;
    cleanup_recorded_process(
        &backend_dir.join(POSTGRES_PID_FILE),
        &postgres_binary,
        Some(&backend_dir.join("postgres-data")),
        "PostgreSQL",
    )
}

fn ensure_packaged_backend_port_available() -> Result<(), String> {
    TcpListener::bind(PACKAGED_BACKEND_BIND)
        .map(|listener| drop(listener))
        .map_err(|error| {
            let occupant = packaged_backend_port_occupant()
                .map(|value| format!(" Detected listener: {value}."))
                .unwrap_or_default();
            format!(
                "packaged backend port {PACKAGED_BACKEND_BIND} is unavailable; refusing to stop an unrelated process: {error}.{occupant} Close the owning application or configure an external backend, then retry."
            )
        })
}

#[cfg(unix)]
fn packaged_backend_port_occupant() -> Option<String> {
    let port = PACKAGED_BACKEND_BIND.rsplit(':').next()?;
    let output = Command::new("lsof")
        .args([
            "-nP",
            &format!("-iTCP:{port}"),
            "-sTCP:LISTEN",
            "-Fp",
            "-Fc",
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let output_text = String::from_utf8_lossy(&output.stdout);
    let values = output_text
        .lines()
        .filter_map(|line| line.strip_prefix('p').or_else(|| line.strip_prefix('c')))
        .filter(|value| !value.trim().is_empty())
        .take(4)
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!values.is_empty()).then(|| values.join("/"))
}

#[cfg(not(unix))]
fn packaged_backend_port_occupant() -> Option<String> {
    None
}

#[cfg(unix)]
fn terminate_pid(pid: u32) {
    let pid_arg = pid.to_string();
    let _ = Command::new("kill").arg("-TERM").arg(&pid_arg).status();
    for _ in 0..20 {
        if !is_pid_running(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let _ = Command::new("kill").arg("-KILL").arg(&pid_arg).status();
}

#[cfg(not(unix))]
fn terminate_pid(_pid: u32) {}

#[cfg(unix)]
fn is_pid_running(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

async fn wait_for_backend_health_or_exit(
    manager: &PackagedBackendManager,
    backend_url: &str,
) -> Result<(), String> {
    for _ in 0..80 {
        if backend_health_has_expected_version(backend_url).await {
            return Ok(());
        }
        let exited = manager
            .child
            .lock()
            .map_err(|_| "packaged backend process lock poisoned".to_string())?
            .as_mut()
            .map(|child| child.try_wait())
            .transpose()
            .map_err(|error| format!("failed to inspect packaged backend process: {error}"))?
            .flatten();
        if let Some(status) = exited {
            return Err(format!(
                "backend sidecar exited before health verification with {status}; startup or database migration failed, inspect backend log output"
            ));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    Err(format!(
        "backend sidecar did not become healthy with version {} at {backend_url}/health",
        env!("CARGO_PKG_VERSION")
    ))
}

async fn backend_health_has_expected_version(backend_url: &str) -> bool {
    let Ok(response) = reqwest::Client::new()
        .get(format!("{backend_url}/health"))
        .timeout(Duration::from_secs(1))
        .send()
        .await
    else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    response
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|body| {
            body.get("version")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .is_some_and(|version| version == env!("CARGO_PKG_VERSION"))
}

fn resolve_backend_binary(app_handle: &AppHandle) -> Result<PathBuf, String> {
    if let Ok(path) = std::env::var(BACKEND_BINARY_PATH_ENV) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
    }

    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|error| format!("failed to resolve resource dir: {error}"))?;
    let binary_name = if cfg!(target_os = "windows") {
        "openkoto-backend.exe"
    } else {
        "openkoto-backend"
    };
    let path = resource_dir.join("backend").join(binary_name);
    if path.is_file() {
        return Ok(path);
    }

    Err(format!(
        "packaged backend binary not found. Expected {path:?} or {BACKEND_BINARY_PATH_ENV}"
    ))
}

fn spawn_log_reader(level: LogLevel, pipe: impl std::io::Read + Send + 'static) {
    std::thread::spawn(move || {
        let reader = BufReader::new(pipe);
        for line in reader.lines().map_while(Result::ok) {
            let line = line.trim();
            if !line.is_empty() {
                LogStore::global().push(level, "backend", line.to_string());
            }
        }
    });
}

fn stop_packaged_backend_process(manager: &PackagedBackendManager) {
    stop_child(&manager.child);
}

fn stop_packaged_postgres_process(manager: &PackagedBackendManager) {
    stop_child(&manager.postgres_child);
}

fn stop_child(lock: &Mutex<Option<Child>>) {
    if let Ok(mut guard) = lock.lock() {
        if let Some(mut child) = guard.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn apply_postgres_env(command: &mut Command, runtime: &BundledPostgresRuntime) {
    command
        .env(
            "PATH",
            prepend_env_paths("PATH", &[runtime.bin_dir.clone()]),
        )
        .env(
            "DYLD_LIBRARY_PATH",
            prepend_env_paths(
                "DYLD_LIBRARY_PATH",
                &[runtime.lib_dir.clone(), runtime.pkglib_dir.clone()],
            ),
        )
        .env(
            "LD_LIBRARY_PATH",
            prepend_env_paths(
                "LD_LIBRARY_PATH",
                &[runtime.lib_dir.clone(), runtime.pkglib_dir.clone()],
            ),
        )
        .env("LC_ALL", "C");
}

fn prepend_env_paths(key: &str, paths: &[PathBuf]) -> OsString {
    let mut merged = paths.to_vec();
    if let Some(existing) = std::env::var_os(key) {
        merged.extend(std::env::split_paths(&existing));
    }
    std::env::join_paths(merged).unwrap_or_else(|_| OsString::new())
}

fn format_command_output(output: &Output) -> String {
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    format!(
        "status={} stdout={} stderr={}",
        output.status, stdout, stderr
    )
}

fn should_autostart_backend() -> bool {
    match std::env::var(AUTOSTART_ENV) {
        Ok(value) => parse_truthy(&value),
        Err(_) => !cfg!(debug_assertions),
    }
}

fn parse_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn resolve_explicit_database_url() -> Option<(String, String)> {
    if let Ok(value) = std::env::var(PACKAGED_DATABASE_URL_ENV) {
        if !value.trim().is_empty() {
            return Some((value, PACKAGED_DATABASE_URL_ENV.to_string()));
        }
    }

    if cfg!(debug_assertions) {
        if let Ok(value) = std::env::var("DATABASE_URL") {
            if !value.trim().is_empty() {
                return Some((value, "DATABASE_URL".to_string()));
            }
        }
    }

    None
}

fn binary_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn find_free_loopback_port() -> Result<u16, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("failed to allocate backend port: {error}"))?;
    listener
        .local_addr()
        .map(|addr| addr.port())
        .map_err(|error| format!("failed to read backend port: {error}"))
}

fn read_or_create_secret(path: &Path) -> Result<String, String> {
    if path.exists() {
        let value = fs::read_to_string(path)
            .map_err(|error| format!("failed to read backend jwt secret: {error}"))?;
        let value = value.trim().to_string();
        if value.as_bytes().len() >= 32 {
            set_private_file_permissions(path)?;
            return Ok(value);
        }
    }

    let secret = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create backend secret directory: {error}"))?;
    }
    fs::write(path, &secret)
        .map_err(|error| format!("failed to write backend jwt secret: {error}"))?;
    set_private_file_permissions(path)?;
    Ok(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn parse_truthy_accepts_expected_values() {
        assert!(parse_truthy("1"));
        assert!(parse_truthy("true"));
        assert!(parse_truthy(" YES "));
        assert!(parse_truthy("on"));
        assert!(!parse_truthy("0"));
        assert!(!parse_truthy("false"));
    }

    #[test]
    fn packaged_backend_url_uses_fixed_local_port() {
        assert_eq!(packaged_backend_url(), "http://127.0.0.1:19421");
    }

    #[test]
    fn bundled_postgres_share_dir_requires_initdb_templates() {
        let root = std::env::temp_dir().join(format!(
            "openkoto-postgres-share-test-{}",
            Uuid::new_v4().simple()
        ));
        let invalid = root.join("share").join("invalid");
        let valid = root.join("share").join("postgresql@16");
        fs::create_dir_all(&invalid).unwrap();
        assert!(resolve_bundled_postgres_share_dir(&root).unwrap().is_none());

        fs::create_dir_all(&valid).unwrap();
        fs::write(valid.join("postgres.bki"), b"test").unwrap();
        fs::write(valid.join("postgresql.conf.sample"), b"test").unwrap();
        assert_eq!(
            resolve_bundled_postgres_share_dir(&root).unwrap(),
            Some(valid)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn packaged_backend_url_does_not_replace_remote_backend() {
        let packaged = packaged_backend_url();
        assert!(should_persist_packaged_backend_url(None, &packaged));
        assert!(should_persist_packaged_backend_url(
            Some("http://127.0.0.1:65288"),
            &packaged
        ));
        assert!(should_persist_packaged_backend_url(
            Some("http://localhost:65288"),
            &packaged
        ));
        assert!(!should_persist_packaged_backend_url(
            Some("https://reader.example.com"),
            &packaged
        ));
        assert!(!should_persist_packaged_backend_url(
            Some(&packaged),
            &packaged
        ));
        assert!(is_packaged_local_backend_url("http://127.0.0.1:19421"));
        assert!(is_packaged_local_backend_url("http://localhost:19421"));
        assert!(!is_packaged_local_backend_url("https://reader.example.com"));
    }

    #[test]
    fn process_command_matching_requires_binary_and_data_directory() {
        let binary = Path::new(
            "/Applications/OpenKoto Desktop.app/Contents/Resources/postgres/bin/postgres",
        );
        let data = Path::new(
            "/Users/test/Library/Application Support/com.openkoto.desktop/backend/postgres-data",
        );
        let command = format!("{} -D {} -p 12345", binary.display(), data.display());
        assert!(command_matches_binary(&command, binary, Some(data)));
        assert!(!command_matches_binary(
            "/usr/local/bin/postgres -D /tmp/other",
            binary,
            Some(data)
        ));
        assert!(!command_matches_binary(
            &format!("{}-wrapper -D {}", binary.display(), data.display()),
            binary,
            Some(data)
        ));
        assert!(!command_matches_binary(
            &format!("{} -D {}-other", binary.display(), data.display()),
            binary,
            Some(data)
        ));
    }

    #[test]
    fn packaged_backend_fingerprint_changes_with_binary_content() {
        let binary = std::env::temp_dir().join(format!(
            "openkoto-backend-fingerprint-{}",
            Uuid::new_v4().simple()
        ));
        fs::write(&binary, b"first build").unwrap();
        let first = file_sha256(&binary).unwrap();
        fs::write(&binary, b"second build").unwrap();
        let second = file_sha256(&binary).unwrap();
        let _ = fs::remove_file(&binary);

        assert_eq!(first.len(), 64);
        assert_eq!(second.len(), 64);
        assert_ne!(first, second);
    }

    #[test]
    fn occupied_packaged_backend_port_fails_without_process_matching() {
        let listener = TcpListener::bind(PACKAGED_BACKEND_BIND).ok();
        let error = ensure_packaged_backend_port_available()
            .expect_err("occupied packaged backend port must fail safely");

        assert!(error.contains("19421"));
        assert!(error.contains("refusing to stop an unrelated process"));
        let repeated_error = ensure_packaged_backend_port_available()
            .expect_err("port conflict must remain after a safe refusal");
        assert!(repeated_error.contains("refusing to stop an unrelated process"));

        drop(listener);
    }

    #[test]
    fn explicit_database_url_prefers_packaged_env() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_packaged = std::env::var_os(PACKAGED_DATABASE_URL_ENV);
        let previous_database = std::env::var_os("DATABASE_URL");
        std::env::set_var(PACKAGED_DATABASE_URL_ENV, "postgres://packaged");
        std::env::set_var("DATABASE_URL", "postgres://generic");

        let (url, source) = resolve_explicit_database_url().unwrap();

        assert_eq!(url, "postgres://packaged");
        assert_eq!(source, PACKAGED_DATABASE_URL_ENV);
        restore_env(PACKAGED_DATABASE_URL_ENV, previous_packaged);
        restore_env("DATABASE_URL", previous_database);
    }

    #[test]
    fn explicit_database_url_ignores_empty_values() {
        let _guard = ENV_LOCK.lock().unwrap();
        let previous_packaged = std::env::var_os(PACKAGED_DATABASE_URL_ENV);
        let previous_database = std::env::var_os("DATABASE_URL");
        std::env::set_var(PACKAGED_DATABASE_URL_ENV, " ");
        std::env::remove_var("DATABASE_URL");

        assert!(resolve_explicit_database_url().is_none());
        restore_env(PACKAGED_DATABASE_URL_ENV, previous_packaged);
        restore_env("DATABASE_URL", previous_database);
    }

    #[test]
    fn read_or_create_secret_is_stable() {
        let path = std::env::temp_dir().join(format!("openkoto-secret-test-{}", Uuid::new_v4()));

        let first = read_or_create_secret(&path).unwrap();
        let second = read_or_create_secret(&path).unwrap();

        assert_eq!(first, second);
        assert!(first.as_bytes().len() >= 32);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn incomplete_postgres_data_is_never_reinitialized() {
        let root = std::env::temp_dir().join(format!(
            "openkoto-postgres-data-state-{}",
            Uuid::new_v4().simple()
        ));
        assert_eq!(
            inspect_postgres_data_dir(&root).unwrap(),
            PostgresDataDirState::Missing
        );

        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("orphan"), b"must survive").unwrap();
        let error = inspect_postgres_data_dir(&root).unwrap_err();
        assert!(error.contains("will not be overwritten"));
        assert_eq!(fs::read(root.join("orphan")).unwrap(), b"must survive");

        fs::write(root.join("PG_VERSION"), b"16").unwrap();
        fs::create_dir_all(root.join("base")).unwrap();
        fs::create_dir_all(root.join("global")).unwrap();
        fs::write(root.join("postgresql.conf"), b"test").unwrap();
        assert_eq!(
            inspect_postgres_data_dir(&root).unwrap(),
            PostgresDataDirState::Ready
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upgrade_failure_diagnostic_points_to_validated_recovery_copy() {
        let backend_dir = Path::new("/tmp/openkoto/backend");
        let backup_dir = Path::new("/tmp/openkoto/backend/backups/pre-upgrade-test");
        let message =
            upgrade_recovery_diagnostic("migration failed", Some(backup_dir), backend_dir);

        assert!(message.contains("Existing application data was not deleted"));
        assert!(message.contains("new backend fingerprint was not committed"));
        assert!(message.contains(backup_dir.to_string_lossy().as_ref()));
        assert!(message.contains("validate/dry-restore"));
    }

    #[test]
    fn fingerprint_write_replaces_file_without_staging_residue() {
        let root = std::env::temp_dir().join(format!(
            "openkoto-fingerprint-write-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join(BACKEND_SHA256_FILE);
        fs::write(&path, "old").unwrap();
        write_atomic_text(&path, &"a".repeat(64)).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "a".repeat(64));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn recorded_pid_with_unmatched_command_is_never_terminated() {
        let pid_file = std::env::temp_dir().join(format!(
            "openkoto-unmatched-pid-{}",
            Uuid::new_v4().simple()
        ));
        let current_pid = std::process::id();
        fs::write(&pid_file, current_pid.to_string()).unwrap();

        let error = cleanup_recorded_process(
            &pid_file,
            Path::new("/definitely/not/the/current/executable"),
            None,
            "backend",
        )
        .unwrap_err();
        assert!(error.contains("refusing to stop it"));
        assert!(is_pid_running(current_pid));
        let _ = fs::remove_file(pid_file);
    }

    fn restore_env(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
