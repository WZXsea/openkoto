use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SafePathError {
    EmptyPath,
    RelativePath,
    AbsolutePath,
    PathTraversal,
    MissingBase,
    MissingParent,
    ParentNotDirectory,
    MissingTarget,
    TargetIsDirectory,
    TargetIsSymlink,
    OutsideBase,
    UnsupportedExtension,
    ForbiddenWriteRoot,
}

impl SafePathError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::MissingBase | Self::MissingTarget)
    }
}

impl fmt::Display for SafePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::EmptyPath => "path is empty",
            Self::RelativePath => "path must be absolute",
            Self::AbsolutePath => "path must be relative to the allowed base directory",
            Self::PathTraversal => "path traversal is not allowed",
            Self::MissingBase => "allowed base directory does not exist",
            Self::MissingParent => "target parent directory does not exist",
            Self::ParentNotDirectory => "target parent is not a directory",
            Self::MissingTarget => "target file does not exist",
            Self::TargetIsDirectory => "target path is a directory",
            Self::TargetIsSymlink => "target path is a symlink",
            Self::OutsideBase => "target path is outside the allowed base directory",
            Self::UnsupportedExtension => "file extension is not supported for this export",
            Self::ForbiddenWriteRoot => "writing to this system location is not allowed",
        };
        f.write_str(message)
    }
}

impl std::error::Error for SafePathError {}

pub type SafePathResult<T> = Result<T, SafePathError>;

fn has_ambiguous_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir | Component::CurDir))
}

fn has_only_plain_relative_components(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn canonicalize_existing_dir(path: &Path, missing_error: SafePathError) -> SafePathResult<PathBuf> {
    if !path.exists() {
        return Err(missing_error);
    }
    let canonical = path.canonicalize().map_err(|_| missing_error)?;
    if !canonical.is_dir() {
        return Err(SafePathError::ParentNotDirectory);
    }
    Ok(canonical)
}

pub fn resolve_existing_child_path(base_dir: &Path, child: &str) -> SafePathResult<PathBuf> {
    if child.trim().is_empty() {
        return Err(SafePathError::EmptyPath);
    }

    let child_path = Path::new(child);
    if child_path.is_absolute() {
        return Err(SafePathError::AbsolutePath);
    }
    if !has_only_plain_relative_components(child_path) {
        return Err(SafePathError::PathTraversal);
    }

    let base = canonicalize_existing_dir(base_dir, SafePathError::MissingBase)?;
    let target = base.join(child_path);

    if !target.exists() {
        return Err(SafePathError::MissingTarget);
    }

    let metadata = target
        .symlink_metadata()
        .map_err(|_| SafePathError::MissingTarget)?;
    if metadata.file_type().is_symlink() {
        return Err(SafePathError::TargetIsSymlink);
    }
    if metadata.is_dir() {
        return Err(SafePathError::TargetIsDirectory);
    }

    let canonical_target = target
        .canonicalize()
        .map_err(|_| SafePathError::MissingTarget)?;
    if !canonical_target.starts_with(&base) {
        return Err(SafePathError::OutsideBase);
    }

    Ok(canonical_target)
}

pub fn validate_storage_id(id: &str) -> SafePathResult<()> {
    if id.trim().is_empty() {
        return Err(SafePathError::EmptyPath);
    }
    if id == "." || id == ".." {
        return Err(SafePathError::PathTraversal);
    }
    if id.len() > 128 {
        return Err(SafePathError::PathTraversal);
    }
    if id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    {
        Ok(())
    } else {
        Err(SafePathError::PathTraversal)
    }
}

pub fn resolve_existing_file_within_base(base_dir: &Path, path: &Path) -> SafePathResult<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(SafePathError::EmptyPath);
    }
    if !path.is_absolute() {
        return Err(SafePathError::RelativePath);
    }
    if has_ambiguous_component(path) {
        return Err(SafePathError::PathTraversal);
    }

    let base = canonicalize_existing_dir(base_dir, SafePathError::MissingBase)?;
    if !path.exists() {
        return Err(SafePathError::MissingTarget);
    }

    let metadata = path
        .symlink_metadata()
        .map_err(|_| SafePathError::MissingTarget)?;
    if metadata.file_type().is_symlink() {
        return Err(SafePathError::TargetIsSymlink);
    }
    if metadata.is_dir() {
        return Err(SafePathError::TargetIsDirectory);
    }

    let canonical_target = path
        .canonicalize()
        .map_err(|_| SafePathError::MissingTarget)?;
    if !canonical_target.starts_with(&base) {
        return Err(SafePathError::OutsideBase);
    }

    Ok(canonical_target)
}

pub fn prepare_user_export_path(
    path: &Path,
    allowed_extensions: &[&str],
) -> SafePathResult<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(SafePathError::EmptyPath);
    }
    if !path.is_absolute() {
        return Err(SafePathError::RelativePath);
    }
    if has_ambiguous_component(path) {
        return Err(SafePathError::PathTraversal);
    }

    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .ok_or(SafePathError::UnsupportedExtension)?;
    if !allowed_extensions
        .iter()
        .any(|allowed| extension == allowed.to_ascii_lowercase())
    {
        return Err(SafePathError::UnsupportedExtension);
    }

    let parent = path.parent().ok_or(SafePathError::MissingParent)?;
    let parent = canonicalize_existing_dir(parent, SafePathError::MissingParent)?;
    if is_forbidden_write_root(&parent) {
        return Err(SafePathError::ForbiddenWriteRoot);
    }

    if path.exists() {
        let metadata = path
            .symlink_metadata()
            .map_err(|_| SafePathError::MissingTarget)?;
        if metadata.file_type().is_symlink() {
            return Err(SafePathError::TargetIsSymlink);
        }
        if metadata.is_dir() {
            return Err(SafePathError::TargetIsDirectory);
        }
    }

    let filename = path.file_name().ok_or(SafePathError::EmptyPath)?;
    Ok(parent.join(filename))
}

#[cfg(unix)]
fn is_forbidden_write_root(parent: &Path) -> bool {
    [
        Path::new("/"),
        Path::new("/bin"),
        Path::new("/sbin"),
        Path::new("/usr"),
        Path::new("/usr/bin"),
        Path::new("/usr/sbin"),
        Path::new("/etc"),
        Path::new("/private/etc"),
        Path::new("/System"),
        Path::new("/Library"),
        Path::new("/Applications"),
    ]
    .iter()
    .any(|forbidden| parent == *forbidden || parent.starts_with(forbidden))
}

#[cfg(not(unix))]
fn is_forbidden_write_root(_parent: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "openkoto-safe-paths-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resolve_existing_child_path_rejects_traversal() {
        let root = temp_dir("resource-traversal");
        fs::create_dir_all(root.join("videos")).unwrap();
        fs::write(root.join("secret.txt"), b"secret").unwrap();

        let error = resolve_existing_child_path(&root.join("videos"), "../secret.txt").unwrap_err();

        assert_eq!(error, SafePathError::PathTraversal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_existing_child_path_accepts_plain_file_inside_base() {
        let root = temp_dir("resource-file");
        let videos = root.join("videos");
        fs::create_dir_all(&videos).unwrap();
        fs::write(videos.join("movie.mp4"), b"video").unwrap();

        let resolved = resolve_existing_child_path(&videos, "movie.mp4").unwrap();

        assert_eq!(resolved, videos.join("movie.mp4").canonicalize().unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepare_user_export_path_rejects_relative_paths() {
        let error = prepare_user_export_path(Path::new("notes.md"), &["md"]).unwrap_err();

        assert_eq!(error, SafePathError::RelativePath);
    }

    #[test]
    fn prepare_user_export_path_rejects_unsupported_extensions() {
        let root = temp_dir("bad-extension");
        let error = prepare_user_export_path(&root.join("script.sh"), &["txt"]).unwrap_err();

        assert_eq!(error, SafePathError::UnsupportedExtension);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_existing_file_within_base_rejects_outside_file() {
        let root = temp_dir("source-base");
        let allowed = root.join("app-data");
        let outside = root.join("outside.log");
        fs::create_dir_all(&allowed).unwrap();
        fs::write(&outside, b"log").unwrap();

        let error = resolve_existing_file_within_base(&allowed, &outside).unwrap_err();

        assert_eq!(error, SafePathError::OutsideBase);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn validate_storage_id_accepts_uuid_like_and_system_ids() {
        validate_storage_id("550e8400-e29b-41d4-a716-446655440000").unwrap();
        validate_storage_id("system-ungrouped").unwrap();
        validate_storage_id("task_1").unwrap();
    }

    #[test]
    fn validate_storage_id_rejects_path_components() {
        assert_eq!(
            validate_storage_id("../config.json").unwrap_err(),
            SafePathError::PathTraversal
        );
        assert_eq!(
            validate_storage_id("a/b").unwrap_err(),
            SafePathError::PathTraversal
        );
        assert_eq!(
            validate_storage_id("a\\b").unwrap_err(),
            SafePathError::PathTraversal
        );
        assert_eq!(
            validate_storage_id(".").unwrap_err(),
            SafePathError::PathTraversal
        );
    }
}
