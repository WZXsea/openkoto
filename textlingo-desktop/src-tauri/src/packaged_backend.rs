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
use tauri::{AppHandle, Manager};
use uuid::Uuid;

use crate::{
    logging::{LogLevel, LogStore},
    storage::{load_config, save_config},
};

const AUTOSTART_ENV: &str = "OPENKOTO_DESKTOP_AUTOSTART_BACKEND";
const BACKEND_BINARY_PATH_ENV: &str = "OPENKOTO_BACKEND_BINARY_PATH";
const PACKAGED_DATABASE_URL_ENV: &str = "OPENKOTO_PACKAGED_DATABASE_URL";
const DEFAULT_DATABASE_URL: &str =
    "postgres://openkoto:openkoto_dev_password@127.0.0.1:5433/openkoto_dev";
const MANAGED_DATABASE_NAME: &str = "openkoto";
const MANAGED_DATABASE_USER: &str = "openkoto";

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
    let secret = read_or_create_secret(&backend_dir.join("jwt_secret"))?;
    let port = find_free_loopback_port()?;
    let bind_addr = format!("127.0.0.1:{port}");
    let backend_url = format!("http://{bind_addr}");
    let database_runtime = prepare_database_runtime(&app_handle, &backend_dir).await?;

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
            return Err(format!("failed to spawn packaged backend: {error}"));
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

    if let Err(error) = wait_for_backend_health(&backend_url).await {
        stop_packaged_backend_process(&manager);
        if database_runtime.managed_postgres {
            stop_packaged_postgres_process(&manager);
        }
        return Err(error);
    }
    persist_backend_url(&app_handle, &backend_url)?;

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

    if !data_dir.join("PG_VERSION").is_file() {
        run_initdb(&runtime, &data_dir)?;
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
    let runtime = BundledPostgresRuntime {
        bin_dir: root.join("bin"),
        lib_dir: root.join("lib"),
        pkglib_dir: root.join("lib").join("postgresql"),
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

fn update_status(app_handle: &AppHandle, status: PackagedBackendStatus) {
    if let Ok(mut guard) = app_handle.state::<PackagedBackendManager>().status.lock() {
        *guard = status;
    }
}

fn persist_backend_url(app_handle: &AppHandle, backend_url: &str) -> Result<(), String> {
    let mut config = load_config(app_handle)?.unwrap_or_default();
    if config.backend_url.as_deref() != Some(backend_url) {
        config.backend_url = Some(backend_url.to_string());
        save_config(app_handle, &config)?;
    }
    Ok(())
}

async fn wait_for_backend_health(backend_url: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let health_url = format!("{backend_url}/health");

    for _ in 0..80 {
        if matches!(
            client.get(&health_url).send().await,
            Ok(response) if response.status().is_success()
        ) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    Err(format!(
        "backend sidecar did not become healthy at {health_url}"
    ))
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

    if let Ok(value) = std::env::var("DATABASE_URL") {
        if !value.trim().is_empty() {
            return Some((value, "DATABASE_URL".to_string()));
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

    fn restore_env(key: &str, previous: Option<std::ffi::OsString>) {
        match previous {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}
