use openkoto_desktop_lib::{
    commands::{
        filter_material_summaries, material_summary_from_article,
        require_active_agent_model_config, MaterialSummary,
    },
    ffmpeg::{
        resolve_ffmpeg_invocation_for_path, resolve_ffmpeg_program_for_requirement_path,
        FfmpegInvocation, FfmpegRequirement,
    },
    ktv_export::{
        build_ktv_ffmpeg_args, ensure_ktv_output_created, generate_ktv_ass, prepare_ktv_segments,
        KtvDisplayMode, KtvExportConfig, KtvPositionPreset,
    },
    pdf_sidecar::{build_pdf_sidecar_command_for_dir, resolve_pdf_sidecar_for_dir},
    subtitle_import::{create_article_from_srt, import_subtitles_into_article, parse_srt_content},
    types::{
        is_supported_material_import_source_kind, material_import_commit_recovery_strategy,
        material_import_commit_state_from_metadata, material_import_effective_duplicate_policy,
        material_import_metadata_with_commit_state, AppConfig, Article, ArticleSegment,
        MaterialImportCommitState, MaterialImportRecoveryStrategy, ModelConfig,
    },
};

fn sample_model_config() -> ModelConfig {
    ModelConfig {
        id: "model-1".to_string(),
        name: "Primary".to_string(),
        api_key: "secret".to_string(),
        api_provider: "google".to_string(),
        model: "gemini-2.0-flash".to_string(),
        is_default: true,
        created_at: Some("2026-03-07T00:00:00Z".to_string()),
        base_url: None,
    }
}

fn sample_article_defaults() -> Article {
    Article {
        id: "article-1".to_string(),
        title: "Sample".to_string(),
        content: "body".to_string(),
        source_type: Some("article".to_string()),
        source_url: None,
        media_path: None,
        book_path: None,
        book_type: None,
        created_at: "2026-03-08T00:00:00Z".to_string(),
        translated: false,
        active_mind_map_artifact_id: None,
        segments: Vec::new(),
        metadata: serde_json::json!({}),
        tags: Vec::new(),
        reading_progress: None,
        archived_at: None,
    }
}

fn sample_material_summary(id: &str, title: &str, material_type: &str) -> MaterialSummary {
    MaterialSummary {
        id: id.to_string(),
        title: title.to_string(),
        material_type: material_type.to_string(),
        created_at: "2026-03-08T00:00:00Z".to_string(),
        translated: false,
    }
}

#[test]
fn material_import_commit_state_preserves_policy_target_and_result() {
    let metadata = serde_json::json!({
        "resume_payload": { "source_kind": "url", "source_uri": "https://example.com" },
        "source": "desktop_import",
    });
    let state = MaterialImportCommitState {
        duplicate_policy: Some("replace".to_string()),
        commit_kind: Some("replace".to_string()),
        target_material_id: Some("target-material".to_string()),
        result_material_id: Some("target-material".to_string()),
    };

    let persisted = material_import_metadata_with_commit_state(metadata, &state);

    assert_eq!(persisted["source"], "desktop_import");
    assert_eq!(persisted["resume_payload"]["source_kind"], "url");
    assert_eq!(
        material_import_commit_state_from_metadata(&persisted),
        state
    );
    assert_eq!(
        material_import_commit_recovery_strategy(&state),
        MaterialImportRecoveryStrategy::SettleRecordedMaterial("target-material".to_string())
    );
}

#[test]
fn material_import_response_loss_recovery_is_idempotent_for_replace_and_open_existing() {
    let open_existing = MaterialImportCommitState {
        duplicate_policy: Some("open_existing".to_string()),
        commit_kind: Some("open_existing".to_string()),
        target_material_id: Some("existing-material".to_string()),
        result_material_id: None,
    };
    let replace = MaterialImportCommitState {
        duplicate_policy: Some("replace".to_string()),
        commit_kind: Some("replace".to_string()),
        target_material_id: Some("existing-material".to_string()),
        result_material_id: None,
    };

    assert_eq!(
        material_import_commit_recovery_strategy(&open_existing),
        MaterialImportRecoveryStrategy::SettleOpenExisting("existing-material".to_string())
    );
    assert_eq!(
        material_import_commit_recovery_strategy(&replace),
        MaterialImportRecoveryStrategy::VerifyTargetMaterialImportMarker(
            "existing-material".to_string()
        )
    );
    assert_eq!(
        material_import_commit_recovery_strategy(&replace),
        material_import_commit_recovery_strategy(&replace)
    );
}

#[test]
fn material_import_replay_keeps_recorded_duplicate_policy_and_accepts_all_source_kinds() {
    assert_eq!(
        material_import_effective_duplicate_policy(None, Some("replace")).unwrap(),
        Some("replace".to_string())
    );
    assert!(
        material_import_effective_duplicate_policy(Some("open_existing"), Some("replace")).is_err()
    );

    for source_kind in [
        "article",
        "url",
        "text_file",
        "book",
        "audio",
        "video",
        "subtitle",
        "youtube",
    ] {
        assert!(is_supported_material_import_source_kind(source_kind));
    }
    assert!(!is_supported_material_import_source_kind("unknown"));
}

#[test]
fn resolve_pdf_sidecar_for_dir_reports_missing_sidecar() {
    let dir = std::env::temp_dir().join(format!(
        "openkoto-missing-pdf-sidecar-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let error = resolve_pdf_sidecar_for_dir(&dir, true).unwrap_err();

    assert!(error.contains("PDF sidecar"));

    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn build_pdf_sidecar_command_for_dir_uses_dev_sidecar_entrypoint() {
    let root = std::env::temp_dir().join(format!(
        "openkoto-pdf-sidecar-command-{}",
        uuid::Uuid::new_v4()
    ));
    let base_dir = root.join("textlingo-desktop/src-tauri");
    let sidecar_dir = root.join("textlingo-desktop/pdf-sidecar/openkoto_pdf_translator");

    std::fs::create_dir_all(&base_dir).unwrap();
    std::fs::create_dir_all(&sidecar_dir).unwrap();
    std::fs::write(sidecar_dir.join("pdf2zh.py"), b"print('ok')").unwrap();

    let command = build_pdf_sidecar_command_for_dir(
        &base_dir,
        true,
        "/tmp/sample.pdf",
        "auto",
        "zh",
        "/tmp/output",
    )
    .unwrap();

    assert_eq!(command.program, "python");
    assert_eq!(
        command.args,
        vec![
            "-m",
            "openkoto_pdf_translator.pdf2zh",
            "/tmp/sample.pdf",
            "-li",
            "auto",
            "-lo",
            "zh",
            "-s",
            "openkoto",
            "-o",
            "/tmp/output",
        ]
    );
    assert_eq!(
        command.working_dir,
        root.join("textlingo-desktop/pdf-sidecar")
            .canonicalize()
            .unwrap()
    );

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn resolve_pdf_sidecar_for_dir_uses_bundled_binary_name() {
    let root = std::env::temp_dir().join(format!(
        "openkoto-pdf-sidecar-bundled-{}",
        uuid::Uuid::new_v4()
    ));
    let binaries_dir = root.join("binaries");
    std::fs::create_dir_all(&binaries_dir).unwrap();

    let binary_name = if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "openkoto-pdf-translator-aarch64-apple-darwin"
        } else {
            "openkoto-pdf-translator-x86_64-apple-darwin"
        }
    } else if cfg!(target_os = "windows") {
        "openkoto-pdf-translator-x86_64-pc-windows-msvc.exe"
    } else {
        "openkoto-pdf-translator-x86_64-unknown-linux-gnu"
    };

    let binary_path = binaries_dir.join(binary_name);
    std::fs::write(&binary_path, b"binary").unwrap();

    let command = resolve_pdf_sidecar_for_dir(&root, false).unwrap();

    assert_eq!(command.program, binary_path.to_string_lossy().to_string());
    assert_eq!(command.working_dir, binaries_dir.canonicalize().unwrap());

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn resolve_pdf_sidecar_for_dir_finds_macos_app_bundle_sidecar() {
    if !cfg!(target_os = "macos") {
        return;
    }

    let root = std::env::temp_dir().join(format!(
        "openkoto-pdf-sidecar-app-bundle-{}",
        uuid::Uuid::new_v4()
    ));
    let resources_dir = root.join("OpenKoto Desktop.app/Contents/Resources");
    let macos_dir = root.join("OpenKoto Desktop.app/Contents/MacOS");
    std::fs::create_dir_all(&resources_dir).unwrap();
    std::fs::create_dir_all(&macos_dir).unwrap();

    let binary_path = macos_dir.join("openkoto-pdf-translator");
    std::fs::write(&binary_path, b"binary").unwrap();

    let command = resolve_pdf_sidecar_for_dir(&resources_dir, false).unwrap();

    assert_eq!(command.program, binary_path.to_string_lossy().to_string());
    assert_eq!(command.working_dir, macos_dir.canonicalize().unwrap());

    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn require_active_agent_model_config_requires_any_saved_config() {
    let error = require_active_agent_model_config(None).unwrap_err();

    assert_eq!(error, "未配置 API，请先在设置中配置 AI 模型");
}

#[test]
fn require_active_agent_model_config_requires_active_entry() {
    let error = require_active_agent_model_config(Some(AppConfig::default())).unwrap_err();

    assert_eq!(error, "未设置活动模型配置，请先在设置中配置 AI 模型");
}

#[test]
fn require_active_agent_model_config_returns_selected_model() {
    let expected = sample_model_config();
    let config = AppConfig {
        active_model_id: Some(expected.id.clone()),
        model_configs: vec![expected.clone()],
        ..AppConfig::default()
    };

    let resolved = require_active_agent_model_config(Some(config)).unwrap();

    assert_eq!(resolved.id, expected.id);
    assert_eq!(resolved.api_provider, "google");
}

#[test]
fn material_summary_from_article_maps_book_and_media_types() {
    let article = Article {
        id: "article-1".to_string(),
        title: "N1 Reading".to_string(),
        content: "body".to_string(),
        source_type: Some("web".to_string()),
        book_type: Some("pdf".to_string()),
        translated: true,
        ..sample_article_defaults()
    };

    let summary = material_summary_from_article(&article);

    assert_eq!(summary.material_type, "pdf");
    assert!(summary.translated);
    assert_eq!(summary.title, "N1 Reading");
}

#[test]
fn filter_material_summaries_applies_keyword_and_type() {
    let items = vec![
        sample_material_summary("1", "N1 PDF", "pdf"),
        sample_material_summary("2", "Podcast", "audio"),
        sample_material_summary("3", "N1 Audio", "audio"),
    ];

    let result = filter_material_summaries(&items, Some("N1"), Some("pdf"), 20);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "1");
}

#[test]
fn parse_srt_content_handles_crlf_and_tags() {
    let srt = "1\r\n00:00:00,000 --> 00:00:01,500\r\n<i>Hello</i>\r\n\r\n2\r\n00:00:02,000 --> 00:00:03,250\r\nWorld\r\n";

    let segments = parse_srt_content(srt, "article-1").unwrap();

    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].text, "Hello");
    assert_eq!(segments[0].start_time, Some(0.0));
    assert_eq!(segments[0].end_time, Some(1.5));
    assert_eq!(segments[1].text, "World");
    assert_eq!(segments[1].start_time, Some(2.0));
    assert_eq!(segments[1].end_time, Some(3.25));
}

#[test]
fn import_subtitles_into_article_replaces_segments_and_content() {
    let temp_dir = std::env::temp_dir().join(format!(
        "openkoto-import-subtitles-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let srt_path = temp_dir.join("sample.srt");
    std::fs::write(
        &srt_path,
        "1\n00:00:00,000 --> 00:00:01,000\nFirst line\n\n2\n00:00:01,250 --> 00:00:02,500\nSecond line\n",
    )
    .unwrap();

    let mut article = Article {
        segments: vec![],
        content: "old".to_string(),
        ..sample_article_defaults()
    };

    import_subtitles_into_article(&mut article, &srt_path).unwrap();

    assert_eq!(article.content, "First line Second line");
    assert_eq!(article.segments.len(), 2);
    assert_eq!(article.segments[0].article_id, article.id);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn create_article_from_srt_uses_file_stem_as_default_title() {
    let temp_dir = std::env::temp_dir().join(format!(
        "openkoto-create-srt-article-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let srt_path = temp_dir.join("lesson-01.srt");
    std::fs::write(
        &srt_path,
        "1\n00:00:00,000 --> 00:00:01,000\nStandalone line\n",
    )
    .unwrap();

    let article = create_article_from_srt(&srt_path, None).unwrap();

    assert_eq!(article.title, "lesson-01");
    assert_eq!(article.source_type.as_deref(), Some("article"));
    assert_eq!(article.segments.len(), 1);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn prepare_ktv_segments_fills_missing_reading_text_for_video_segments() {
    let article = Article {
        id: "video-1".to_string(),
        title: "Video".to_string(),
        content: "こんにちは".to_string(),
        source_type: Some("local_video".to_string()),
        source_url: Some("file:///tmp/video.mp4".to_string()),
        media_path: Some("/tmp/video.mp4".to_string()),
        created_at: "2026-03-30T00:00:00Z".to_string(),
        translated: false,
        active_mind_map_artifact_id: None,
        segments: vec![ArticleSegment {
            id: "segment-1".to_string(),
            article_id: "video-1".to_string(),
            order: 0,
            text: "こんにちは".to_string(),
            reading_text: None,
            translation: Some("你好".to_string()),
            explanation: None,
            start_time: Some(0.0),
            end_time: Some(2.0),
            created_at: "2026-03-30T00:00:00Z".to_string(),
            is_new_paragraph: true,
        }],
        ..sample_article_defaults()
    };

    let prepared = prepare_ktv_segments(article, Some("ja")).unwrap();

    assert_eq!(
        prepared.segments[0].reading_text.as_deref(),
        Some("こんにちは")
    );
}

#[test]
fn generate_ktv_ass_contains_reading_original_and_translation_styles() {
    let article = Article {
        id: "video-1".to_string(),
        title: "Video".to_string(),
        content: "こんにちは".to_string(),
        source_type: Some("local_video".to_string()),
        source_url: Some("file:///tmp/video.mp4".to_string()),
        media_path: Some("/tmp/video.mp4".to_string()),
        created_at: "2026-03-30T00:00:00Z".to_string(),
        translated: false,
        active_mind_map_artifact_id: None,
        segments: vec![ArticleSegment {
            id: "segment-1".to_string(),
            article_id: "video-1".to_string(),
            order: 0,
            text: "こんにちは".to_string(),
            reading_text: Some("コンニチハ".to_string()),
            translation: Some("你好".to_string()),
            explanation: None,
            start_time: Some(0.0),
            end_time: Some(2.0),
            created_at: "2026-03-30T00:00:00Z".to_string(),
            is_new_paragraph: true,
        }],
        ..sample_article_defaults()
    };
    let config = sample_ktv_export_config();

    let ass = generate_ktv_ass(&article, &config).unwrap();

    assert!(ass.contains("Style: Original"));
    assert!(ass.contains("Style: Reading"));
    assert!(ass.contains("Style: Translation"));
    assert!(ass.contains("&H64000000"));
    assert!(ass.contains(",2,4,2,32,32,104,1"));
    assert!(ass.contains(",2,4,2,32,32,160,1"));
    assert!(ass.contains(",2,4,2,32,32,48,1"));
    assert!(ass.contains("こんにちは"));
    assert!(ass.contains("（コンニチハ）"));
    assert!(ass.contains("你好"));
    assert!(ass.contains("{\\fnNoto Sans CJK JP\\fs34\\1c&HDBD5D1&"));
    assert!(!ass.contains("Dialogue: 0,0:00:00.00,0:00:02.00,Reading"));
}

#[test]
fn generate_ktv_ass_uses_vocabulary_readings_when_sentence_reading_matches_original() {
    let article = Article {
        id: "video-1".to_string(),
        title: "Video".to_string(),
        content: "氷は全部溶けた".to_string(),
        source_type: Some("local_video".to_string()),
        source_url: Some("file:///tmp/video.mp4".to_string()),
        media_path: Some("/tmp/video.mp4".to_string()),
        created_at: "2026-03-30T00:00:00Z".to_string(),
        translated: false,
        active_mind_map_artifact_id: None,
        segments: vec![ArticleSegment {
            id: "segment-1".to_string(),
            article_id: "video-1".to_string(),
            order: 0,
            text: "氷は全部溶けた".to_string(),
            reading_text: Some("氷は全部溶けた".to_string()),
            translation: Some("冰都融化了".to_string()),
            explanation: Some(openkoto_desktop_lib::types::SegmentExplanation {
                translation: "冰都融化了".to_string(),
                explanation: "Line explanation".to_string(),
                reading_text: None,
                vocabulary: vec![
                    openkoto_desktop_lib::types::VocabularyItem {
                        word: "氷".to_string(),
                        meaning: "ice".to_string(),
                        usage: "".to_string(),
                        example: None,
                        reading: Some("こおり".to_string()),
                    },
                    openkoto_desktop_lib::types::VocabularyItem {
                        word: "溶けた".to_string(),
                        meaning: "melted".to_string(),
                        usage: "".to_string(),
                        example: None,
                        reading: Some("とけた".to_string()),
                    },
                ],
                grammar_points: vec![],
                cultural_context: None,
                difficulty_level: None,
                learning_tips: None,
            }),
            start_time: Some(0.0),
            end_time: Some(2.0),
            created_at: "2026-03-30T00:00:00Z".to_string(),
            is_new_paragraph: true,
        }],
        ..sample_article_defaults()
    };
    let config = sample_ktv_export_config();

    let ass = generate_ktv_ass(&article, &config).unwrap();

    assert!(ass.contains("氷{\\fnNoto Sans CJK JP\\fs34\\1c&HDBD5D1&}（こおり）{\\rOriginal}"));
    assert!(ass.contains("溶けた{\\fnNoto Sans CJK JP\\fs34\\1c&HDBD5D1&}（とけた）{\\rOriginal}"));
}

#[test]
fn generate_ktv_ass_translation_mode_only_contains_translation_dialogue() {
    let article = Article {
        id: "video-1".to_string(),
        title: "Video".to_string(),
        content: "こんにちは".to_string(),
        source_type: Some("local_video".to_string()),
        source_url: Some("file:///tmp/video.mp4".to_string()),
        media_path: Some("/tmp/video.mp4".to_string()),
        created_at: "2026-03-30T00:00:00Z".to_string(),
        translated: false,
        active_mind_map_artifact_id: None,
        segments: vec![ArticleSegment {
            id: "segment-1".to_string(),
            article_id: "video-1".to_string(),
            order: 0,
            text: "こんにちは".to_string(),
            reading_text: Some("コンニチハ".to_string()),
            translation: Some("你好".to_string()),
            explanation: None,
            start_time: Some(0.0),
            end_time: Some(2.0),
            created_at: "2026-03-30T00:00:00Z".to_string(),
            is_new_paragraph: true,
        }],
        ..sample_article_defaults()
    };
    let mut config = sample_ktv_export_config();
    config.display_mode = KtvDisplayMode::Translation;
    config.show_reading = false;

    let ass = generate_ktv_ass(&article, &config).unwrap();

    assert!(ass.contains("Style: Original"));
    assert!(ass.contains("Style: Reading"));
    assert!(ass.contains("Style: Translation"));
    assert!(ass.contains("Dialogue: 0,0:00:00.00,0:00:02.00,Translation"));
    assert!(!ass.contains("Dialogue: 0,0:00:00.00,0:00:02.00,Original"));
    assert!(!ass.contains("Dialogue: 0,0:00:00.00,0:00:02.00,Reading"));
    assert!(ass.contains("你好"));
    assert!(!ass.contains("こんにちは"));
    assert!(!ass.contains("コンニチハ"));
}

#[test]
fn generate_ktv_ass_uses_source_video_resolution_when_provided() {
    let article = Article {
        id: "video-1".to_string(),
        title: "Video".to_string(),
        content: "こんにちは".to_string(),
        source_type: Some("local_video".to_string()),
        source_url: Some("file:///tmp/video.mp4".to_string()),
        media_path: Some("/tmp/video.mp4".to_string()),
        created_at: "2026-03-30T00:00:00Z".to_string(),
        translated: false,
        active_mind_map_artifact_id: None,
        segments: vec![ArticleSegment {
            id: "segment-1".to_string(),
            article_id: "video-1".to_string(),
            order: 0,
            text: "こんにちは".to_string(),
            reading_text: Some("コンニチハ".to_string()),
            translation: Some("你好".to_string()),
            explanation: None,
            start_time: Some(0.0),
            end_time: Some(2.0),
            created_at: "2026-03-30T00:00:00Z".to_string(),
            is_new_paragraph: true,
        }],
        ..sample_article_defaults()
    };
    let mut config = sample_ktv_export_config();
    config.video_width = Some(640);
    config.video_height = Some(360);

    let ass = generate_ktv_ass(&article, &config).unwrap();

    assert!(ass.contains("PlayResX: 640"));
    assert!(ass.contains("PlayResY: 360"));
}

#[test]
fn build_ktv_ffmpeg_args_uses_ass_filter_and_output_path() {
    let args = build_ktv_ffmpeg_args(
        std::path::Path::new("/tmp/input.mp4"),
        std::path::Path::new("/tmp/subtitles.ass"),
        std::path::Path::new("/tmp/output.mp4"),
    );

    assert_eq!(args[0], "-y");
    assert!(args
        .iter()
        .any(|arg| arg.contains("ass=filename='/tmp/subtitles.ass'")));
    assert_eq!(args.last().unwrap(), "/tmp/output.mp4");
}

#[test]
fn ensure_ktv_output_created_rejects_missing_output_file() {
    let output =
        std::env::temp_dir().join(format!("openkoto-ktv-missing-{}.mp4", uuid::Uuid::new_v4()));

    let error = ensure_ktv_output_created(&output).unwrap_err();

    assert!(error.contains("FFmpeg 未生成导出文件"));
}

#[test]
fn resolve_ffmpeg_invocation_prefers_system_binary_in_dev_mode() {
    let temp_dir =
        std::env::temp_dir().join(format!("openkoto-system-ffmpeg-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let binary_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let binary_path = temp_dir.join(binary_name);

    if cfg!(target_os = "windows") {
        std::fs::write(&binary_path, "@echo off\r\nexit /b 0\r\n").unwrap();
    } else {
        std::fs::write(&binary_path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&binary_path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&binary_path, permissions).unwrap();
        }
    }

    let invocation = resolve_ffmpeg_invocation_for_path(true, Some(temp_dir.as_os_str()));

    assert_eq!(invocation, FfmpegInvocation::System);

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

#[test]
fn resolve_ffmpeg_invocation_falls_back_to_sidecar_when_missing() {
    let invocation = resolve_ffmpeg_invocation_for_path(false, None);

    assert_eq!(invocation, FfmpegInvocation::Sidecar);
}

#[test]
fn resolve_ffmpeg_program_for_subtitle_burn_prefers_supported_override() {
    let temp_dir = std::env::temp_dir().join(format!(
        "openkoto-system-ffmpeg-subtitle-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_dir).unwrap();

    let binary_name = if cfg!(target_os = "windows") {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let system_binary_path = temp_dir.join(binary_name);
    let subtitle_binary_path = temp_dir.join(if cfg!(target_os = "windows") {
        "ffmpeg-subtitle.exe"
    } else {
        "ffmpeg-subtitle"
    });

    write_fake_ffmpeg_binary(&system_binary_path, false);
    write_fake_ffmpeg_binary(&subtitle_binary_path, true);

    let previous = std::env::var_os("OPENKOTO_FFMPEG_SUBTITLE");
    std::env::set_var("OPENKOTO_FFMPEG_SUBTITLE", &subtitle_binary_path);

    let resolved = resolve_ffmpeg_program_for_requirement_path(
        true,
        Some(temp_dir.as_os_str()),
        FfmpegRequirement::SubtitleBurn,
    );

    match previous {
        Some(value) => std::env::set_var("OPENKOTO_FFMPEG_SUBTITLE", value),
        None => std::env::remove_var("OPENKOTO_FFMPEG_SUBTITLE"),
    }

    assert_eq!(
        resolved.as_deref(),
        Some(subtitle_binary_path.to_string_lossy().as_ref())
    );

    std::fs::remove_dir_all(&temp_dir).unwrap();
}

fn sample_ktv_export_config() -> KtvExportConfig {
    KtvExportConfig {
        display_mode: KtvDisplayMode::Bilingual,
        show_reading: true,
        original_font_family: "Noto Sans CJK JP".to_string(),
        translation_font_family: "Noto Sans CJK SC".to_string(),
        reading_font_family: "Noto Sans CJK JP".to_string(),
        font_size: 48,
        reading_scale: 0.7,
        line_gap: 8,
        bilingual_gap: 12,
        original_color: "#FFFFFF".to_string(),
        translation_color: "#FACC15".to_string(),
        reading_color: "#D1D5DB".to_string(),
        outline_color: "#000000".to_string(),
        outline_width: 2,
        shadow_enabled: true,
        shadow_color: "#000000".to_string(),
        shadow_offset_x: 0,
        shadow_offset_y: 2,
        shadow_blur: 4,
        position_preset: KtvPositionPreset::Bottom,
        bottom_margin: 48,
        horizontal_margin: 32,
        video_width: None,
        video_height: None,
    }
}

fn write_fake_ffmpeg_binary(path: &std::path::Path, supports_subtitles: bool) {
    if cfg!(target_os = "windows") {
        let filters_output = if supports_subtitles {
            " .. ass               V->V       Render ASS subtitles onto input video using the libass library.\r\n"
        } else {
            ""
        };
        let script = format!(
            "@echo off\r\nif \"%~1\"==\"-version\" exit /b 0\r\nif \"%~1\"==\"-hide_banner\" if \"%~2\"==\"-filters\" (\r\n  echo {filters_output}\r\n  exit /b 0\r\n)\r\nexit /b 1\r\n"
        );
        std::fs::write(path, script).unwrap();
    } else {
        let filters_output = if supports_subtitles {
            " .. ass               V->V       Render ASS subtitles onto input video using the libass library.\n"
        } else {
            ""
        };
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"-version\" ]; then\n  exit 0\nfi\nif [ \"$1\" = \"-hide_banner\" ] && [ \"$2\" = \"-filters\" ]; then\n  printf '%s' \"{filters_output}\"\n  exit 0\nfi\nexit 1\n"
        );
        std::fs::write(path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(path, permissions).unwrap();
        }
    }
}
