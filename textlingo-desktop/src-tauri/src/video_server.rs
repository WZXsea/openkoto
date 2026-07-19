// 本地 HTTP 视频流服务器
// 使用 warp 框架提供视频文件，完美支持 Range 请求
// 这是解决 macOS WebKit 自定义协议视频播放问题的终极方案

use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;
use uuid::Uuid;
use warp::http::{Response, StatusCode};
use warp::hyper::Body;
use warp::Filter;

use crate::platform::safe_paths::{resolve_existing_child_path, SafePathError};

/// 视频服务器端口（固定使用一个不太常用的端口）
pub const VIDEO_SERVER_PORT: u16 = 19420;

static RESOURCE_SESSION_TOKEN: OnceLock<String> = OnceLock::new();

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResourceServerInfo {
    pub base_url: String,
    pub token: String,
}

pub fn resource_session_token() -> String {
    RESOURCE_SESSION_TOKEN
        .get_or_init(|| Uuid::new_v4().simple().to_string())
        .clone()
}

pub fn resource_server_info() -> ResourceServerInfo {
    ResourceServerInfo {
        base_url: format!("http://127.0.0.1:{VIDEO_SERVER_PORT}"),
        token: resource_session_token(),
    }
}

#[tauri::command]
pub async fn get_resource_server_info_cmd() -> Result<ResourceServerInfo, String> {
    Ok(resource_server_info())
}

pub(crate) fn validate_resource_token(provided: &str, expected: &str) -> bool {
    !provided.is_empty() && provided == expected
}

/// 启动资源服务器（在后台运行）
/// 提供视频和书籍文件的本地访问
pub async fn start_resource_server(app_data_dir: PathBuf) -> Result<(), String> {
    let app_data_dir = Arc::new(app_data_dir);
    let session_token = Arc::new(resource_session_token());

    // 视频目录: app_data_dir/videos
    let videos_dir_filter = {
        let dir = app_data_dir.join("videos");
        warp::any().map(move || Arc::new(dir.clone()))
    };

    // 书籍目录: app_data_dir/books
    let books_dir_filter = {
        let dir = app_data_dir.join("books");
        warp::any().map(move || Arc::new(dir.clone()))
    };

    let token_filter = {
        let token = session_token.clone();
        warp::any().map(move || token.clone())
    };

    // GET /resource/{token}/video/{filename}
    let video_route = warp::path("resource")
        .and(warp::path::param::<String>())
        .and(warp::path("video"))
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::header::optional::<String>("range"))
        .and(videos_dir_filter)
        .and(token_filter.clone())
        .and_then(serve_file);

    // GET /resource/{token}/book/{filename}
    let book_route = warp::path("resource")
        .and(warp::path::param::<String>())
        .and(warp::path("book"))
        .and(warp::path::param::<String>())
        .and(warp::path::end())
        .and(warp::header::optional::<String>("range"))
        .and(books_dir_filter)
        .and(token_filter)
        .and_then(serve_file);

    // CORS 支持（仅允许 Tauri WebView 和本地开发服务器）
    let cors = warp::cors()
        .allow_origins(vec![
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
            "http://localhost:1420",
            "http://127.0.0.1:1420",
        ])
        .allow_methods(vec!["GET", "HEAD", "OPTIONS"])
        .allow_headers(vec!["range", "content-type"]);

    let routes = video_route.or(book_route).with(cors);

    // 在后台启动服务器
    tokio::spawn(async move {
        println!("[ResourceServer] Starting on port {}", VIDEO_SERVER_PORT);
        warp::serve(routes)
            .run(([127, 0, 0, 1], VIDEO_SERVER_PORT))
            .await;
    });

    Ok(())
}

/// 提供文件（支持 Range 请求）
/// 通用于视频和书籍
async fn serve_file(
    provided_token: String,
    filename: String,
    range_header: Option<String>,
    base_dir: Arc<PathBuf>,
    expected_token: Arc<String>,
) -> Result<impl warp::Reply, warp::Rejection> {
    if !validate_resource_token(&provided_token, expected_token.as_str()) {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from("Invalid resource token"))
            .unwrap());
    }

    // URL 解码文件名
    let decoded_filename = urlencoding::decode(&filename)
        .map(|s| s.to_string())
        .unwrap_or(filename);

    let file_path = match resolve_resource_file_path(base_dir.as_ref(), &decoded_filename) {
        Ok(path) => path,
        Err(error) => {
            let status = if error.is_not_found() {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::FORBIDDEN
            };
            println!(
                "[ResourceServer] Rejected resource path {:?}: {}",
                decoded_filename, error
            );
            return Ok(Response::builder()
                .status(status)
                .body(Body::empty())
                .unwrap());
        }
    };

    // 打开文件
    let mut file = match File::open(&file_path).await {
        Ok(f) => f,
        Err(e) => {
            println!("[ResourceServer] File not found: {:?} ({})", file_path, e);
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("File not found"))
                .unwrap());
        }
    };

    let metadata = match file.metadata().await {
        Ok(m) => m,
        Err(_) => {
            return Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::empty())
                .unwrap());
        }
    };

    let file_size = metadata.len();

    // 确定 Content-Type
    let content_type = if decoded_filename.ends_with(".mp4") {
        "video/mp4"
    } else if decoded_filename.ends_with(".webm") {
        "video/webm"
    } else if decoded_filename.ends_with(".mkv") {
        "video/x-matroska"
    } else if decoded_filename.ends_with(".mp3") {
        "audio/mpeg"
    } else if decoded_filename.ends_with(".wav") {
        "audio/wav"
    } else if decoded_filename.ends_with(".m4a") {
        "audio/mp4"
    } else if decoded_filename.ends_with(".aac") {
        "audio/aac"
    } else if decoded_filename.ends_with(".flac") {
        "audio/flac"
    } else if decoded_filename.ends_with(".ogg") {
        "audio/ogg"
    } else if decoded_filename.ends_with(".wma") {
        "audio/x-ms-wma"
    } else if decoded_filename.ends_with(".epub") {
        "application/epub+zip"
    } else if decoded_filename.ends_with(".txt") {
        "text/plain; charset=utf-8"
    } else if decoded_filename.ends_with(".pdf") {
        "application/pdf"
    } else {
        "application/octet-stream"
    };

    // 判断是否为流媒体类型（视频/音频）
    let is_streaming_media =
        content_type.starts_with("video/") || content_type.starts_with("audio/");

    if file_size == 0 {
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", content_type)
            .header("Content-Length", "0")
            .header("Accept-Ranges", "bytes")
            .body(Body::empty())
            .unwrap());
    }

    // 解析 Range 请求，确定读取范围
    // 规则：
    // 1) 如果是视频/音频，始终返回分段响应 (206) 以兼容 WebKit 的拖动
    // 2) 对于 EPUB/PDF/TXT 等非流式文件，始终返回完整文件，避免被截断导致解压失败
    let (start, end, status_code) = if is_streaming_media {
        let (s, e) = match parse_range_header(range_header.as_deref(), file_size) {
            Some((s, e)) => (s, e),
            // 无 Range 时也返回 206，表明支持随机访问
            None => (0, file_size - 1),
        };
        (s, e, StatusCode::PARTIAL_CONTENT)
    } else {
        // 始终返回完整文件 (200) 以避免 EPUB/PDF 被截断
        (0, file_size - 1, StatusCode::OK)
    };

    if start > end || start >= file_size {
        return Ok(Response::builder()
            .status(StatusCode::RANGE_NOT_SATISFIABLE)
            .header("Content-Range", format!("bytes */{}", file_size))
            .body(Body::empty())
            .unwrap());
    }

    let chunk_size = end - start + 1;

    // Seek 到起始位置
    if let Err(_) = file.seek(SeekFrom::Start(start)).await {
        return Ok(Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap());
    }

    // 使用流式读取，避免一次性将大文件读入内存
    let stream = ReaderStream::new(file.take(chunk_size));

    let mut builder = Response::builder()
        .status(status_code)
        .header("Content-Type", content_type)
        .header("Content-Length", chunk_size.to_string())
        .header("Accept-Ranges", "bytes");

    // 仅在分段传输时附带 Content-Range
    if status_code == StatusCode::PARTIAL_CONTENT {
        builder = builder.header(
            "Content-Range",
            format!("bytes {}-{}/{}", start, end, file_size),
        );
    }

    Ok(builder.body(Body::wrap_stream(stream)).unwrap())
}

/// 解析 Range 头，返回 (start, end)
fn parse_range_header(range: Option<&str>, file_size: u64) -> Option<(u64, u64)> {
    let range = range?.trim().trim_start_matches("bytes=");
    let parts: Vec<&str> = range.split('-').collect();

    match (parts.get(0), parts.get(1)) {
        // bytes=START-END
        (Some(start), Some(end)) if !start.is_empty() && !end.is_empty() => {
            let s = start.parse().unwrap_or(0);
            let e = end.parse().unwrap_or(file_size.saturating_sub(1));
            if s <= e && s < file_size {
                Some((s, e.min(file_size - 1)))
            } else {
                None
            }
        }
        // bytes=START-
        (Some(start), Some(end)) if !start.is_empty() && end.is_empty() => {
            let s = start.parse().unwrap_or(0);
            if s < file_size {
                Some((s, file_size - 1))
            } else {
                None
            }
        }
        // bytes=-SUFFIX (最后 N 字节)
        (Some(start), Some(end)) if start.is_empty() && !end.is_empty() => {
            let len: u64 = end.parse().unwrap_or(0);
            if len == 0 {
                return None;
            }
            let s = file_size.saturating_sub(len);
            Some((s, file_size - 1))
        }
        _ => None,
    }
}

pub(crate) fn resolve_resource_file_path(
    base_dir: &std::path::Path,
    filename: &str,
) -> Result<PathBuf, SafePathError> {
    resolve_existing_child_path(base_dir, filename)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "openkoto-resource-server-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn resource_path_rejects_encoded_traversal() {
        let root = temp_dir("traversal");
        let videos = root.join("videos");
        fs::create_dir_all(&videos).unwrap();
        fs::write(root.join("secret.txt"), b"secret").unwrap();

        let error = resolve_resource_file_path(&videos, "../secret.txt").unwrap_err();

        assert_eq!(error, SafePathError::PathTraversal);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resource_path_resolves_existing_file_inside_base() {
        let root = temp_dir("inside");
        let books = root.join("books");
        fs::create_dir_all(&books).unwrap();
        fs::write(books.join("book.pdf"), b"pdf").unwrap();

        let resolved = resolve_resource_file_path(&books, "book.pdf").unwrap();

        assert_eq!(resolved, books.join("book.pdf").canonicalize().unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resource_token_validation_requires_exact_session_token() {
        assert!(validate_resource_token("abc123", "abc123"));
        assert!(!validate_resource_token("", "abc123"));
        assert!(!validate_resource_token("abc123", "ABC123"));
        assert!(!validate_resource_token("abc123/", "abc123"));
    }

    #[test]
    fn resource_server_info_exposes_token_and_base_url() {
        let info = resource_server_info();

        assert_eq!(
            info.base_url,
            format!("http://127.0.0.1:{VIDEO_SERVER_PORT}")
        );
        assert_eq!(info.token, resource_session_token());
        assert!(info.token.len() >= 32);
    }
}
