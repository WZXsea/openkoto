use crate::types::{Article, ArticleSegment};
use regex::Regex;
use std::fs;
use std::path::Path;
use uuid::Uuid;

fn parse_srt_timestamp(ts: &str) -> Option<f64> {
    let normalized = ts.trim().replace(',', ".");
    let parts: Vec<&str> = normalized.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;

    Some(hours * 3600.0 + minutes * 60.0 + seconds)
}

pub fn parse_srt_content(content: &str, article_id: &str) -> Result<Vec<ArticleSegment>, String> {
    let normalized = content.replace("\r\n", "\n");
    let blocks: Vec<&str> = normalized
        .split("\n\n")
        .map(str::trim)
        .filter(|block| !block.is_empty())
        .collect();
    let time_regex =
        Regex::new(r"(\d{2}:\d{2}:\d{2}[,.]\d{3})\s*-->\s*(\d{2}:\d{2}:\d{2}[,.]\d{3})")
            .map_err(|e| format!("Failed to compile subtitle regex: {}", e))?;
    let tag_regex =
        Regex::new(r"<[^>]+>").map_err(|e| format!("Failed to compile tag regex: {}", e))?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut segments = Vec::new();

    for block in blocks {
        let lines: Vec<&str> = block.lines().map(str::trim).collect();
        let Some(time_index) = lines.iter().position(|line| line.contains("-->")) else {
            continue;
        };

        let Some(caps) = time_regex.captures(lines[time_index]) else {
            continue;
        };

        let start_time = parse_srt_timestamp(&caps[1]);
        let end_time = parse_srt_timestamp(&caps[2]);
        let text = lines
            .iter()
            .skip(time_index + 1)
            .copied()
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let text = tag_regex.replace_all(&text, "").trim().to_string();

        if text.is_empty() {
            continue;
        }

        segments.push(ArticleSegment {
            id: Uuid::new_v4().to_string(),
            article_id: article_id.to_string(),
            order: segments.len() as i32,
            text,
            text_sha256: None,
            reading_text: None,
            translation: None,
            explanation: None,
            start_time,
            end_time,
            created_at: now.clone(),
            is_new_paragraph: true,
        });
    }

    if segments.is_empty() {
        return Err("未能从字幕文件中解析到有效字幕".to_string());
    }

    Ok(segments)
}

pub fn build_content_from_segments(segments: &[ArticleSegment]) -> String {
    segments
        .iter()
        .map(|segment| segment.text.trim())
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_srt_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("字幕文件不存在".to_string());
    }

    let is_srt = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("srt"))
        .unwrap_or(false);

    if !is_srt {
        return Err("仅支持导入 .srt 字幕文件".to_string());
    }

    Ok(())
}

pub fn load_segments_from_srt(
    path: &Path,
    article_id: &str,
) -> Result<Vec<ArticleSegment>, String> {
    validate_srt_path(path)?;
    let content =
        fs::read_to_string(path).map_err(|e| format!("Failed to read subtitle file: {}", e))?;
    parse_srt_content(&content, article_id)
}

pub fn import_subtitles_into_article(article: &mut Article, path: &Path) -> Result<(), String> {
    let segments = load_segments_from_srt(path, &article.id)?;
    article.content = build_content_from_segments(&segments);
    article.segments = segments;
    Ok(())
}

pub fn create_article_from_srt(path: &Path, title: Option<String>) -> Result<Article, String> {
    validate_srt_path(path)?;
    let article_id = Uuid::new_v4().to_string();
    let segments = load_segments_from_srt(path, &article_id)?;
    let created_at = chrono::Utc::now().to_rfc3339();
    let default_title = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("subtitles")
        .to_string();

    Ok(Article {
        id: article_id,
        title: title.unwrap_or(default_title),
        content: build_content_from_segments(&segments),
        source_type: Some("article".to_string()),
        source_url: Some(format!("file://{}", path.to_string_lossy())),
        media_path: None,
        book_path: None,
        book_type: None,
        created_at,
        translated: false,
        active_mind_map_artifact_id: None,
        segments,
        metadata: serde_json::json!({}),
        tags: Vec::new(),
        reading_progress: None,
        archived_at: None,
    })
}
