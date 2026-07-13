use crate::ffmpeg::{run_ffmpeg_with_requirement, FfmpegRequirement};
use crate::types::Article;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KtvDisplayMode {
    Original,
    Bilingual,
    Translation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KtvPositionPreset {
    Bottom,
    LowerThird,
    CenterLower,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KtvExportConfig {
    pub display_mode: KtvDisplayMode,
    pub show_reading: bool,
    pub original_font_family: String,
    pub translation_font_family: String,
    pub reading_font_family: String,
    pub font_size: u32,
    pub reading_scale: f32,
    pub line_gap: u32,
    pub bilingual_gap: u32,
    pub original_color: String,
    pub translation_color: String,
    pub reading_color: String,
    pub outline_color: String,
    pub outline_width: u32,
    pub shadow_enabled: bool,
    pub shadow_color: String,
    pub shadow_offset_x: i32,
    pub shadow_offset_y: i32,
    pub shadow_blur: u32,
    pub position_preset: KtvPositionPreset,
    pub bottom_margin: u32,
    pub horizontal_margin: u32,
    #[serde(default)]
    pub video_width: Option<u32>,
    #[serde(default)]
    pub video_height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KtvExportResult {
    pub output_path: String,
}

fn should_fill_reading(language_hint: Option<&str>) -> bool {
    matches!(
        language_hint,
        Some("ja") | Some("ja-JP") | Some("ko") | Some("ko-KR")
    )
}

fn generate_segment_reading(text: &str, language_hint: Option<&str>) -> Option<String> {
    if !should_fill_reading(language_hint) {
        return None;
    }

    let normalized = text.trim();
    if normalized.is_empty() {
        return None;
    }

    Some(normalized.to_string())
}

pub fn prepare_ktv_segments(
    mut article: Article,
    language_hint: Option<&str>,
) -> Result<Article, String> {
    if article.media_path.is_none() {
        return Err("仅视频素材支持 KTV 导出".to_string());
    }

    for segment in article.segments.iter_mut() {
        if segment.reading_text.is_some() {
            continue;
        }

        if segment.start_time.is_none() || segment.end_time.is_none() {
            continue;
        }

        segment.reading_text = generate_segment_reading(&segment.text, language_hint);
    }

    Ok(article)
}

pub fn generate_ktv_ass(article: &Article, config: &KtvExportConfig) -> Result<String, String> {
    let (play_res_x, play_res_y) = ass_play_resolution(config);
    let original_font_size = config.font_size;
    let reading_font_size = ((config.font_size as f32) * config.reading_scale).round() as u32;
    let translation_font_size = config.font_size.saturating_sub(4).max(20);
    let original_margin_v = original_margin_v(config, translation_font_size);
    let reading_margin_v = reading_margin_v(config, original_margin_v, original_font_size);
    let translation_margin_v = translation_margin_v(config);
    let shadow = shadow_value(config);

    let mut lines = vec![
        "[Script Info]".to_string(),
        "ScriptType: v4.00+".to_string(),
        format!("PlayResX: {play_res_x}"),
        format!("PlayResY: {play_res_y}"),
        "".to_string(),
        "[V4+ Styles]".to_string(),
        "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding".to_string(),
        format!(
            "Style: Original,{},{},{},&H000000FF,{},{},0,0,0,0,100,100,0,0,1,{},{},2,{},{},{},1",
            config.original_font_family,
            original_font_size,
            ass_color(&config.original_color),
            ass_color(&config.outline_color),
            ass_alpha_color(&config.shadow_color, 0x64),
            config.outline_width,
            shadow,
            config.horizontal_margin,
            config.horizontal_margin,
            original_margin_v,
        ),
        format!(
            "Style: Reading,{},{},{},&H000000FF,{},{},0,0,0,0,100,100,0,0,1,{},{},2,{},{},{},1",
            config.reading_font_family,
            reading_font_size,
            ass_color(&config.reading_color),
            ass_color(&config.outline_color),
            ass_alpha_color(&config.shadow_color, 0x64),
            config.outline_width,
            shadow,
            config.horizontal_margin,
            config.horizontal_margin,
            reading_margin_v,
        ),
        format!(
            "Style: Translation,{},{},{},&H000000FF,{},{},0,0,0,0,100,100,0,0,1,{},{},2,{},{},{},1",
            config.translation_font_family,
            translation_font_size,
            ass_color(&config.translation_color),
            ass_color(&config.outline_color),
            ass_alpha_color(&config.shadow_color, 0x64),
            config.outline_width,
            shadow,
            config.horizontal_margin,
            config.horizontal_margin,
            translation_margin_v,
        ),
        "".to_string(),
        "[Events]".to_string(),
        "Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text".to_string(),
    ];

    for segment in &article.segments {
        let (Some(start_time), Some(end_time)) = (segment.start_time, segment.end_time) else {
            continue;
        };

        if !matches!(config.display_mode, KtvDisplayMode::Translation) {
            lines.push(format!(
                "Dialogue: 0,{},{},Original,,0,0,0,,{}",
                format_ass_time(start_time),
                format_ass_time(end_time),
                build_original_ass_text(
                    &segment.text,
                    segment.reading_text.as_deref(),
                    segment.explanation.as_ref(),
                    config,
                    reading_font_size,
                ),
            ));
        }

        if matches!(
            config.display_mode,
            KtvDisplayMode::Bilingual | KtvDisplayMode::Translation
        ) {
            if let Some(translation_text) = &segment.translation {
                lines.push(format!(
                    "Dialogue: 0,{},{},Translation,,0,0,0,,{}",
                    format_ass_time(start_time),
                    format_ass_time(end_time),
                    escape_ass_text(translation_text),
                ));
            }
        }
    }

    Ok(lines.join("\n"))
}

fn ass_play_resolution(config: &KtvExportConfig) -> (u32, u32) {
    match (config.video_width, config.video_height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => (width, height),
        _ => (1920, 1080),
    }
}

pub fn build_ktv_ffmpeg_args(input: &Path, ass_path: &Path, output: &Path) -> Vec<String> {
    vec![
        "-y".to_string(),
        "-i".to_string(),
        input.display().to_string(),
        "-vf".to_string(),
        format!("ass=filename='{}'", format_ass_filter_path(ass_path)),
        "-c:v".to_string(),
        "libx264".to_string(),
        "-c:a".to_string(),
        "aac".to_string(),
        output.display().to_string(),
    ]
}

pub async fn export_ktv_video(
    app_handle: &AppHandle,
    article: &Article,
    config: &KtvExportConfig,
    output_path: &Path,
) -> Result<KtvExportResult, String> {
    let media_path = article
        .media_path
        .as_deref()
        .ok_or_else(|| "仅视频素材支持 KTV 导出".to_string())?;
    let input_path = Path::new(media_path);

    if !input_path.exists() {
        return Err("视频文件不存在".to_string());
    }

    if let Some(parent_dir) = output_path.parent() {
        fs::create_dir_all(parent_dir).map_err(|error| format!("创建导出目录失败: {error}"))?;
    }

    let ass_content = generate_ktv_ass(article, config)?;
    let ass_path = std::env::temp_dir().join(format!("openkoto-ktv-{}.ass", article.id));
    fs::write(&ass_path, ass_content).map_err(|error| format!("写入 ASS 文件失败: {error}"))?;

    let args = build_ktv_ffmpeg_args(input_path, &ass_path, output_path);
    let output =
        run_ffmpeg_with_requirement(app_handle, args, FfmpegRequirement::SubtitleBurn).await?;

    let _ = fs::remove_file(&ass_path);

    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 导出失败: {stderr}"));
    }

    ensure_ktv_output_created(output_path)?;

    Ok(KtvExportResult {
        output_path: output_path.display().to_string(),
    })
}

pub fn ensure_ktv_output_created(output_path: &Path) -> Result<(), String> {
    let metadata = fs::metadata(output_path).map_err(|_| {
        format!(
            "FFmpeg 未生成导出文件: {}。请确认 FFmpeg sidecar 可用。",
            output_path.display()
        )
    })?;

    if metadata.len() == 0 {
        return Err(format!(
            "FFmpeg 生成的导出文件为空: {}",
            output_path.display()
        ));
    }

    Ok(())
}

fn format_ass_time(seconds: f64) -> String {
    let total_centiseconds = (seconds * 100.0).round() as u64;
    let hours = total_centiseconds / 360_000;
    let minutes = (total_centiseconds % 360_000) / 6_000;
    let secs = (total_centiseconds % 6_000) / 100;
    let centis = total_centiseconds % 100;
    format!("{hours}:{minutes:02}:{secs:02}.{centis:02}")
}

fn escape_ass_text(text: &str) -> String {
    text.replace('\\', r"\\")
        .replace('{', r"\{")
        .replace('}', r"\}")
}

fn build_original_ass_text(
    text: &str,
    reading_text: Option<&str>,
    explanation: Option<&crate::types::SegmentExplanation>,
    config: &KtvExportConfig,
    reading_font_size: u32,
) -> String {
    if let Some(parts) = build_vocabulary_inline_reading_parts(text, explanation) {
        let mut rendered = String::new();

        for part in parts {
            match part {
                InlineReadingPart::Plain(text) => rendered.push_str(&escape_ass_text(&text)),
                InlineReadingPart::Annotated { text, reading } => {
                    rendered.push_str(&escape_ass_text(&text));
                    rendered.push_str(&format!(
                        "{{\\fn{}\\fs{}\\1c{}}}{}{{\\rOriginal}}",
                        config.reading_font_family,
                        reading_font_size,
                        ass_tag_color(&config.reading_color),
                        escape_ass_text(&format!("（{}）", reading)),
                    ));
                }
            }
        }

        return rendered;
    }

    let original_text = escape_ass_text(text);

    if !config.show_reading {
        return original_text;
    }

    let Some(inline_reading) = format_inline_reading(text, reading_text) else {
        return original_text;
    };

    format!(
        "{}{{\\fn{}\\fs{}\\1c{}}}{}{{\\rOriginal}}",
        original_text,
        config.reading_font_family,
        reading_font_size,
        ass_tag_color(&config.reading_color),
        escape_ass_text(&inline_reading),
    )
}

enum InlineReadingPart {
    Plain(String),
    Annotated { text: String, reading: String },
}

fn build_vocabulary_inline_reading_parts(
    text: &str,
    explanation: Option<&crate::types::SegmentExplanation>,
) -> Option<Vec<InlineReadingPart>> {
    let mut candidates: Vec<(String, String)> = explanation
        .into_iter()
        .flat_map(|item| item.vocabulary.iter())
        .filter_map(|item| {
            let word = item.word.trim();
            let reading = item.reading.as_deref()?.trim();

            if word.is_empty() || reading.is_empty() || word == reading {
                return None;
            }

            Some((word.to_string(), reading.to_string()))
        })
        .collect();

    if candidates.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    let mut cursor = 0usize;

    while cursor < text.len() {
        let mut best_match: Option<(usize, usize, String, String)> = None;

        for (index, (word, reading)) in candidates.iter().enumerate() {
            let Some(relative_start) = text[cursor..].find(word) else {
                continue;
            };
            let start = cursor + relative_start;

            let replace = match &best_match {
                Some((_, current_start, current_word, _)) => {
                    start < *current_start
                        || (start == *current_start && word.len() > current_word.len())
                }
                None => true,
            };

            if replace {
                best_match = Some((index, start, word.clone(), reading.clone()));
            }
        }

        let Some((candidate_index, start, word, reading)) = best_match else {
            break;
        };

        if start > cursor {
            parts.push(InlineReadingPart::Plain(text[cursor..start].to_string()));
        }

        cursor = start + word.len();
        parts.push(InlineReadingPart::Annotated {
            text: word,
            reading,
        });
        candidates.remove(candidate_index);
    }

    if cursor < text.len() {
        parts.push(InlineReadingPart::Plain(text[cursor..].to_string()));
    }

    if parts
        .iter()
        .any(|part| matches!(part, InlineReadingPart::Annotated { .. }))
    {
        Some(parts)
    } else {
        None
    }
}

fn format_inline_reading(text: &str, reading_text: Option<&str>) -> Option<String> {
    let normalized_text = text.trim();
    let normalized_reading = reading_text?.trim();

    if normalized_text.is_empty()
        || normalized_reading.is_empty()
        || normalized_text == normalized_reading
    {
        return None;
    }

    Some(format!("（{}）", normalized_reading))
}

fn ass_color(color: &str) -> String {
    let normalized = color.trim().trim_start_matches('#');
    if normalized.len() != 6 {
        return "&H00FFFFFF".to_string();
    }

    let r = &normalized[0..2];
    let g = &normalized[2..4];
    let b = &normalized[4..6];
    format!("&H00{b}{g}{r}")
}

fn ass_tag_color(color: &str) -> String {
    let normalized = color.trim().trim_start_matches('#');
    if normalized.len() != 6 {
        return "&HFFFFFF&".to_string();
    }

    let r = &normalized[0..2];
    let g = &normalized[2..4];
    let b = &normalized[4..6];
    format!("&H{b}{g}{r}&")
}

fn ass_alpha_color(color: &str, alpha: u8) -> String {
    let normalized = color.trim().trim_start_matches('#');
    if normalized.len() != 6 {
        return "&H64FFFFFF".to_string();
    }

    let r = &normalized[0..2];
    let g = &normalized[2..4];
    let b = &normalized[4..6];
    format!("&H{alpha:02X}{b}{g}{r}")
}

fn position_base_margin(config: &KtvExportConfig) -> u32 {
    match config.position_preset {
        KtvPositionPreset::Bottom => config.bottom_margin,
        KtvPositionPreset::LowerThird => config.bottom_margin.saturating_add(120),
        KtvPositionPreset::CenterLower => config.bottom_margin.saturating_add(220),
    }
}

fn translation_margin_v(config: &KtvExportConfig) -> u32 {
    position_base_margin(config)
}

fn original_margin_v(config: &KtvExportConfig, translation_font_size: u32) -> u32 {
    if matches!(config.display_mode, KtvDisplayMode::Bilingual) {
        position_base_margin(config)
            .saturating_add(translation_font_size)
            .saturating_add(config.bilingual_gap)
    } else {
        position_base_margin(config)
    }
}

fn reading_margin_v(
    config: &KtvExportConfig,
    original_margin_v: u32,
    original_font_size: u32,
) -> u32 {
    original_margin_v
        .saturating_add(original_font_size)
        .saturating_add(config.line_gap)
}

fn shadow_value(config: &KtvExportConfig) -> u32 {
    if !config.shadow_enabled {
        return 0;
    }

    let offset = config
        .shadow_offset_x
        .unsigned_abs()
        .max(config.shadow_offset_y.unsigned_abs());
    offset.max(config.shadow_blur.max(1))
}

fn format_ass_filter_path(path: &Path) -> String {
    let normalized = path.display().to_string().replace('\\', "/");
    #[cfg(target_os = "windows")]
    {
        return normalized.replacen(':', r"\:", 1).replace('\'', r"\'");
    }
    normalized.replace('\'', r"\'")
}
