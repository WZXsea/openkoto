use std::fs;
use std::path::Path;

use tauri::{AppHandle, Manager};

use super::safe_paths::{
    prepare_user_export_path, resolve_existing_file_within_base, SafePathError,
};

pub const TEXT_EXPORT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "json", "log", "srt", "vtt", "ass", "lrc", "csv",
];
pub const BINARY_EXPORT_EXTENSIONS: &[&str] = &["docx"];
pub const COPY_EXPORT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "json", "log", "srt", "vtt", "ass", "lrc", "csv", "docx", "pdf",
    "epub",
];

fn export_error(action: &str, error: SafePathError) -> String {
    format!("{action}: {error}")
}

pub fn write_text_export(path: &str, content: &str) -> Result<(), String> {
    let target = prepare_user_export_path(Path::new(path), TEXT_EXPORT_EXTENSIONS)
        .map_err(|error| export_error("Rejected text export path", error))?;

    fs::write(target, content).map_err(|error| format!("Failed to write file: {error}"))
}

pub fn write_binary_export(path: &str, content: &[u8]) -> Result<(), String> {
    let target = prepare_user_export_path(Path::new(path), BINARY_EXPORT_EXTENSIONS)
        .map_err(|error| export_error("Rejected binary export path", error))?;

    fs::write(target, content).map_err(|error| format!("Failed to write file: {error}"))
}

pub fn copy_app_data_file_to_export(
    app_handle: &AppHandle,
    src_path: &str,
    dest_path: &str,
) -> Result<(), String> {
    let app_data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|error| format!("Failed to get app data dir: {error}"))?;
    let source = resolve_existing_file_within_base(&app_data_dir, Path::new(src_path))
        .map_err(|error| export_error("Rejected export source path", error))?;
    let target = prepare_user_export_path(Path::new(dest_path), COPY_EXPORT_EXTENSIONS)
        .map_err(|error| export_error("Rejected export destination path", error))?;

    fs::copy(source, target).map_err(|error| format!("Failed to export file: {error}"))?;
    Ok(())
}
