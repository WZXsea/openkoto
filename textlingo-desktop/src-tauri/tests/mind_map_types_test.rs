use openkoto_desktop_lib::types::{
    AgentTask, AgentTaskInput, AgentTaskStatus, AgentTaskType, Artifact, ArtifactType,
    DiagnosticsContentType, DiagnosticsCoverage, MindMap, MindMapDiagnostics, MindMapNode,
    MindMapNodeType, MindMapResult, MindMapStatus, SourceOffset, TimeRange,
};

#[test]
fn agent_task_round_trips_through_json() {
    let task = AgentTask {
        id: "task-1".to_string(),
        task_type: AgentTaskType::MindMapGenerate,
        status: AgentTaskStatus::Queued,
        article_id: "article-1".to_string(),
        input: AgentTaskInput {
            article_id: "article-1".to_string(),
            display_language: "zh-CN".to_string(),
            max_depth: 3,
            evidence_mode: "strict".to_string(),
            prefer_structure: "topic_tree".to_string(),
        },
        progress: 0.0,
        stage: Some("queued".to_string()),
        message: None,
        error: None,
        worker_session_id: None,
        artifact_ids: Vec::new(),
        created_at: "2026-03-07T00:00:00Z".to_string(),
        updated_at: "2026-03-07T00:00:00Z".to_string(),
        started_at: None,
        finished_at: None,
    };

    let json = serde_json::to_string(&task).unwrap();
    let round_trip: AgentTask = serde_json::from_str(&json).unwrap();

    assert_eq!(round_trip.id, task.id);
    assert!(matches!(
        round_trip.task_type,
        AgentTaskType::MindMapGenerate
    ));
    assert!(matches!(round_trip.status, AgentTaskStatus::Queued));
    assert_eq!(round_trip.input.max_depth, 3);
}

#[test]
fn artifact_round_trips_through_json() {
    let artifact = Artifact {
        id: "artifact-1".to_string(),
        task_id: "task-1".to_string(),
        article_id: "article-1".to_string(),
        artifact_type: ArtifactType::MindMap,
        version: "1".to_string(),
        content: serde_json::json!({
            "status": "applicable"
        }),
        metadata: Some(serde_json::json!({
            "source_hash": "sha256:abc"
        })),
        created_at: "2026-03-07T00:00:00Z".to_string(),
        updated_at: "2026-03-07T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string(&artifact).unwrap();
    let round_trip: Artifact = serde_json::from_str(&json).unwrap();

    assert_eq!(round_trip.id, artifact.id);
    assert!(matches!(round_trip.artifact_type, ArtifactType::MindMap));
    assert_eq!(
        round_trip.metadata.unwrap()["source_hash"],
        serde_json::json!("sha256:abc")
    );
}

#[test]
fn mind_map_result_supports_applicable_partial_and_not_applicable() {
    let node = MindMapNode {
        id: "root".to_string(),
        title: "Main Theme".to_string(),
        node_type: MindMapNodeType::Root,
        summary: "A concise summary".to_string(),
        confidence: 0.91,
        source_segment_ids: vec!["seg-1".to_string(), "seg-2".to_string()],
        source_offsets: vec![SourceOffset { start: 0, end: 24 }],
        time_range: Some(TimeRange {
            start: 0.0,
            end: 18.5,
        }),
        children: Vec::new(),
    };

    let applicable = MindMapResult {
        status: MindMapStatus::Applicable,
        reason: None,
        map: Some(MindMap {
            version: "1".to_string(),
            article_id: "article-1".to_string(),
            title: "Sample".to_string(),
            display_language: "zh-CN".to_string(),
            generation_mode: "evidence_first".to_string(),
            source_hash: "sha256:abc".to_string(),
            summary: "Overall summary".to_string(),
            root: node.clone(),
        }),
        diagnostics: MindMapDiagnostics {
            content_type: DiagnosticsContentType::Narrative,
            coverage: DiagnosticsCoverage::Full,
            notes: Vec::new(),
            window_count: 4,
            evidence_density: 1.0,
            low_confidence_node_ids: Vec::new(),
        },
    };

    let partial = MindMapResult {
        status: MindMapStatus::Partial,
        reason: Some("too_long_partial_only".to_string()),
        map: applicable.map.clone(),
        diagnostics: MindMapDiagnostics {
            content_type: DiagnosticsContentType::Dialogue,
            coverage: DiagnosticsCoverage::Partial,
            notes: vec!["Only major themes were included".to_string()],
            window_count: 12,
            evidence_density: 0.72,
            low_confidence_node_ids: vec!["node-7".to_string()],
        },
    };

    let not_applicable = MindMapResult {
        status: MindMapStatus::NotApplicable,
        reason: Some("music_only".to_string()),
        map: None,
        diagnostics: MindMapDiagnostics {
            content_type: DiagnosticsContentType::MusicOnly,
            coverage: DiagnosticsCoverage::None,
            notes: vec!["No stable semantic content detected".to_string()],
            window_count: 1,
            evidence_density: 0.0,
            low_confidence_node_ids: Vec::new(),
        },
    };

    let applicable_json = serde_json::to_string(&applicable).unwrap();
    let partial_json = serde_json::to_string(&partial).unwrap();
    let not_applicable_json = serde_json::to_string(&not_applicable).unwrap();

    let applicable_round_trip: MindMapResult = serde_json::from_str(&applicable_json).unwrap();
    let partial_round_trip: MindMapResult = serde_json::from_str(&partial_json).unwrap();
    let not_applicable_round_trip: MindMapResult =
        serde_json::from_str(&not_applicable_json).unwrap();

    assert!(matches!(
        applicable_round_trip.status,
        MindMapStatus::Applicable
    ));
    assert!(matches!(partial_round_trip.status, MindMapStatus::Partial));
    assert!(matches!(
        not_applicable_round_trip.status,
        MindMapStatus::NotApplicable
    ));
    assert!(not_applicable_round_trip.map.is_none());
    assert!(matches!(
        applicable_round_trip.map.as_ref().unwrap().root.node_type,
        MindMapNodeType::Root
    ));
}
