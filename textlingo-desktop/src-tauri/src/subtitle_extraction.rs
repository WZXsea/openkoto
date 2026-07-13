// 字幕提取模块
// 使用 Gemini 多模态 API 从视频中提取字幕
//
// 工作流程:
// 1. 使用 FFmpeg 从视频中提取音频 (MP3 格式)
// 2. 将音频文件编码为 base64
// 3. 发送至 Gemini API 进行转录
// 4. 解析转录结果为 ArticleSegment

use crate::ai_service::AIService;
use crate::ffmpeg::run_ffmpeg;
use crate::moonshot::{is_moonshot_provider, moonshot_chat_completions_url};
use crate::types::{
    ArticleSegment, ChatContent, ChatMessage, ChatRequest, ContentPart, TranscriptionResult,
    TranscriptionSegment, VideoUrl,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use chrono::Utc;
use reqwest::Client;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri::Emitter;
use uuid::Uuid;

// API 端点
const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";
const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const API_302AI_URL: &str = "https://api.302.ai/v1/chat/completions";
const GOOGLE_GEMINI_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// 从视频中提取字幕的主函数
///
/// # 参数
/// - `app`: Tauri 应用句柄
/// - `video_path`: 视频文件路径
/// - `video_id`: 视频 ID (用于生成 segment ID)
/// - `provider`: API 提供商 ("openrouter", "302ai", "google")
/// - `api_key`: API 密钥
/// - `model`: 模型名称
///
/// # 返回
/// - 成功: Vec<ArticleSegment> 字幕段落列表
/// - 失败: 错误信息
pub async fn extract_subtitles(
    app: AppHandle,
    video_path: &Path,
    video_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    base_url: Option<&str>,
    use_asr: bool,
    event_id: &str,
) -> Result<Vec<ArticleSegment>, String> {
    println!("[SubtitleExtraction] 开始提取字幕: {:?}", video_path);

    // 发送开始事件
    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({ "phase": "start", "message": "开始提取字幕..." }),
    );

    // 1. 获取视频时长
    let duration = get_video_duration(&app, video_path).await?;
    println!(
        "[SubtitleExtraction] 视频时长: {:.1} 秒 ({:.1} 分钟)",
        duration,
        duration / 60.0
    );

    // 字幕转写(ASR / Whisper)路径 —— 优先于旧的 LLM 听写
    if use_asr {
        println!("[SubtitleExtraction] 使用 ASR(Whisper)转写路径");
        return extract_subtitles_with_whisper(
            app, video_path, video_id, provider, api_key, model, base_url, duration, event_id,
        )
        .await;
    }

    // 分片提取阈值：10分钟
    const CHUNK_THRESHOLD_SECONDS: f64 = 10.0 * 60.0;

    // Kimi K2.5 / K2.6 视频理解模式
    if is_moonshot_provider(provider) && (model.contains("k2.5") || model.contains("k2.6")) {
        println!("[SubtitleExtraction] 检测到 Kimi K2.5/K2.6 模型，启用视频理解模式");
        let _ = app.emit(&format!("subtitle-extraction-progress://{}", event_id), 
            serde_json::json!({ "phase": "processing", "message": "正在使用 Kimi 视频理解模式..." }));

        return extract_subtitles_with_kimi(
            app, video_path, video_id, provider, api_key, model, event_id,
        )
        .await;
    }

    if duration > CHUNK_THRESHOLD_SECONDS {
        println!("[SubtitleExtraction] 视频超过 10 分钟，启用分片提取模式");
        let _ = app.emit(
            &format!("subtitle-extraction-progress://{}", event_id),
            serde_json::json!({ "phase": "chunked", "message": "视频较长，启用分片提取模式..." }),
        );
        return extract_subtitles_chunked(
            app, video_path, video_id, provider, api_key, model, base_url, duration, event_id,
        )
        .await;
    }

    // 原有逻辑：短视频直接提取
    println!("[SubtitleExtraction] 视频较短，使用标准提取模式");
    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({ "phase": "audio", "message": "提取音频中..." }),
    );

    // 2. 从视频中提取完整音频
    let audio_path = extract_audio_from_video(&app, video_path).await?;
    println!("[SubtitleExtraction] 音频提取完成: {:?}", audio_path);

    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({ "phase": "transcribe", "message": "转录音频中..." }),
    );

    // 3. 调用 Gemini API 进行转录
    let transcription =
        transcribe_audio_with_gemini(&audio_path, provider, api_key, model, base_url).await?;
    println!(
        "[SubtitleExtraction] 转录完成，共 {} 个片段",
        transcription.segments.len()
    );

    // 4. 转换为 ArticleSegment
    let segments = transcription_to_segments(&transcription, video_id);

    // 5. 清理临时音频文件
    if let Err(e) = fs::remove_file(&audio_path) {
        println!("[SubtitleExtraction] 清理临时音频文件失败: {}", e);
    }

    let _ = app.emit(&format!("subtitle-extraction-progress://{}", event_id), 
        serde_json::json!({ "phase": "done", "message": "字幕提取完成！", "count": segments.len() }));

    Ok(segments)
}

/// 获取视频时长（秒）
///
/// 使用 FFmpeg 获取视频的精确时长（通过解析 stderr 输出）
async fn get_video_duration(app: &AppHandle, video_path: &Path) -> Result<f64, String> {
    let video_path_str = video_path.to_str().ok_or("无效的视频文件路径")?;

    // 使用 FFmpeg 获取时长
    // 运行 FFmpeg 但不产生输出，从 stderr 解析时长信息
    // FFmpeg 会在 stderr 中输出类似 "Duration: 00:25:30.50" 的信息
    let output = run_ffmpeg(
        app,
        vec![
            "-i".to_string(),
            video_path_str.to_string(),
            "-f".to_string(),
            "null".to_string(),
            "-".to_string(),
        ],
    )
    .await?;

    // FFmpeg 即使成功也会返回非0状态码（因为我们没有真正输出）
    // 所以我们直接解析 stderr
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 查找 Duration 行，格式: "Duration: HH:MM:SS.ms"
    for line in stderr.lines() {
        if line.contains("Duration:") {
            // 示例: "  Duration: 00:25:30.50, start: 0.000000, bitrate: 1234 kb/s"
            if let Some(duration_part) = line.split("Duration:").nth(1) {
                if let Some(time_str) = duration_part.split(',').next() {
                    let time_str = time_str.trim();
                    // 解析 HH:MM:SS.ms 格式
                    return parse_ffmpeg_duration(time_str);
                }
            }
        }
    }

    Err(format!(
        "无法从 FFmpeg 输出中解析视频时长。stderr: {}",
        stderr.chars().take(500).collect::<String>()
    ))
}

/// 解析 FFmpeg 时长格式 (HH:MM:SS.ms) 为秒
fn parse_ffmpeg_duration(time_str: &str) -> Result<f64, String> {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("无效的时长格式: {}", time_str));
    }

    let hours: f64 = parts[0]
        .parse()
        .map_err(|_| format!("无法解析小时: {}", parts[0]))?;
    let minutes: f64 = parts[1]
        .parse()
        .map_err(|_| format!("无法解析分钟: {}", parts[1]))?;
    let seconds: f64 = parts[2]
        .parse()
        .map_err(|_| format!("无法解析秒: {}", parts[2]))?;

    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

/// 分片音频提取结果
#[derive(Debug)]
struct ChunkTranscriptionResult {
    /// 转录得到的字幕片段（已调整时间轴）
    segments: Vec<TranscriptionSegment>,
    /// 第一个字幕的开始时间（调整后）
    #[allow(dead_code)]
    first_segment_start: Option<f64>,
    /// 最后一个字幕的结束时间（调整后）
    last_segment_end: Option<f64>,
    /// 时间轴偏移量
    #[allow(dead_code)]
    time_offset: f64,
}

/// 带源分片元数据的字幕：用于重叠区去重时判断"新鲜度"。
///
/// `chunk_offset` 是该段所属分片在整段视频里的全局起始秒数。
/// 对任意一条字幕，`start_time - chunk_offset` 越小，说明它越接近所在分片
/// 的开头——LLM 在音频播放早期的时间戳通常更准确，漂移最小。
#[derive(Debug, Clone)]
struct ChunkedSegment {
    seg: TranscriptionSegment,
    chunk_offset: f64,
}

/// 使用 FFmpeg 从视频中提取指定时间段的音频
///
/// # 参数
/// - `app`: Tauri 应用句柄
/// - `video_path`: 视频文件路径
/// - `start_time`: 起始时间（秒）
/// - `duration`: 提取时长（秒）
/// - `suffix`: 输出文件后缀（用于区分不同片段）
async fn extract_audio_segment(
    app: &AppHandle,
    video_path: &Path,
    start_time: f64,
    duration: f64,
    suffix: &str,
) -> Result<PathBuf, String> {
    let video_dir = video_path.parent().ok_or("无法获取视频目录")?;

    let video_stem = video_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("无法获取视频文件名")?;

    let audio_path = video_dir.join(format!("{}_audio_{}.mp3", video_stem, suffix));
    let audio_path_str = audio_path.to_str().ok_or("无效的音频文件路径")?;
    let video_path_str = video_path.to_str().ok_or("无效的视频文件路径")?;

    // 清理旧文件
    if audio_path.exists() {
        if let Err(e) = fs::remove_file(&audio_path) {
            println!("[SubtitleExtraction] 清理旧音频片段文件失败: {}", e);
        }
    }

    // FFmpeg 参数说明:
    // -ss: 起始时间（放在 -i 前面可以快速定位）
    // -t: 提取时长
    // -ar 44100: 保持44.1kHz采样率以保留语音细节
    // -ab 192k: 192kbps比特率兼顾质量和API文件大小限制
    let output = run_ffmpeg(
        app,
        vec![
            "-ss".to_string(),
            format!("{start_time:.2}"),
            "-i".to_string(),
            video_path_str.to_string(),
            "-t".to_string(),
            format!("{duration:.2}"),
            "-vn".to_string(),
            "-acodec".to_string(),
            "libmp3lame".to_string(),
            "-ab".to_string(),
            "192k".to_string(),
            "-ar".to_string(),
            "44100".to_string(),
            "-ac".to_string(),
            "1".to_string(),
            "-y".to_string(),
            audio_path_str.to_string(),
        ],
    )
    .await?;

    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 音频片段提取失败: {}", stderr));
    }

    if !audio_path.exists() {
        return Err("音频片段文件未生成".to_string());
    }

    Ok(audio_path)
}

/// 提取并转录单个音频片段
///
/// 此函数提取指定时间段的音频，发送至 API 转录，并调整时间轴
async fn extract_and_transcribe_segment(
    app: AppHandle,
    video_path: PathBuf,
    start_time: f64,
    duration: f64,
    suffix: String,
    provider: String,
    api_key: String,
    model: String,
    base_url: Option<String>,
) -> Result<ChunkTranscriptionResult, String> {
    println!(
        "[SubtitleExtraction] 提取片段: start={:.1}s, duration={:.1}s, suffix={}",
        start_time, duration, suffix
    );

    // 1. 提取音频片段
    let audio_path =
        extract_audio_segment(&app, &video_path, start_time, duration, &suffix).await?;

    // 2. 转录音频
    let transcription = transcribe_audio_with_gemini(
        &audio_path,
        &provider,
        &api_key,
        &model,
        base_url.as_deref(),
    )
    .await?;

    // 3. 清理临时音频文件
    if let Err(e) = fs::remove_file(&audio_path) {
        println!("[SubtitleExtraction] 清理临时音频片段失败: {}", e);
    }

    // 4. 调整时间轴（加上偏移量）
    let segments: Vec<TranscriptionSegment> = transcription
        .segments
        .into_iter()
        .map(|mut seg| {
            if let Some(st) = seg.start_time {
                seg.start_time = Some(st + start_time);
            }
            if let Some(et) = seg.end_time {
                seg.end_time = Some(et + start_time);
            }
            seg
        })
        .collect();

    // 5. 获取边界时间
    let first_segment_start = segments.first().and_then(|s| s.start_time);
    let last_segment_end = segments.last().and_then(|s| s.end_time);

    println!(
        "[SubtitleExtraction] 片段 {} 转录完成: {} 个字幕, 时间范围 {:?} - {:?}",
        suffix,
        segments.len(),
        first_segment_start,
        last_segment_end
    );

    Ok(ChunkTranscriptionResult {
        segments,
        first_segment_start,
        last_segment_end,
        time_offset: start_time,
    })
}

/// 分片提取长视频字幕（顺序线性分片策略）
///
/// # 算法说明
/// 1. 将音频按固定步长（5 分钟）顺序切片，相邻片段有 15 秒重叠
/// 2. 每两个相邻片段并发提取，逐步向前推进
/// 3. 合并所有片段后，通过模糊匹配去重消除 overlap 区域的重复字幕；
///    重叠处优先保留"分片开头"那一份，避免 LLM 在长音频尾部累积的时间戳漂移
async fn extract_subtitles_chunked(
    app: AppHandle,
    video_path: &Path,
    video_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    base_url: Option<&str>,
    total_duration: f64,
    event_id: &str,
) -> Result<Vec<ArticleSegment>, String> {
    // 每片 5 分钟：更短的音频上下文显著降低 LLM 在片内累计的时间戳漂移
    // （10 分钟片尾部观感非常明显）。overlap 相应降到 15 秒，够覆盖一句话跨界，
    // 又能减少重复段落进入去重阶段的比例。
    const CHUNK_DURATION: f64 = 5.0 * 60.0; // 每片 5 分钟
    const OVERLAP: f64 = 15.0; // 15 秒重叠
    let step = CHUNK_DURATION - OVERLAP; // 实际步进 = 4 分 45 秒

    // 计算所有片段的起始时间
    let mut chunk_starts: Vec<f64> = Vec::new();
    let mut pos = 0.0;
    while pos < total_duration {
        chunk_starts.push(pos);
        pos += step;
    }
    let total_chunks = chunk_starts.len() as i32;
    let mut completed_chunks = 0;

    println!(
        "[SubtitleExtraction] 顺序分片: 共 {} 个片段, 每片 {:.0}s, 重叠 {:.0}s, 步进 {:.0}s",
        total_chunks, CHUNK_DURATION, OVERLAP, step
    );

    let mut all_segments: Vec<ChunkedSegment> = Vec::new();

    // 两两并发提取
    let mut i = 0;
    while i < chunk_starts.len() {
        // 计算本轮要提取的片段（最多2个并发）
        let start1 = chunk_starts[i];
        let dur1 = (total_duration - start1).min(CHUNK_DURATION);

        if i + 1 < chunk_starts.len() {
            // 并发提取两个片段
            let start2 = chunk_starts[i + 1];
            let dur2 = (total_duration - start2).min(CHUNK_DURATION);
            let chunk1_offset = start1;
            let chunk2_offset = start2;

            let _ = app.emit(
                &format!("subtitle-extraction-progress://{}", event_id),
                serde_json::json!({
                    "phase": "chunk",
                    "message": format!("提取片段 {}-{}/{}", i+1, i+2, total_chunks),
                    "current": completed_chunks,
                    "total": total_chunks
                }),
            );

            let (r1, r2) = tokio::join!(
                extract_and_transcribe_segment(
                    app.clone(),
                    video_path.to_path_buf(),
                    start1,
                    dur1,
                    format!("chunk_{}", i),
                    provider.to_string(),
                    api_key.to_string(),
                    model.to_string(),
                    base_url.map(str::to_string),
                ),
                extract_and_transcribe_segment(
                    app.clone(),
                    video_path.to_path_buf(),
                    start2,
                    dur2,
                    format!("chunk_{}", i + 1),
                    provider.to_string(),
                    api_key.to_string(),
                    model.to_string(),
                    base_url.map(str::to_string),
                )
            );

            all_segments.extend(r1?.segments.into_iter().map(|s| ChunkedSegment {
                seg: s,
                chunk_offset: chunk1_offset,
            }));
            all_segments.extend(r2?.segments.into_iter().map(|s| ChunkedSegment {
                seg: s,
                chunk_offset: chunk2_offset,
            }));
            completed_chunks += 2;
            i += 2;
        } else {
            // 奇数片段，单独提取
            let _ = app.emit(
                &format!("subtitle-extraction-progress://{}", event_id),
                serde_json::json!({
                    "phase": "chunk",
                    "message": format!("提取片段 {}/{}", i+1, total_chunks),
                    "current": completed_chunks,
                    "total": total_chunks
                }),
            );

            let r = extract_and_transcribe_segment(
                app.clone(),
                video_path.to_path_buf(),
                start1,
                dur1,
                format!("chunk_{}", i),
                provider.to_string(),
                api_key.to_string(),
                model.to_string(),
                base_url.map(str::to_string),
            )
            .await?;

            all_segments.extend(r.segments.into_iter().map(|s| ChunkedSegment {
                seg: s,
                chunk_offset: start1,
            }));
            completed_chunks += 1;
            i += 1;
        }

        let _ = app.emit(&format!("subtitle-extraction-progress://{}", event_id),
            serde_json::json!({
                "phase": "chunk",
                "message": format!("已完成 {}/{} 片段", completed_chunks.min(total_chunks), total_chunks),
                "current": completed_chunks.min(total_chunks),
                "total": total_chunks
            }));
    }

    // === 合并、排序、去重 ===
    println!(
        "[SubtitleExtraction] === 合并排序去重: {} 个原始字幕 ===",
        all_segments.len()
    );
    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({
            "phase": "merge",
            "message": "合并排序去重中..."
        }),
    );

    // 过滤掉没有时间戳的字幕
    all_segments.retain(|c| c.seg.start_time.is_some() && c.seg.end_time.is_some());

    // 按时间排序（保证 dedup 遇到重复时更早出现的版本先进入结果集，并保证
    // 最终输出按时间有序；dedup 内部会再做一次兜底排序）
    all_segments.sort_by(|a, b| {
        let a_time = a.seg.start_time.unwrap_or(0.0);
        let b_time = b.seg.start_time.unwrap_or(0.0);
        a_time
            .partial_cmp(&b_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 去重：移除时间重叠且内容相似的字幕，并在重复时优先保留"更新鲜"的那一份
    let deduped_segments = deduplicate_segments(all_segments);

    println!(
        "[SubtitleExtraction] 分片提取完成，共 {} 个字幕片段",
        deduped_segments.len()
    );

    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({
            "phase": "done",
            "message": "字幕提取完成！",
            "count": deduped_segments.len()
        }),
    );

    // 转换为 ArticleSegment
    let result = TranscriptionResult {
        segments: deduped_segments,
        full_text: String::new(),
    };

    Ok(transcription_to_segments(&result, video_id))
}

/// 计算两个字符串的相似度 (基于最长公共子序列, 0.0-1.0)
fn text_similarity(a: &str, b: &str) -> f64 {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    // LCS 用两行滚动数组节省内存
    let mut prev = vec![0u32; n + 1];
    let mut curr = vec![0u32; n + 1];

    for i in 1..=m {
        for j in 1..=n {
            if a_chars[i - 1] == b_chars[j - 1] {
                curr[j] = prev[j - 1] + 1;
            } else {
                curr[j] = prev[j].max(curr[j - 1]);
            }
        }
        std::mem::swap(&mut prev, &mut curr);
        curr.iter_mut().for_each(|x| *x = 0);
    }

    let lcs_len = prev[n] as f64;
    let max_len = m.max(n) as f64;
    lcs_len / max_len
}

/// 判断候选字幕是否与已保留的某一条"在同一事件"（overlap 去重用）。
///
/// 返回命中已有条目的下标；如果没有命中返回 None。
fn find_duplicate_index(result: &[ChunkedSegment], cand: &ChunkedSegment) -> Option<usize> {
    let cand_start = cand.seg.start_time.unwrap_or(0.0);
    let cand_end = cand.seg.end_time.unwrap_or(cand_start);
    let cand_duration = (cand_end - cand_start).max(0.1);

    for (idx, existing) in result.iter().enumerate() {
        let ex_start = existing.seg.start_time.unwrap_or(0.0);

        // 快速排除：起始时间差超过15秒不可能是同一句
        if (cand_start - ex_start).abs() > 15.0 {
            continue;
        }

        let ex_end = existing.seg.end_time.unwrap_or(ex_start);
        let overlap_start = cand_start.max(ex_start);
        let overlap_end = cand_end.min(ex_end);
        let overlap_duration = (overlap_end - overlap_start).max(0.0);
        let overlap_ratio = overlap_duration / cand_duration;

        // 条件1: 时间重叠 > 30% 且内容相似度 > 60%
        if overlap_ratio > 0.3 && text_similarity(&cand.seg.content, &existing.seg.content) > 0.6 {
            return Some(idx);
        }

        // 条件2: 起始时间非常接近（< 5秒）且内容高度相似
        if (cand_start - ex_start).abs() < 5.0
            && text_similarity(&cand.seg.content, &existing.seg.content) > 0.5
        {
            return Some(idx);
        }
    }

    None
}

/// 去除重复的字幕片段。
///
/// 判断标准（针对分片 overlap 区域优化）：
/// 1. 时间接近（起始时间差 < 15秒）
/// 2. 内容相似度 > 60%（基于 LCS）
///
/// 当检测到重复时，**保留"更新鲜"的那一份**——即所在分片越晚开始、且字幕越靠
/// 近该分片开头的版本。度量方式是 `start_time - chunk_offset`，值越小表明字幕
/// 来自对应分片的前段，LLM 尚未在长音频中累积时间戳漂移。
fn deduplicate_segments(segments: Vec<ChunkedSegment>) -> Vec<TranscriptionSegment> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut result: Vec<ChunkedSegment> = Vec::new();

    for cand in segments {
        match find_duplicate_index(&result, &cand) {
            Some(idx) => {
                let cand_freshness = cand.seg.start_time.unwrap_or(0.0) - cand.chunk_offset;
                let existing_freshness =
                    result[idx].seg.start_time.unwrap_or(0.0) - result[idx].chunk_offset;
                // 新鲜度小 => 离所在分片起点更近 => 时间轴更可信
                if cand_freshness < existing_freshness {
                    result[idx] = cand;
                }
            }
            None => result.push(cand),
        }
    }

    // 替换可能打乱按起始时间的顺序，最终再排一次
    result.sort_by(|a, b| {
        a.seg
            .start_time
            .unwrap_or(0.0)
            .partial_cmp(&b.seg.start_time.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    result.into_iter().map(|c| c.seg).collect()
}

/// 使用 FFmpeg 从视频中提取音频
///
/// 输出格式: MP3 (Gemini 支持的格式)
/// 输出位置: 与视频同目录，文件名为 {video_name}_audio.mp3
async fn extract_audio_from_video(app: &AppHandle, video_path: &Path) -> Result<PathBuf, String> {
    let video_dir = video_path.parent().ok_or("无法获取视频目录")?;

    let video_stem = video_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("无法获取视频文件名")?;

    let audio_path = video_dir.join(format!("{}_audio.mp3", video_stem));
    let audio_path_str = audio_path.to_str().ok_or("无效的音频文件路径")?;
    let video_path_str = video_path.to_str().ok_or("无效的视频文件路径")?;

    // 检查是否已存在音频文件（之前提取过但未清理）
    if audio_path.exists() {
        if let Err(e) = fs::remove_file(&audio_path) {
            println!("[SubtitleExtraction] 清理旧音频文件失败: {}", e);
        }
    }

    // 使用 FFmpeg 提取音频
    // 参数说明:
    // -i: 输入文件
    // -vn: 不处理视频流
    // -acodec libmp3lame: 使用 MP3 编码器
    // -ab 192k: 192kbps 保留语音细节
    // -ar 44100: 44.1kHz 采样率保留完整频率信息
    // -ac 1: 单声道
    // -y: 覆盖已存在的文件
    let output = run_ffmpeg(
        app,
        vec![
            "-i".to_string(),
            video_path_str.to_string(),
            "-vn".to_string(),
            "-acodec".to_string(),
            "libmp3lame".to_string(),
            "-ab".to_string(),
            "192k".to_string(),
            "-ar".to_string(),
            "44100".to_string(),
            "-ac".to_string(),
            "1".to_string(),
            "-y".to_string(),
            audio_path_str.to_string(),
        ],
    )
    .await?;

    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 音频提取失败: {}", stderr));
    }

    // 验证音频文件已创建
    if !audio_path.exists() {
        return Err("音频文件未生成".to_string());
    }

    Ok(audio_path)
}

// ============================================================================
// ASR (Whisper) 转写路径 —— 走 OpenAI 兼容 /audio/transcriptions
// 用真正的语音识别替代「LLM 听写」，时间戳更准、长音频更稳。
// ============================================================================

/// Whisper /audio/transcriptions 单文件上限（OpenAI/302 为 25MB，留点余量）
const MAX_ASR_BYTES: u64 = 24 * 1024 * 1024;

/// 解析 ASR provider 的 /audio/transcriptions 端点
fn asr_transcriptions_url(provider: &str, base_url: Option<&str>) -> String {
    if let Some(b) = base_url {
        let b = b.trim().trim_end_matches('/');
        if !b.is_empty() {
            if b.ends_with("/audio/transcriptions") {
                return b.to_string();
            }
            return format!("{}/audio/transcriptions", b);
        }
    }
    let base = match provider {
        "openai" => "https://api.openai.com/v1",
        "groq" => "https://api.groq.com/openai/v1",
        "siliconflow" => "https://api.siliconflow.cn/v1",
        _ => "https://api.302.ai/v1", // 302ai 及兜底
    };
    format!("{}/audio/transcriptions", base)
}

/// 从视频抽取「为 ASR 优化」的音频：单声道 16kHz 32kbps mp3。
/// 32kbps 下 25MB ≈ 100 分钟，绝大多数视频可一次过。
/// `start`/`dur` 为 None 时抽完整音频；否则抽 [start, start+dur] 片段。
async fn extract_audio_for_asr(
    app: &AppHandle,
    video_path: &Path,
    suffix: &str,
    start: Option<f64>,
    dur: Option<f64>,
) -> Result<PathBuf, String> {
    let video_dir = video_path.parent().ok_or("无法获取视频目录")?;
    let video_stem = video_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("无法获取视频文件名")?;
    let audio_path = video_dir.join(format!("{}_asr_{}.mp3", video_stem, suffix));
    let audio_path_str = audio_path.to_str().ok_or("无效的音频文件路径")?;
    let video_path_str = video_path.to_str().ok_or("无效的视频文件路径")?;

    if audio_path.exists() {
        let _ = fs::remove_file(&audio_path);
    }

    let mut args: Vec<String> = Vec::new();
    if let Some(s) = start {
        args.push("-ss".to_string());
        args.push(format!("{:.2}", s));
    }
    args.push("-i".to_string());
    args.push(video_path_str.to_string());
    if let Some(d) = dur {
        args.push("-t".to_string());
        args.push(format!("{:.2}", d));
    }
    args.extend(
        [
            "-vn",
            "-acodec",
            "libmp3lame",
            "-ab",
            "32k",
            "-ar",
            "16000",
            "-ac",
            "1",
            "-y",
            audio_path_str,
        ]
        .iter()
        .map(|s| s.to_string()),
    );

    let output = run_ffmpeg(app, args).await?;
    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 音频提取失败: {}", stderr));
    }
    if !audio_path.exists() {
        return Err("音频文件未生成".to_string());
    }
    Ok(audio_path)
}

/// 一个词（whisper word 级时间戳）
struct AsrWord {
    text: String,
    start: f64,
    end: f64,
}

/// 是否含 CJK（中日韩）字符 —— 决定分行的字符上限/不加空格
fn contains_cjk(s: &str) -> bool {
    s.chars().any(|c| {
        let u = c as u32;
        (0x3040..=0x30FF).contains(&u)      // 平假名/片假名
            || (0x4E00..=0x9FFF).contains(&u) // CJK 统一表意
            || (0x3400..=0x4DBF).contains(&u) // CJK 扩展 A
            || (0xAC00..=0xD7AF).contains(&u) // 谚文（韩）
    })
}

/// 一句是否以句末标点结束（硬断句）
fn ends_sentence(s: &str) -> bool {
    matches!(
        s.trim_end().chars().last(),
        Some('.') | Some('?') | Some('!') | Some('。') | Some('？') | Some('！') | Some('…')
    )
}

/// 把 word 级时间戳重新切成「可读字幕块」：
/// 限制每块字符数 / 时长，在句末标点、长停顿处断句。
/// 这是字幕质量的关键 —— 直接用 whisper 的 segment 往往一行过长、在短语中间断。
fn segment_words_into_cues(words: &[AsrWord], time_offset: f64) -> Vec<TranscriptionSegment> {
    const MAX_DURATION: f64 = 6.0; // 单块最长 6 秒
    const PAUSE_GAP: f64 = 0.7; // 词间停顿 > 0.7s 视为自然边界

    let full: String = words.iter().map(|w| w.text.as_str()).collect();
    let is_cjk = contains_cjk(&full);
    let max_chars: usize = if is_cjk { 20 } else { 42 };
    let joiner = if is_cjk { "" } else { " " };

    let cue_text = |ws: &[&AsrWord]| -> String {
        ws.iter()
            .map(|w| w.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(joiner)
    };

    let mut cues: Vec<TranscriptionSegment> = Vec::new();
    let mut cur: Vec<&AsrWord> = Vec::new();

    let flush = |cur: &mut Vec<&AsrWord>, cues: &mut Vec<TranscriptionSegment>| {
        if cur.is_empty() {
            return;
        }
        let text = cur
            .iter()
            .map(|w| w.text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(
                if contains_cjk(&cur.iter().map(|w| w.text.as_str()).collect::<String>()) {
                    ""
                } else {
                    " "
                },
            );
        if !text.is_empty() {
            cues.push(TranscriptionSegment {
                speaker: None,
                content: text,
                start_time: Some(cur.first().unwrap().start + time_offset),
                end_time: Some(cur.last().unwrap().end + time_offset),
            });
        }
        cur.clear();
    };

    for (i, w) in words.iter().enumerate() {
        // 加入前判断长度/时长是否超限（当前非空时才另起一块）
        if !cur.is_empty() {
            let mut tentative: Vec<&AsrWord> = cur.clone();
            tentative.push(w);
            let over_len = cue_text(&tentative).chars().count() > max_chars;
            let over_dur = w.end - cur.first().unwrap().start > MAX_DURATION;
            if over_len || over_dur {
                flush(&mut cur, &mut cues);
            }
        }
        cur.push(w);

        // 句末标点 → 硬断
        if ends_sentence(&w.text) {
            flush(&mut cur, &mut cues);
            continue;
        }
        // 与下一个词间停顿过长 → 自然断
        if let Some(next) = words.get(i + 1) {
            if next.start - w.end > PAUSE_GAP {
                flush(&mut cur, &mut cues);
            }
        }
    }
    flush(&mut cur, &mut cues);
    cues
}

/// 调用 Whisper /audio/transcriptions（OpenAI 兼容，multipart）转写单个音频文件。
/// 优先用 word 级时间戳重新分行；没有 word 时退回 segment。
/// `time_offset` 会加到每段时间戳上（用于分片拼接）。
async fn transcribe_audio_with_whisper(
    audio_path: &Path,
    endpoint: &str,
    api_key: &str,
    model: &str,
    time_offset: f64,
) -> Result<TranscriptionResult, String> {
    let bytes = fs::read(audio_path).map_err(|e| format!("读取音频失败: {}", e))?;
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name("audio.mp3")
        .mime_str("audio/mpeg")
        .map_err(|e| format!("构造 multipart 失败: {}", e))?;
    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", model.to_string())
        .text("response_format", "verbose_json")
        .text("timestamp_granularities[]", "segment")
        .text("timestamp_granularities[]", "word");

    // 显式超时：单片 ~10 分钟音频请求一般 1 分钟内返回，给足 5 分钟兜底；
    // 卡住时干净失败，而不是无限等待。
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| format!("构造 HTTP 客户端失败: {}", e))?;
    let response = client
        .post(endpoint)
        .bearer_auth(api_key)
        .multipart(form)
        .send()
        .await
        .map_err(|e| format!("请求转写接口失败: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(format!("转写接口错误 ({}): {}", status, error_text));
    }

    let v: Value = response
        .json()
        .await
        .map_err(|e| format!("解析转写响应失败: {}", e))?;

    let mut segments: Vec<TranscriptionSegment> = Vec::new();

    // 优先：用 word 级时间戳重新分行（字幕可读性更好）
    if let Some(warr) = v["words"].as_array() {
        let words: Vec<AsrWord> = warr
            .iter()
            .filter_map(|w| {
                let text = w["word"].as_str()?.to_string();
                let start = w["start"].as_f64()?;
                let end = w["end"].as_f64()?;
                if text.trim().is_empty() {
                    return None;
                }
                Some(AsrWord { text, start, end })
            })
            .collect();
        if !words.is_empty() {
            segments = segment_words_into_cues(&words, time_offset);
        }
    }

    // 退回：没有 word 时用 segment
    if segments.is_empty() {
        if let Some(arr) = v["segments"].as_array() {
            for s in arr {
                let text = s["text"].as_str().unwrap_or("").trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let start = s["start"].as_f64();
                let end = s["end"].as_f64();
                segments.push(TranscriptionSegment {
                    speaker: None,
                    content: text,
                    start_time: start.map(|t| t + time_offset),
                    end_time: end.map(|t| t + time_offset),
                });
            }
        }
    }

    // 没有 segments（例如 SiliconFlow 的 SenseVoice 只回纯文本）→ 报清晰错误
    if segments.is_empty() {
        let plain = v["text"].as_str().unwrap_or("").trim();
        if plain.is_empty() {
            return Err("转写返回为空".to_string());
        }
        return Err(
            "该转写模型未返回时间戳（segments），无法生成字幕。请改用支持时间戳的模型（如 302ai whisper-1）。"
                .to_string(),
        );
    }

    let full_text = v["text"].as_str().unwrap_or("").trim().to_string();
    Ok(TranscriptionResult {
        segments,
        full_text,
    })
}

/// ASR 字幕提取主流程：压音频 → (必要时分片) → Whisper 转写 → 拼接。
async fn extract_subtitles_with_whisper(
    app: AppHandle,
    video_path: &Path,
    video_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    base_url: Option<&str>,
    duration: f64,
    event_id: &str,
) -> Result<Vec<ArticleSegment>, String> {
    let endpoint = asr_transcriptions_url(provider, base_url);

    // 302 的 /audio/transcriptions 目前只支持 whisper-1；其它模型名（如 whisper-large-v3，
    // 实为 Groq 的模型）会返回 500。这里对 302 统一纠正成 whisper-1，避免用户填错就卡死。
    let model: &str = if provider == "302ai" && model != "whisper-1" {
        println!(
            "[SubtitleExtraction][ASR] 302 仅支持 whisper-1，已将 '{}' 自动纠正为 whisper-1",
            model
        );
        "whisper-1"
    } else {
        model
    };

    println!(
        "[SubtitleExtraction][ASR] provider={} model={} endpoint={}",
        provider, model, endpoint
    );

    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({ "phase": "audio", "message": "提取音频中..." }),
    );

    // 单片最长时长：太长的单次请求容易被 302/代理超时断开（499）。控制在 ~10 分钟，
    // 单次请求约 1 分钟，稳定很多。
    const MAX_CHUNK_SECS: f64 = 600.0;

    // 先压完整音频，拿到大小
    let full_audio = extract_audio_for_asr(&app, video_path, "full", None, None).await?;
    let size = fs::metadata(&full_audio).map(|m| m.len()).unwrap_or(0);

    // 分片数 = max(按时长, 按大小)，两者都要满足
    let parts_by_dur = if duration > 0.0 {
        (duration / MAX_CHUNK_SECS).ceil() as usize
    } else {
        1
    };
    let parts_by_size = ((size as f64) / (MAX_ASR_BYTES as f64)).ceil() as usize;
    let parts = parts_by_dur.max(parts_by_size).max(1);

    let mut all_segments: Vec<TranscriptionSegment> = Vec::new();

    if parts <= 1 {
        // 一次过
        let _ = app.emit(
            &format!("subtitle-extraction-progress://{}", event_id),
            serde_json::json!({ "phase": "transcribe", "message": "转录音频中..." }),
        );
        let result =
            transcribe_audio_with_whisper(&full_audio, &endpoint, api_key, model, 0.0).await?;
        all_segments = result.segments;
        let _ = fs::remove_file(&full_audio);
    } else {
        // 分片：按时长均分（无 overlap，Whisper 段本身干净）
        let _ = fs::remove_file(&full_audio);
        let chunk_dur = duration / parts as f64;
        println!(
            "[SubtitleExtraction][ASR] 时长 {:.1} 分钟 / {:.1}MB，分 {} 段（每段 ~{:.1} 分钟）",
            duration / 60.0,
            size as f64 / 1024.0 / 1024.0,
            parts,
            chunk_dur / 60.0
        );
        for i in 0..parts {
            let start = i as f64 * chunk_dur;
            let _ = app.emit(
                &format!("subtitle-extraction-progress://{}", event_id),
                serde_json::json!({
                    "phase": "chunk",
                    "message": format!("转录片段 {}/{}", i + 1, parts),
                    "current": i + 1,
                    "total": parts,
                }),
            );
            let seg_audio = extract_audio_for_asr(
                &app,
                video_path,
                &i.to_string(),
                Some(start),
                Some(chunk_dur),
            )
            .await?;
            let result =
                transcribe_audio_with_whisper(&seg_audio, &endpoint, api_key, model, start).await?;
            all_segments.extend(result.segments);
            let _ = fs::remove_file(&seg_audio);
        }
    }

    let transcription = TranscriptionResult {
        full_text: all_segments
            .iter()
            .map(|s| s.content.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        segments: all_segments,
    };
    let segments = transcription_to_segments(&transcription, video_id);
    println!(
        "[SubtitleExtraction][ASR] 转写完成，共 {} 个片段",
        segments.len()
    );

    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({ "phase": "done", "message": "字幕提取完成！", "count": segments.len() }),
    );

    Ok(segments)
}

/// 使用 Kimi K2.5 模型提取字幕 (视频理解 - 使用 Base64 内嵌视频)
async fn extract_subtitles_with_kimi(
    app: AppHandle,
    video_path: &Path,
    video_id: &str,
    provider: &str,
    api_key: &str,
    model: &str,
    event_id: &str,
) -> Result<Vec<ArticleSegment>, String> {
    // 1. 压缩视频 (至 480p, CRF 28 以减小体积，便于 Base64 编码)
    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({ "phase": "compress", "message": "正在优化视频体积..." }),
    );

    let compressed_path = compress_video_for_upload(&app, video_path).await?;
    println!("[SubtitleExtraction] 视频压缩完成: {:?}", compressed_path);

    // 2. 读取压缩后的视频并 Base64 编码
    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({ "phase": "encode", "message": "正在编码视频数据..." }),
    );

    let video_bytes = fs::read(&compressed_path).map_err(|e| format!("读取压缩视频失败: {}", e))?;

    let video_size_mb = video_bytes.len() as f64 / 1024.0 / 1024.0;
    println!(
        "[SubtitleExtraction] 压缩后视频大小: {:.2} MB",
        video_size_mb
    );

    // 获取视频扩展名用于 MIME 类型
    let ext = compressed_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("mp4")
        .to_lowercase();

    // 构建 data URL: data:video/{ext};base64,{base64_data}
    let video_base64 = BASE64.encode(&video_bytes);
    let video_data_url = format!("data:video/{};base64,{}", ext, video_base64);

    // 清理本地压缩文件
    if let Err(e) = fs::remove_file(&compressed_path) {
        println!("[SubtitleExtraction] 警告: 清理临时视频文件失败: {}", e);
    }

    // 3. 发送转录请求
    let _ = app.emit(
        &format!("subtitle-extraction-progress://{}", event_id),
        serde_json::json!({ "phase": "analyze", "message": "Kimi 正在分析视频生成字幕..." }),
    );

    let prompt = r#"请分析视频中的语音内容，并生成带时间轴的字幕。
严格按照以下 JSON 格式返回结果：
{
  "segments": [
    {
      "start": "MM:SS",
      "end": "MM:SS",
      "content": "字幕内容"
    }
  ],
  "full_text": "全文内容"
}
要求：
1. 精确对应语音时间。
2. 按句子或短语断句。
3. 保持原语言，不要翻译。
4. 忽略背景音和无意义语气词。
"#;

    let ai_service = AIService::new(api_key.to_string(), provider.to_string(), model.to_string());

    let chat_request = ChatRequest {
        model: model.to_string(),
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: ChatContent::Parts(vec![
                ContentPart {
                    part_type: "video_url".to_string(),
                    text: None,
                    image_url: None,
                    file_data: None,
                    video_url: Some(VideoUrl {
                        url: video_data_url, // 使用 Base64 data URL
                    }),
                },
                ContentPart {
                    part_type: "text".to_string(),
                    text: Some(prompt.to_string()),
                    image_url: None,
                    file_data: None,
                    video_url: None,
                },
            ]),
        }],
        temperature: pick_transcription_temperature(provider, model).map(|v| v as f32),
    };

    let response = match ai_service.chat(chat_request).await {
        Ok(res) => res,
        Err(e) => return Err(format!("Kimi 分析失败: {}", e)),
    };

    // 4. 解析结果
    let transcription = parse_transcription_response(&response.content)?;

    // 5. 转换为 ArticleSegment
    let segments = transcription_to_segments(&transcription, video_id);

    let _ = app.emit(&format!("subtitle-extraction-progress://{}", event_id), 
        serde_json::json!({ "phase": "done", "message": "字幕提取完成！", "count": segments.len() }));

    Ok(segments)
}

/// 根据 provider/model 选择转录请求里该带的 `temperature`。
///
/// 返回 `None` 表示不带该字段（留给 API 走默认值）——用于兜底未来某模型拒绝任何
/// 显式取值的情况。当前所有已知路径都有明确取值：
///
/// - Kimi K2.5 / K2.6：模型强制要求 `temperature=1`。
/// - Google Gemini：允许 0.0，贪心解码对转录时间戳最稳定。
/// - 其他（openai / openrouter / 302ai / 非-K2.5/K2.6 kimi 等走 OpenAI 兼容接口）：
///   沿用历史值 0.1，低随机但保留一点探索以避免极端退化。
fn pick_transcription_temperature(provider: &str, model: &str) -> Option<f64> {
    if is_moonshot_provider(provider) && (model.contains("k2.5") || model.contains("k2.6")) {
        return Some(1.0);
    }
    match provider {
        "google" | "google-ai-studio" => Some(0.0),
        _ => Some(0.1),
    }
}

/// 压缩视频以便上传
/// 目标: 480p, CRF 28, Preset veryfast
async fn compress_video_for_upload(app: &AppHandle, video_path: &Path) -> Result<PathBuf, String> {
    let video_dir = video_path.parent().ok_or("无效的视频目录")?;
    let video_stem = video_path
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or("无效的文件名")?;
    let output_path = video_dir.join(format!("{}_compressed.mp4", video_stem));

    if output_path.exists() {
        let _ = fs::remove_file(&output_path);
    }

    let output = run_ffmpeg(
        app,
        vec![
            "-i".to_string(),
            video_path.to_str().unwrap().to_string(),
            "-vf".to_string(),
            "scale=-2:480".to_string(),
            "-c:v".to_string(),
            "libx264".to_string(),
            "-crf".to_string(),
            "28".to_string(),
            "-preset".to_string(),
            "veryfast".to_string(),
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "128k".to_string(),
            "-ac".to_string(),
            "1".to_string(),
            "-y".to_string(),
            output_path.to_str().unwrap().to_string(),
        ],
    )
    .await?;

    if !output.success {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 压缩错误: {}", stderr));
    }

    if !output_path.exists() {
        return Err("压缩后的视频文件未生成".to_string());
    }

    Ok(output_path)
}

/// 使用 Gemini API 转录音频
///
/// 支持的 API 提供商:
/// - openrouter: OpenRouter API (使用 input_audio 格式)
/// - 302ai: 302.AI API (兼容 OpenAI 格式)
/// - google: Google Gemini 直接 API
async fn transcribe_audio_with_gemini(
    audio_path: &Path,
    provider: &str,
    api_key: &str,
    model: &str,
    base_url: Option<&str>,
) -> Result<TranscriptionResult, String> {
    const MAX_RETRIES: u32 = 3;
    let mut retry_count = 0;

    loop {
        // 读取并编码音频文件 (每次重试都重新读取可能没必要，但为了安全起见暂时不改这里)
        let audio_bytes = fs::read(audio_path).map_err(|e| format!("读取音频文件失败: {}", e))?;

        let audio_base64 = BASE64.encode(&audio_bytes);
        let audio_size_mb = audio_bytes.len() as f64 / 1024.0 / 1024.0;
        println!("[SubtitleExtraction] 音频文件大小: {:.2} MB", audio_size_mb);

        // 注意：20MB 限制现在由分片提取算法处理，此处不再需要检查

        // 转录提示词 - 强调时间戳精度和按句子断句
        let transcription_prompt = r#"Transcribe this audio into text with precise timestamps. Return strictly in the following JSON format.

Requirements:
1. **Sentence-level segmentation**: Each segment contains exactly ONE complete sentence. Do NOT merge multiple sentences.
2. Split at sentence-ending punctuation (periods, question marks, exclamation marks) or natural speech pauses.
3. Each sentence should be roughly 5-30 characters/words. Never exceed 50.
4. **Timestamp accuracy is critical**: start and end times MUST precisely match when the speech actually begins and ends in the audio. Listen carefully to the exact timing.
5. Format: MM:SS.mmm or HH:MM:SS.mmm with millisecond precision (e.g., "01:23.456" means 1 minute 23 seconds and 456 milliseconds). The `.mmm` fractional part is REQUIRED — do NOT round to whole seconds. Both start and end are required.
6. Keep the original language. Do NOT translate.
7. Timestamps must be monotonically increasing — each segment's start must be >= the previous segment's end.

Return format:
{
  "segments": [
    {
      "start": "00:00.000",
      "end": "00:03.420",
      "content": "First sentence of the audio.",
      "speaker": null
    },
    {
      "start": "00:03.420",
      "end": "00:06.180",
      "content": "Second sentence of the audio.",
      "speaker": null
    }
  ],
  "full_text": "Full transcription text..."
}

IMPORTANT: Each segment = one sentence. Timestamps MUST include milliseconds (the `.mmm` part). Integer-second timestamps like "00:03" are NOT acceptable — use "00:03.000" or the exact sub-second value.
"#;

        let client = Client::new();

        // 根据提供商选择不同的 API 格式
        let response = match provider {
            "google" | "google-ai-studio" => {
                // Google Gemini 直接 API
                let url = format!(
                    "{}/{}:generateContent?key={}",
                    GOOGLE_GEMINI_URL,
                    model.strip_prefix("models/").unwrap_or(model),
                    api_key
                );

                let mut generation_config = serde_json::json!({
                    "response_mime_type": "application/json"
                });
                if let Some(temp) = pick_transcription_temperature(provider, model) {
                    generation_config["temperature"] = serde_json::json!(temp);
                }
                let request_body = json!({
                    "contents": [{
                        "parts": [
                            {
                                "inline_data": {
                                    "mime_type": "audio/mp3",
                                    "data": audio_base64
                                }
                            },
                            {
                                "text": transcription_prompt
                            }
                        ]
                    }],
                    "generationConfig": generation_config
                });

                client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(|e| format!("API 请求失败: {}", e))?
            }
            _ => {
                // OpenAI 兼容格式：优先使用用户配置的 base_url，避免错误回退到固定网关
                let api_url = if let Some(custom_base_url) =
                    base_url.and_then(|url| (!url.trim().is_empty()).then_some(url))
                {
                    let trimmed = custom_base_url.trim_end_matches('/');
                    if trimmed.ends_with("/chat/completions") {
                        trimmed.to_string()
                    } else {
                        format!("{}/chat/completions", trimmed)
                    }
                } else {
                    match provider {
                        "openrouter" => OPENROUTER_API_URL.to_string(),
                        "302ai" => API_302AI_URL.to_string(),
                        provider if is_moonshot_provider(provider) => {
                            moonshot_chat_completions_url(provider).ok_or_else(|| {
                                format!(
                                    "Unsupported provider '{}' for subtitle transcription without base_url",
                                    provider
                                )
                            })?
                        }
                        "openai" => OPENAI_API_URL.to_string(),
                        "openai-compatible" => {
                            return Err("openai-compatible provider requires base_url in settings"
                                .to_string());
                        }
                        _ => {
                            return Err(format!(
                                "Unsupported provider '{}' for subtitle transcription without base_url",
                                provider
                            ));
                        }
                    }
                };

                // 使用 OpenAI 兼容的 input_audio 格式
                let mut request_body = json!({
                    "model": model,
                    "messages": [{
                        "role": "user",
                        "content": [
                            {
                                "type": "input_audio",
                                "input_audio": {
                                    "data": audio_base64,
                                    "format": "mp3"
                                }
                            },
                            {
                                "type": "text",
                                "text": transcription_prompt
                            }
                        ]
                    }]
                });
                if let Some(temp) = pick_transcription_temperature(provider, model) {
                    request_body["temperature"] = serde_json::json!(temp);
                }

                client
                    .post(&api_url)
                    .header("Authorization", format!("Bearer {}", api_key))
                    .header("Content-Type", "application/json")
                    .json(&request_body)
                    .send()
                    .await
                    .map_err(|e| format!("API 请求失败: {}", e))?
            }
        };

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("API 错误: {}", error_text));
        }

        let response_json: Value = response
            .json()
            .await
            .map_err(|e| format!("解析响应失败: {}", e))?;

        // 提取响应内容
        let content = if provider == "google" || provider == "google-ai-studio" {
            // Google API 响应格式
            response_json["candidates"][0]["content"]["parts"][0]["text"]
                .as_str()
                .unwrap_or("")
                .to_string()
        } else {
            // OpenAI 兼容格式
            response_json["choices"][0]["message"]["content"]
                .as_str()
                .unwrap_or("")
                .to_string()
        };

        // 解析转录结果
        match parse_transcription_response(&content) {
            Ok(result) => return Ok(result),
            Err(e) => {
                println!("[SubtitleExtraction] JSON 解析失败: {}", e);
                println!("[SubtitleExtraction] 尝试解析的原始内容: {}", content);

                retry_count += 1;
                if retry_count >= MAX_RETRIES {
                    // 最后一次尝试失败，如果是解析错误且内容不为空，可能是格式问题
                    // 但如果内容为空，已经在 parse_transcription_response 中处理了
                    return Err(format!("多次重试后仍然失败: {}", e));
                }

                println!(
                    "[SubtitleExtraction] 将进行第 {} 次重试...",
                    retry_count + 1
                );
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }
        }
    } // end loop
}

/// 解析转录 API 响应
/// 解析转录 API 响应
fn parse_transcription_response(content: &str) -> Result<TranscriptionResult, String> {
    // 0. 处理空内容
    if content.trim().is_empty() {
        println!("[SubtitleExtraction] 警告: API 返回内容为空，视为空音频处理");
        return Ok(TranscriptionResult {
            segments: Vec::new(),
            full_text: String::new(),
        });
    }

    // 尝试提取 JSON
    let mut json_str = extract_json(content);

    // 如果提取出的 JSON 为空
    if json_str.trim().is_empty() {
        println!("[SubtitleExtraction] 警告: 无法提取有效 JSON，尝试直接解析原始内容");
        // 尝试直接解析内容，也许内容本身就是 JSON
        if let Ok(parsed) = serde_json::from_str::<Value>(content) {
            if parsed.get("segments").is_some() {
                // 内容本身就是有效的 JSON
                json_str = content.to_string();
            } else {
                println!("[SubtitleExtraction] 警告: 内容是 JSON 但没有 segments 字段，视为空音频");
                return Ok(TranscriptionResult {
                    segments: Vec::new(),
                    full_text: String::new(),
                });
            }
        } else {
            println!("[SubtitleExtraction] 警告: 内容不是 JSON 且无法提取，视为空音频");
            return Ok(TranscriptionResult {
                segments: Vec::new(),
                full_text: String::new(),
            });
        }
    }

    // 解析 JSON
    let parsed: Value = serde_json::from_str(&json_str).map_err(|e| {
        format!(
            "JSON 解析失败: {}. \n提取的JSON: {}\n原始响应: {}",
            e, json_str, content
        )
    })?;

    // 提取 segments
    let segments = parsed["segments"]
        .as_array()
        .ok_or("响应中没有 segments 字段")?
        .iter()
        .filter_map(|seg| {
            // 支持 "start"/"end" 或旧格式 "timestamp"
            let start_str = seg["start"].as_str().or(seg["timestamp"].as_str())?;
            let end_str = seg["end"].as_str().unwrap_or(start_str);

            let start_time = parse_time_str(start_str);
            let end_time = parse_time_str(end_str);

            Some(TranscriptionSegment {
                speaker: seg["speaker"].as_str().map(|s| s.to_string()),
                // timestamp removed as per user request
                content: seg["content"].as_str()?.to_string(),
                start_time: Some(start_time),
                end_time: Some(end_time),
            })
        })
        .collect();

    let full_text = parsed["full_text"].as_str().unwrap_or("").to_string();

    Ok(TranscriptionResult {
        segments,
        full_text,
    })
}

/// 将 MM:SS 或 HH:MM:SS 格式字符串解析为秒数
fn parse_time_str(time_str: &str) -> f64 {
    let parts: Vec<&str> = time_str.split(':').collect();
    if parts.len() == 2 {
        let min: f64 = parts[0].parse().unwrap_or(0.0);
        let sec: f64 = parts[1].parse().unwrap_or(0.0);
        min * 60.0 + sec
    } else if parts.len() == 3 {
        let h: f64 = parts[0].parse().unwrap_or(0.0);
        let m: f64 = parts[1].parse().unwrap_or(0.0);
        let s: f64 = parts[2].parse().unwrap_or(0.0);
        h * 3600.0 + m * 60.0 + s
    } else {
        0.0
    }
}

/// 从响应中提取 JSON 字符串
fn extract_json(content: &str) -> String {
    // 1. 尝试找 markdown 代码块
    if let Some(start) = content.find("```json") {
        if let Some(end) = content[start..].rfind("```") {
            if end > 7 {
                return content[start + 7..start + end].trim().to_string();
            }
        }
    }

    // 2. 尝试找通用代码块
    if let Some(start) = content.find("```") {
        if let Some(end_offset) = content[start + 3..].find("```") {
            let end = start + 3 + end_offset;
            return content[start + 3..end].trim().to_string();
        }
    }

    // 3. 尝试找大括号 (Generic find first '{' and last '}')
    if let Some(start) = content.find('{') {
        if let Some(end) = content.rfind('}') {
            if end > start {
                return content[start..=end].to_string();
            }
        }
    }

    content.trim().to_string()
}

/// 将转录结果转换为 ArticleSegment
fn transcription_to_segments(
    transcription: &TranscriptionResult,
    article_id: &str,
) -> Vec<ArticleSegment> {
    transcription
        .segments
        .iter()
        .enumerate()
        .map(|(i, seg)| ArticleSegment {
            id: Uuid::new_v4().to_string(),
            article_id: article_id.to_string(),
            order: i as i32,
            text: seg.content.clone(),
            reading_text: None,
            translation: None,
            explanation: None,
            start_time: seg.start_time,
            end_time: seg.end_time,
            created_at: Utc::now().to_rfc3339(),
            is_new_paragraph: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, end: f64) -> AsrWord {
        AsrWord {
            text: text.to_string(),
            start,
            end,
        }
    }

    #[test]
    fn test_segment_words_breaks_on_sentence_end() {
        let words = vec![
            w("Hello", 0.0, 0.4),
            w("world.", 0.4, 0.9),
            w("How", 1.0, 1.2),
            w("are", 1.2, 1.4),
            w("you?", 1.4, 1.8),
        ];
        let cues = segment_words_into_cues(&words, 0.0);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].content, "Hello world.");
        assert_eq!(cues[0].start_time, Some(0.0));
        assert_eq!(cues[0].end_time, Some(0.9));
        assert_eq!(cues[1].content, "How are you?");
    }

    #[test]
    fn test_segment_words_breaks_on_long_pause() {
        let words = vec![
            w("first", 0.0, 0.4),
            w("part", 0.4, 0.8),
            // 1.5s 停顿
            w("second", 2.3, 2.7),
            w("part", 2.7, 3.0),
        ];
        let cues = segment_words_into_cues(&words, 0.0);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].content, "first part");
        assert_eq!(cues[1].content, "second part");
    }

    #[test]
    fn test_segment_words_caps_line_length() {
        // 一长串无标点无停顿的词，应按 42 字符上限切成多块
        let words: Vec<AsrWord> = (0..20)
            .map(|i| w("toomanywords", i as f64 * 0.3, i as f64 * 0.3 + 0.3))
            .collect();
        let cues = segment_words_into_cues(&words, 0.0);
        assert!(cues.len() > 1);
        assert!(cues.iter().all(|c| c.content.chars().count() <= 42));
    }

    #[test]
    fn test_segment_words_time_offset_applied() {
        let words = vec![w("a", 0.0, 0.2), w("b.", 0.2, 0.4)];
        let cues = segment_words_into_cues(&words, 100.0);
        assert_eq!(cues[0].start_time, Some(100.0));
        assert_eq!(cues[0].end_time, Some(100.4));
    }

    #[test]
    fn test_contains_cjk() {
        assert!(contains_cjk("こんにちは"));
        assert!(contains_cjk("你好world"));
        assert!(!contains_cjk("hello world"));
    }

    #[test]
    fn test_extract_json_from_markdown() {
        let content = r#"Here is the transcription:
```json
{"segments": [{"timestamp": "00:00", "content": "Hello"}], "full_text": "Hello"}
```
"#;
        let json = extract_json(content);
        assert!(json.contains("segments"));
    }

    #[test]
    fn test_extract_json_plain() {
        let content =
            r#"{"segments": [{"start": "00:00", "content": "Test"}], "full_text": "Test"}"#;
        let json = extract_json(content);
        assert_eq!(json, content);
    }

    #[test]
    fn test_parse_transcription_response() {
        // Test with new format (start/end)
        let content = r#"{"segments": [{"start": "00:00", "end": "00:05", "content": "Hello world", "speaker": null}], "full_text": "Hello world"}"#;
        let result = parse_transcription_response(content).unwrap();
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].content, "Hello world");
        assert_eq!(result.segments[0].start_time, Some(0.0));
        assert_eq!(result.segments[0].end_time, Some(5.0));
    }

    #[test]
    fn test_parse_time_str() {
        assert_eq!(parse_time_str("00:00"), 0.0);
        assert_eq!(parse_time_str("00:05"), 5.0);
        assert_eq!(parse_time_str("01:00"), 60.0);
        assert_eq!(parse_time_str("01:02:03"), 3723.0);
    }

    fn make_seg(content: &str, start: f64, end: f64) -> TranscriptionSegment {
        TranscriptionSegment {
            speaker: None,
            content: content.to_string(),
            start_time: Some(start),
            end_time: Some(end),
        }
    }

    #[test]
    fn test_dedup_prefers_fresh_chunk_over_drifted_chunk() {
        // Same sentence appears at the boundary between two chunks:
        // - chunk 0 (offset=0.0): speech occurs 9.5 min into this chunk → LLM timing drift ~1s.
        // - chunk 1 (offset=570.0): same speech is right at the start → fresh, no drift.
        // Sort-by-start-time puts the drifted one first. Current behavior keeps
        // whichever comes first; desired behavior keeps the one whose source chunk
        // started latest (freshest relative to its own chunk origin).
        let drifted = ChunkedSegment {
            seg: make_seg("Hello there my dear friend", 569.2, 571.2), // drifted -0.8s
            chunk_offset: 0.0,
        };
        let fresh = ChunkedSegment {
            seg: make_seg("Hello there my dear friend", 570.0, 572.0), // accurate
            chunk_offset: 570.0,
        };

        let kept = deduplicate_segments(vec![drifted, fresh]);

        assert_eq!(kept.len(), 1, "the two should be deduped to one");
        let s = &kept[0];
        assert!(
            (s.start_time.unwrap() - 570.0).abs() < 0.01,
            "expected fresh chunk's timestamp 570.0s (from chunk whose offset is 570.0), got {:?}",
            s.start_time
        );
    }

    #[test]
    fn test_dedup_keeps_non_duplicates() {
        let a = ChunkedSegment {
            seg: make_seg("first sentence", 0.0, 2.0),
            chunk_offset: 0.0,
        };
        let b = ChunkedSegment {
            seg: make_seg("completely different", 3.0, 5.0),
            chunk_offset: 0.0,
        };
        let kept = deduplicate_segments(vec![a, b]);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn test_pick_temperature_kimi_k25_must_be_one() {
        // Kimi K2.5 (video understanding mode) only accepts temperature=1.
        assert_eq!(
            pick_transcription_temperature("moonshot", "kimi-k2.5-preview"),
            Some(1.0)
        );
        assert_eq!(
            pick_transcription_temperature("moonshot-cn", "kimi-k2.5"),
            Some(1.0)
        );
        assert_eq!(
            pick_transcription_temperature("moonshot-global", "kimi-k2.5-latest"),
            Some(1.0)
        );
    }

    #[test]
    fn test_pick_temperature_kimi_k26_must_be_one() {
        // Kimi K2.6 behaves like K2.5: video understanding + temperature=1 only.
        assert_eq!(
            pick_transcription_temperature("moonshot-cn", "kimi-k2.6"),
            Some(1.0)
        );
        assert_eq!(
            pick_transcription_temperature("moonshot-global", "kimi-k2.6-latest"),
            Some(1.0)
        );
    }

    #[test]
    fn test_pick_temperature_google_gemini_is_zero() {
        // Gemini accepts 0.0 and greedy decoding gives most deterministic timestamps.
        assert_eq!(
            pick_transcription_temperature("google", "gemini-2.5-flash"),
            Some(0.0)
        );
        assert_eq!(
            pick_transcription_temperature("google-ai-studio", "gemini-2.0-flash"),
            Some(0.0)
        );
    }

    #[test]
    fn test_pick_temperature_non_kimi_moonshot_is_low() {
        // Non-K2.5 Kimi models go through the OpenAI-compatible path and accept
        // low-but-nonzero temperatures; don't force 1.0 on them.
        assert_eq!(
            pick_transcription_temperature("moonshot", "kimi-latest"),
            Some(0.1)
        );
    }

    #[test]
    fn test_pick_temperature_default_providers_are_low() {
        assert_eq!(
            pick_transcription_temperature("openai", "gpt-4o-audio-preview"),
            Some(0.1)
        );
        assert_eq!(
            pick_transcription_temperature("openrouter", "openai/gpt-4o"),
            Some(0.1)
        );
        assert_eq!(
            pick_transcription_temperature("302ai", "any-model"),
            Some(0.1)
        );
    }

    #[test]
    fn test_parse_time_str_with_milliseconds() {
        assert!((parse_time_str("00:00.500") - 0.5).abs() < 1e-6);
        assert!((parse_time_str("00:01.250") - 1.25).abs() < 1e-6);
        assert!((parse_time_str("01:23.456") - 83.456).abs() < 1e-6);
        assert!((parse_time_str("00:01:23.456") - 83.456).abs() < 1e-6);
        assert!((parse_time_str("01:02:03.999") - 3723.999).abs() < 1e-6);
    }
}
