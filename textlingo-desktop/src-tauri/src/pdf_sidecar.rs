use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfSidecarCommand {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
}

pub fn resolve_pdf_sidecar(app_handle: &AppHandle) -> Result<PdfSidecarCommand, String> {
    if cfg!(debug_assertions) {
        let cwd = std::env::current_dir().map_err(|err| {
            format!("PDF sidecar resolution failed to read current directory: {err}")
        })?;
        return resolve_pdf_sidecar_for_dir(&cwd, true);
    }

    let resource_dir = app_handle
        .path()
        .resource_dir()
        .map_err(|err| format!("PDF sidecar resolution failed to read resource dir: {err}"))?;

    resolve_pdf_sidecar_for_dir(&resource_dir, false)
}

pub fn resolve_pdf_sidecar_for_dir(
    base_dir: &Path,
    dev_mode: bool,
) -> Result<PdfSidecarCommand, String> {
    if dev_mode {
        let candidates = [
            base_dir.join("../pdf-sidecar"),
            base_dir.join("../../pdf-sidecar"),
            base_dir.join("pdf-sidecar"),
            base_dir.join("../textlingo-desktop/pdf-sidecar"),
            base_dir.join("../../textlingo-desktop/pdf-sidecar"),
            base_dir.join("textlingo-desktop/pdf-sidecar"),
            base_dir.join("../plugins/openkoto-pdf-translator"),
            base_dir.join("../../plugins/openkoto-pdf-translator"),
            base_dir.join("plugins/openkoto-pdf-translator"),
        ];

        for candidate in candidates {
            if candidate.join("openkoto_pdf_translator/pdf2zh.py").exists() {
                let working_dir = candidate.canonicalize().unwrap_or(candidate);
                return Ok(PdfSidecarCommand {
                    program: "python".to_string(),
                    args: vec![
                        "-m".to_string(),
                        "openkoto_pdf_translator.pdf2zh".to_string(),
                    ],
                    working_dir,
                });
            }
        }

        return Err(format!(
            "PDF sidecar not available in development mode from base dir: {}",
            base_dir.display()
        ));
    }

    for binary_path in production_binary_candidates(base_dir) {
        if binary_path.exists() {
            return Ok(PdfSidecarCommand {
                program: binary_path.to_string_lossy().to_string(),
                args: Vec::new(),
                working_dir: binary_path
                    .parent()
                    .and_then(|path| path.canonicalize().ok())
                    .or_else(|| binary_path.parent().map(Path::to_path_buf))
                    .unwrap_or_else(|| base_dir.to_path_buf()),
            });
        }
    }

    Err(format!(
        "PDF sidecar not bundled for production from base dir: {}",
        base_dir.display()
    ))
}

fn bundled_binary_name() -> &'static str {
    if cfg!(target_os = "macos") && cfg!(target_arch = "aarch64") {
        "openkoto-pdf-translator-aarch64-apple-darwin"
    } else if cfg!(target_os = "macos") {
        "openkoto-pdf-translator-x86_64-apple-darwin"
    } else if cfg!(target_os = "windows") {
        "openkoto-pdf-translator-x86_64-pc-windows-msvc.exe"
    } else {
        "openkoto-pdf-translator-x86_64-unknown-linux-gnu"
    }
}

fn packaged_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "openkoto-pdf-translator.exe"
    } else {
        "openkoto-pdf-translator"
    }
}

fn production_binary_candidates(base_dir: &Path) -> Vec<PathBuf> {
    let mut candidates = vec![
        base_dir.join("binaries").join(bundled_binary_name()),
        base_dir.join("binaries").join(packaged_binary_name()),
        base_dir.join(packaged_binary_name()),
    ];

    if let Some(parent) = base_dir.parent() {
        candidates.push(parent.join(packaged_binary_name()));
        candidates.push(parent.join("MacOS").join(packaged_binary_name()));
    }

    candidates
}

pub fn build_pdf_sidecar_command_for_dir(
    base_dir: &Path,
    dev_mode: bool,
    pdf_path: &str,
    lang_in: &str,
    lang_out: &str,
    output_dir: &str,
) -> Result<PdfSidecarCommand, String> {
    let mut command = resolve_pdf_sidecar_for_dir(base_dir, dev_mode)?;
    command.args.extend([
        pdf_path.to_string(),
        "-li".to_string(),
        lang_in.to_string(),
        "-lo".to_string(),
        lang_out.to_string(),
        "-s".to_string(),
        "openkoto".to_string(),
        "-o".to_string(),
        output_dir.to_string(),
    ]);
    Ok(command)
}

/// Suppress the console window that would otherwise flash on Windows when the
/// PDF sidecar (a `console=True` PyInstaller binary) is spawned via
/// `std::process::Command`. Tauri's shell plugin already does this for its own
/// sidecars (ffmpeg/yt-dlp), so only this raw spawn needs it. No-op elsewhere.
pub fn hide_console_window(command: &mut std::process::Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}
