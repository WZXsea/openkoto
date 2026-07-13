use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::process::Command;
use tauri::AppHandle;
use tauri_plugin_shell::ShellExt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegInvocation {
    System,
    Sidecar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FfmpegRequirement {
    Basic,
    SubtitleBurn,
}

pub struct FfmpegRunOutput {
    pub invocation: FfmpegInvocation,
    pub success: bool,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn resolve_ffmpeg_invocation_for_path(
    dev_mode: bool,
    path_override: Option<&OsStr>,
) -> FfmpegInvocation {
    if resolve_ffmpeg_program_for_requirement_path(
        dev_mode,
        path_override,
        FfmpegRequirement::Basic,
    )
    .is_some()
    {
        FfmpegInvocation::System
    } else {
        FfmpegInvocation::Sidecar
    }
}

pub fn resolve_ffmpeg_program_for_requirement_path(
    dev_mode: bool,
    path_override: Option<&OsStr>,
    requirement: FfmpegRequirement,
) -> Option<String> {
    if !dev_mode {
        return None;
    }

    system_ffmpeg_candidates(requirement)
        .into_iter()
        .find(|candidate| {
            system_ffmpeg_satisfies_requirement(candidate, path_override, requirement)
        })
        .map(|candidate| candidate.to_string_lossy().into_owned())
}

pub async fn run_ffmpeg(
    app_handle: &AppHandle,
    args: Vec<String>,
) -> Result<FfmpegRunOutput, String> {
    run_ffmpeg_with_requirement(app_handle, args, FfmpegRequirement::Basic).await
}

pub async fn run_ffmpeg_with_requirement(
    app_handle: &AppHandle,
    args: Vec<String>,
    requirement: FfmpegRequirement,
) -> Result<FfmpegRunOutput, String> {
    let program = resolve_ffmpeg_program_for_requirement_path(
        cfg!(debug_assertions),
        std::env::var_os("PATH").as_deref(),
        requirement,
    );

    match program {
        Some(program) => run_system_ffmpeg(program, args).await,
        None => run_sidecar_ffmpeg(app_handle, args).await,
    }
}

async fn run_system_ffmpeg(program: String, args: Vec<String>) -> Result<FfmpegRunOutput, String> {
    let output =
        tauri::async_runtime::spawn_blocking(move || Command::new(program).args(args).output())
            .await
            .map_err(|error| format!("系统 FFmpeg 执行任务失败: {error}"))?
            .map_err(|error| format!("系统 FFmpeg 执行失败: {error}"))?;

    Ok(FfmpegRunOutput {
        invocation: FfmpegInvocation::System,
        success: output.status.success(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

async fn run_sidecar_ffmpeg(
    app_handle: &AppHandle,
    args: Vec<String>,
) -> Result<FfmpegRunOutput, String> {
    let output = app_handle
        .shell()
        .sidecar("ffmpeg")
        .map_err(|error| format!("无法创建 FFmpeg sidecar: {error}"))?
        .args(args)
        .output()
        .await
        .map_err(|error| format!("FFmpeg sidecar 执行失败: {error}"))?;

    Ok(FfmpegRunOutput {
        invocation: FfmpegInvocation::Sidecar,
        success: output.status.success(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

fn system_ffmpeg_satisfies_requirement(
    program: &OsStr,
    path_override: Option<&OsStr>,
    requirement: FfmpegRequirement,
) -> bool {
    if !system_ffmpeg_available(program, path_override) {
        return false;
    }

    match requirement {
        FfmpegRequirement::Basic => true,
        FfmpegRequirement::SubtitleBurn => {
            system_ffmpeg_supports_subtitle_burn(program, path_override)
        }
    }
}

fn system_ffmpeg_available(program: &OsStr, path_override: Option<&OsStr>) -> bool {
    let mut command = Command::new(program);
    command.arg("-version");

    if let Some(path) = path_override {
        command.env("PATH", path);
    }

    matches!(command.output(), Ok(output) if output.status.success())
}

fn system_ffmpeg_supports_subtitle_burn(program: &OsStr, path_override: Option<&OsStr>) -> bool {
    let mut command = Command::new(program);
    command.args(["-hide_banner", "-filters"]);

    if let Some(path) = path_override {
        command.env("PATH", path);
    }

    let Ok(output) = command.output() else {
        return false;
    };

    if !output.status.success() {
        return false;
    }

    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    combined
        .lines()
        .any(|line| line.contains(" ass ") || line.contains(" subtitles "))
}

fn system_ffmpeg_candidates(requirement: FfmpegRequirement) -> Vec<OsString> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    if matches!(requirement, FfmpegRequirement::SubtitleBurn) {
        if let Some(path) = std::env::var_os("OPENKOTO_FFMPEG_SUBTITLE") {
            push_candidate(&mut candidates, &mut seen, path);
        }

        #[cfg(target_os = "macos")]
        {
            push_candidate(
                &mut candidates,
                &mut seen,
                OsString::from("/opt/homebrew/opt/ffmpeg-full/bin/ffmpeg"),
            );
            push_candidate(
                &mut candidates,
                &mut seen,
                OsString::from("/usr/local/opt/ffmpeg-full/bin/ffmpeg"),
            );
        }
    }

    if let Some(path) = std::env::var_os("OPENKOTO_FFMPEG") {
        push_candidate(&mut candidates, &mut seen, path);
    }

    push_candidate(
        &mut candidates,
        &mut seen,
        OsString::from(ffmpeg_binary_name()),
    );
    candidates
}

fn push_candidate(candidates: &mut Vec<OsString>, seen: &mut HashSet<String>, candidate: OsString) {
    let key = candidate.to_string_lossy().into_owned();
    if seen.insert(key) {
        candidates.push(candidate);
    }
}

fn ffmpeg_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    }
}
