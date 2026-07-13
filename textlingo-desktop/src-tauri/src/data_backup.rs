use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

const BACKUP_FORMAT_VERSION: u32 = 1;
const BACKUPS_DIR: &str = "backups";
const MAX_UPGRADE_BACKUPS: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupFileEntry {
    pub relative_path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpgradeBackupManifest {
    pub format_version: u32,
    pub created_at: String,
    pub source_backend_sha256: Option<String>,
    pub target_backend_sha256: String,
    pub postgres_snapshot: bool,
    pub app_config_snapshot: bool,
    pub jwt_secret_snapshot: bool,
    pub file_storage_entries: Vec<BackupFileEntry>,
}

pub fn create_upgrade_backup_if_needed(
    app_data_dir: &Path,
    backend_dir: &Path,
    source_backend_sha256: Option<&str>,
    target_backend_sha256: &str,
) -> Result<Option<PathBuf>, String> {
    if source_backend_sha256 == Some(target_backend_sha256) {
        return Ok(None);
    }

    let postgres_dir = backend_dir.join("postgres-data");
    if !postgres_dir.join("PG_VERSION").is_file() {
        return Ok(None);
    }

    let backups_dir = backend_dir.join(BACKUPS_DIR);
    fs::create_dir_all(&backups_dir)
        .map_err(|error| format!("failed to create upgrade backup directory: {error}"))?;
    set_private_directory_permissions(&backups_dir)?;

    let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
    let target_prefix = target_backend_sha256
        .get(..12)
        .unwrap_or(target_backend_sha256);
    let final_dir = backups_dir.join(format!("pre-upgrade-{timestamp}-{target_prefix}"));
    let staging_dir = backups_dir.join(format!(".staging-{}", Uuid::new_v4().simple()));
    fs::create_dir_all(&staging_dir)
        .map_err(|error| format!("failed to create upgrade backup staging directory: {error}"))?;
    set_private_directory_permissions(&staging_dir)?;

    let result = (|| {
        copy_dir_recursive(&postgres_dir, &staging_dir.join("postgres-data"))?;

        let app_config_snapshot = copy_optional_file(
            &app_data_dir.join("config.json"),
            &staging_dir.join("config.json"),
        )?;
        let jwt_secret_snapshot = copy_optional_file(
            &backend_dir.join("jwt_secret"),
            &staging_dir.join("jwt_secret"),
        )?;
        if jwt_secret_snapshot {
            set_private_file_permissions(&staging_dir.join("jwt_secret"))?;
        }
        let file_storage_entries = collect_file_manifest(&backend_dir.join("files"))?;

        let manifest = UpgradeBackupManifest {
            format_version: BACKUP_FORMAT_VERSION,
            created_at: Utc::now().to_rfc3339(),
            source_backend_sha256: source_backend_sha256.map(str::to_string),
            target_backend_sha256: target_backend_sha256.to_string(),
            postgres_snapshot: true,
            app_config_snapshot,
            jwt_secret_snapshot,
            file_storage_entries,
        };
        write_manifest(&staging_dir, &manifest)?;
        validate_backup(&staging_dir)?;
        dry_restore_backup(&staging_dir)?;
        fs::rename(&staging_dir, &final_dir)
            .map_err(|error| format!("failed to commit upgrade backup: {error}"))?;
        prune_upgrade_backups(&backups_dir, MAX_UPGRADE_BACKUPS)?;
        Ok(final_dir.clone())
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&staging_dir);
    }
    result.map(Some)
}

pub fn validate_backup(backup_dir: &Path) -> Result<UpgradeBackupManifest, String> {
    let manifest_path = backup_dir.join("manifest.json");
    let bytes = fs::read(&manifest_path)
        .map_err(|error| format!("failed to read backup manifest: {error}"))?;
    let manifest: UpgradeBackupManifest = serde_json::from_slice(&bytes)
        .map_err(|error| format!("failed to parse backup manifest: {error}"))?;
    if manifest.format_version != BACKUP_FORMAT_VERSION {
        return Err(format!(
            "unsupported backup format version {}",
            manifest.format_version
        ));
    }
    if manifest.target_backend_sha256.len() != 64 {
        return Err("backup manifest target fingerprint is invalid".to_string());
    }
    if manifest.postgres_snapshot && !backup_dir.join("postgres-data/PG_VERSION").is_file() {
        return Err("backup PostgreSQL snapshot is incomplete".to_string());
    }
    if manifest.app_config_snapshot && !backup_dir.join("config.json").is_file() {
        return Err("backup config snapshot is missing".to_string());
    }
    if manifest.jwt_secret_snapshot && !backup_dir.join("jwt_secret").is_file() {
        return Err("backup JWT secret snapshot is missing".to_string());
    }
    Ok(manifest)
}

pub fn dry_restore_backup(backup_dir: &Path) -> Result<(), String> {
    let manifest = validate_backup(backup_dir)?;
    let restore_dir = backup_dir
        .parent()
        .unwrap_or(backup_dir)
        .join(format!(".restore-check-{}", Uuid::new_v4().simple()));
    let result = (|| {
        if manifest.postgres_snapshot {
            copy_dir_recursive(
                &backup_dir.join("postgres-data"),
                &restore_dir.join("postgres-data"),
            )?;
            if !restore_dir.join("postgres-data/PG_VERSION").is_file() {
                return Err("dry restore did not reproduce PG_VERSION".to_string());
            }
        }
        if manifest.app_config_snapshot {
            copy_optional_file(
                &backup_dir.join("config.json"),
                &restore_dir.join("config.json"),
            )?;
        }
        if manifest.jwt_secret_snapshot {
            copy_optional_file(
                &backup_dir.join("jwt_secret"),
                &restore_dir.join("jwt_secret"),
            )?;
        }
        validate_restored_files(&restore_dir, &manifest)
    })();
    let _ = fs::remove_dir_all(&restore_dir);
    result
}

fn validate_restored_files(
    restore_dir: &Path,
    manifest: &UpgradeBackupManifest,
) -> Result<(), String> {
    if manifest.app_config_snapshot && !restore_dir.join("config.json").is_file() {
        return Err("dry restore config snapshot is missing".to_string());
    }
    if manifest.jwt_secret_snapshot && !restore_dir.join("jwt_secret").is_file() {
        return Err("dry restore JWT secret snapshot is missing".to_string());
    }
    Ok(())
}

fn write_manifest(backup_dir: &Path, manifest: &UpgradeBackupManifest) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("failed to serialize backup manifest: {error}"))?;
    let path = backup_dir.join("manifest.json");
    fs::write(&path, bytes).map_err(|error| format!("failed to write backup manifest: {error}"))?;
    set_private_file_permissions(&path)
}

fn copy_optional_file(source: &Path, target: &Path) -> Result<bool, String> {
    if !source.is_file() {
        return Ok(false);
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create backup file directory: {error}"))?;
    }
    fs::copy(source, target)
        .map_err(|error| format!("failed to copy {}: {error}", source.display()))?;
    Ok(true)
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target)
        .map_err(|error| format!("failed to create {}: {error}", target.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("failed to read {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| format!("failed to read backup entry: {error}"))?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", source_path.display()))?;
        if file_type.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &target_path).map_err(|error| {
                format!(
                    "failed to copy {} to {}: {error}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn collect_file_manifest(root: &Path) -> Result<Vec<BackupFileEntry>, String> {
    if !root.is_dir() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    collect_file_manifest_recursive(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(entries)
}

fn collect_file_manifest_recursive(
    root: &Path,
    current: &Path,
    entries: &mut Vec<BackupFileEntry>,
) -> Result<(), String> {
    for entry in
        fs::read_dir(current).map_err(|error| format!("failed to inspect file storage: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("failed to inspect file storage entry: {error}"))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect {}: {error}", path.display()))?;
        if file_type.is_dir() {
            collect_file_manifest_recursive(root, &path, entries)?;
        } else if file_type.is_file() {
            let relative_path = path
                .strip_prefix(root)
                .map_err(|error| format!("failed to normalize file manifest path: {error}"))?
                .to_string_lossy()
                .replace('\\', "/");
            let (size, sha256) = hash_file(&path)?;
            entries.push(BackupFileEntry {
                relative_path,
                size,
                sha256,
            });
        }
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<(u64, String), String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("failed to open {} for checksum: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to checksum {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, hex::encode(hasher.finalize())))
}

#[cfg(unix)]
pub(crate) fn set_private_file_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|error| format!("failed to secure {}: {error}", path.display()))
}

#[cfg(not(unix))]
pub(crate) fn set_private_file_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("failed to secure {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn prune_upgrade_backups(backups_dir: &Path, keep: usize) -> Result<(), String> {
    let mut backups = fs::read_dir(backups_dir)
        .map_err(|error| format!("failed to inspect upgrade backups: {error}"))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
                && entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("pre-upgrade-")
        })
        .collect::<Vec<_>>();
    backups.sort_by_key(|entry| entry.file_name());
    let remove_count = backups.len().saturating_sub(keep);
    for entry in backups.into_iter().take(remove_count) {
        fs::remove_dir_all(entry.path())
            .map_err(|error| format!("failed to prune old upgrade backup: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("openkoto-{label}-{}", Uuid::new_v4().simple()))
    }

    #[test]
    fn creates_valid_upgrade_backup_and_dry_restore() {
        let app_data = temp_root("backup");
        let backend = app_data.join("backend");
        fs::create_dir_all(backend.join("postgres-data/base")).unwrap();
        fs::create_dir_all(backend.join("files/materials")).unwrap();
        fs::write(backend.join("postgres-data/PG_VERSION"), b"16").unwrap();
        fs::write(backend.join("postgres-data/base/data"), b"database").unwrap();
        fs::write(backend.join("files/materials/example.txt"), b"material").unwrap();
        fs::write(
            backend.join("jwt_secret"),
            b"01234567890123456789012345678901",
        )
        .unwrap();
        fs::write(app_data.join("config.json"), b"{}").unwrap();

        let target = "b".repeat(64);
        let backup =
            create_upgrade_backup_if_needed(&app_data, &backend, Some(&"a".repeat(64)), &target)
                .unwrap()
                .unwrap();
        let manifest = validate_backup(&backup).unwrap();

        assert_eq!(manifest.target_backend_sha256, target);
        assert!(manifest.postgres_snapshot);
        assert!(manifest.app_config_snapshot);
        assert!(manifest.jwt_secret_snapshot);
        assert_eq!(manifest.file_storage_entries.len(), 1);
        assert_eq!(
            manifest.file_storage_entries[0].relative_path,
            "materials/example.txt"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(backup.join("jwt_secret"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        dry_restore_backup(&backup).unwrap();
        let _ = fs::remove_dir_all(app_data);
    }

    #[test]
    fn skips_backup_without_existing_database_or_backend_change() {
        let app_data = temp_root("backup-skip");
        let backend = app_data.join("backend");
        fs::create_dir_all(&backend).unwrap();
        let hash = "a".repeat(64);

        assert!(
            create_upgrade_backup_if_needed(&app_data, &backend, None, &hash)
                .unwrap()
                .is_none()
        );
        fs::create_dir_all(backend.join("postgres-data")).unwrap();
        fs::write(backend.join("postgres-data/PG_VERSION"), b"16").unwrap();
        assert!(
            create_upgrade_backup_if_needed(&app_data, &backend, Some(&hash), &hash)
                .unwrap()
                .is_none()
        );
        let _ = fs::remove_dir_all(app_data);
    }

    #[test]
    fn retention_keeps_latest_backups() {
        let root = temp_root("backup-retention");
        fs::create_dir_all(&root).unwrap();
        for index in 0..5 {
            fs::create_dir_all(root.join(format!("pre-upgrade-2026010{index}"))).unwrap();
        }
        prune_upgrade_backups(&root, 3).unwrap();
        let remaining = fs::read_dir(&root).unwrap().count();
        assert_eq!(remaining, 3);
        let _ = fs::remove_dir_all(root);
    }
}
