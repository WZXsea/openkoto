use serde::{Deserialize, Deserializer, Serialize};

/// A single model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub id: String,
    pub name: String,
    pub api_key: String,
    pub api_provider: String, // "openai", "openrouter", "deepseek", "google", "openai-compatible", "ollama", "lmstudio"
    pub model: String,
    pub is_default: bool,
    #[serde(default)]
    pub created_at: Option<String>,
    /// Custom base URL for OpenAI-compatible services, Ollama, LM Studio, etc.
    #[serde(default)]
    pub base_url: Option<String>,
}

impl ModelConfig {
    pub fn new(name: String, api_key: String, api_provider: String, model: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            api_key,
            api_provider,
            model,
            is_default: false,
            created_at: Some(chrono::Utc::now().to_rfc3339()),
            base_url: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptFeature {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub description: String,
    pub prompt_template: String,
    #[serde(default)]
    pub requires_selection: bool,
    #[serde(default)]
    pub show_in_quick_actions: bool,
    pub icon: String,
    #[serde(default)]
    pub sort_order: i32,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub is_builtin: bool,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

impl PromptFeature {
    fn builtin(
        id: &str,
        kind: &str,
        name: &str,
        description: &str,
        prompt_template: &str,
        requires_selection: bool,
        show_in_quick_actions: bool,
        icon: &str,
        sort_order: i32,
    ) -> Self {
        Self {
            id: id.to_string(),
            kind: kind.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            prompt_template: prompt_template.to_string(),
            requires_selection,
            show_in_quick_actions,
            icon: icon.to_string(),
            sort_order,
            enabled: true,
            is_builtin: true,
            created_at: None,
            updated_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Whether onboarding has been completed at least once
    #[serde(default)]
    pub onboarding_completed: bool,
    /// Active model config ID (defaults to first config if not set)
    #[serde(default)]
    pub active_model_id: Option<String>,
    /// List of saved model configurations
    #[serde(default)]
    pub model_configs: Vec<ModelConfig>,
    /// Default target language for translations
    pub target_language: String,
    /// Interface language
    #[serde(default = "default_interface_language")]
    pub interface_language: String,
    /// Concurrent workers used by article batch explanation
    #[serde(default = "default_batch_translation_concurrency")]
    pub batch_translation_concurrency: i32,
    /// Optional global UI font-family override.
    #[serde(default)]
    pub ui_font_family: Option<String>,
    /// Optional reader content font-family override.
    #[serde(default)]
    pub reader_font_family: Option<String>,
    /// Backend API URL for enhanced features
    #[serde(default)]
    pub backend_url: Option<String>,
    /// Auth token for backend API
    #[serde(default)]
    pub auth_token: Option<String>,
    /// Daily limit for introducing new cards in SRS
    #[serde(default = "default_srs_daily_new_limit")]
    pub srs_daily_new_limit: i32,
    /// Daily limit for review cards in SRS
    #[serde(default = "default_srs_daily_review_limit")]
    pub srs_daily_review_limit: i32,
    /// User-editable prompt features for chat and quick actions
    #[serde(
        default = "default_prompt_features",
        deserialize_with = "deserialize_prompt_features"
    )]
    pub prompt_features: Vec<PromptFeature>,
    /// Saved subtitle transcription (ASR) provider configs. Separate from
    /// `model_configs` because ASR (speech-to-text) is a different capability
    /// than chat/translation models.
    #[serde(default)]
    pub asr_configs: Vec<ModelConfig>,
    /// Active ASR config ID (defaults to first asr_config if not set).
    #[serde(default)]
    pub active_asr_model_id: Option<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            onboarding_completed: false,
            active_model_id: None,
            model_configs: Vec::new(),
            target_language: "zh-CN".to_string(),
            interface_language: default_interface_language(),
            batch_translation_concurrency: default_batch_translation_concurrency(),
            ui_font_family: None,
            reader_font_family: None,
            backend_url: None,
            auth_token: None,
            srs_daily_new_limit: default_srs_daily_new_limit(),
            srs_daily_review_limit: default_srs_daily_review_limit(),
            prompt_features: default_prompt_features(),
            asr_configs: Vec::new(),
            active_asr_model_id: None,
        }
    }
}

impl AppConfig {
    /// Get the active model config, or the first one, or None
    pub fn get_active_config(&self) -> Option<&ModelConfig> {
        if let Some(id) = &self.active_model_id {
            self.model_configs.iter().find(|c| &c.id == id)
        } else {
            self.model_configs.first()
        }
    }

    /// Get a model config by ID
    pub fn get_config(&self, id: &str) -> Option<&ModelConfig> {
        self.model_configs.iter().find(|c| c.id == id)
    }

    /// Get the active ASR (subtitle transcription) config, or the first one, or None.
    pub fn get_active_asr_config(&self) -> Option<&ModelConfig> {
        if let Some(id) = &self.active_asr_model_id {
            self.asr_configs.iter().find(|c| &c.id == id)
        } else {
            self.asr_configs.first()
        }
    }
}

fn default_interface_language() -> String {
    "en".to_string()
}

fn default_batch_translation_concurrency() -> i32 {
    3
}

fn default_true() -> bool {
    true
}

pub fn default_prompt_features() -> Vec<PromptFeature> {
    vec![
        PromptFeature::builtin(
            "chat.default",
            "chat_default",
            "Default Chat",
            "Default reading assistant behavior",
            "You are a helpful reading assistant. Help the user understand the material, answer clearly, and prefer the target language when appropriate.",
            false,
            false,
            "sparkles",
            0,
        ),
        PromptFeature::builtin(
            "selection.translate",
            "quick_action",
            "Translate",
            "Translate the selected text",
            "Translate the following text to {target_language}:\n\n{text}",
            true,
            true,
            "translate",
            10,
        ),
        PromptFeature::builtin(
            "selection.explain",
            "quick_action",
            "Explain",
            "Explain the selected text",
            "Explain the following text in {target_language}:\n\n{text}",
            true,
            true,
            "explain",
            20,
        ),
        PromptFeature::builtin(
            "selection.grammar",
            "quick_action",
            "Grammar",
            "Analyze the grammar of the selected text",
            "Analyze the grammar of the following text in {target_language}:\n\n{text}",
            true,
            true,
            "grammar",
            30,
        ),
    ]
}

pub fn merge_missing_builtin_prompt_features(features: Vec<PromptFeature>) -> Vec<PromptFeature> {
    let mut merged = features;

    for builtin in default_prompt_features() {
        if !merged.iter().any(|item| item.id == builtin.id) {
            merged.push(builtin);
        }
    }

    merged.sort_by(|left, right| {
        left.sort_order
            .cmp(&right.sort_order)
            .then_with(|| left.id.cmp(&right.id))
    });

    merged
}

fn deserialize_prompt_features<'de, D>(deserializer: D) -> Result<Vec<PromptFeature>, D::Error>
where
    D: Deserializer<'de>,
{
    let features = Vec::<PromptFeature>::deserialize(deserializer)?;
    Ok(merge_missing_builtin_prompt_features(features))
}

fn default_srs_daily_new_limit() -> i32 {
    20
}

fn default_srs_daily_review_limit() -> i32 {
    100
}

fn default_srs_state() -> String {
    "new".to_string()
}

fn default_srs_ease_factor() -> f64 {
    2.5
}

fn default_srs_due_date() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

fn default_zero() -> i32 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub id: String,
    pub title: String,
    pub content: String,
    /// 素材来源类型: "web" | "article" | "youtube" | "local_video" | "audio" | "book"
    #[serde(default)]
    pub source_type: Option<String>,
    pub source_url: Option<String>,
    pub media_path: Option<String>,
    /// 书籍文件路径 (EPUB/TXT/PDF)
    #[serde(default)]
    pub book_path: Option<String>,
    /// 书籍类型: "epub" | "txt" | "pdf"
    #[serde(default)]
    pub book_type: Option<String>,
    pub created_at: String,
    pub translated: bool,
    #[serde(default)]
    pub active_mind_map_artifact_id: Option<String>,
    #[serde(default)]
    pub segments: Vec<ArticleSegment>,
    #[serde(default = "empty_json_object")]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<MaterialTag>,
    #[serde(default)]
    pub reading_progress: Option<ReadingProgress>,
    #[serde(default)]
    pub archived_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialSummary {
    pub id: String,
    pub title: String,
    pub material_type: String,
    pub created_at: String,
    pub translated: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListMaterialsQuery {
    pub query: Option<String>,
    pub source_type: Option<String>,
    pub tag: Option<String>,
    pub reading_status: Option<String>,
    pub sort: Option<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
    pub include_archived: Option<String>,
    pub created_from: Option<String>,
    pub created_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MaterialTag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMaterialTagRequest {
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatchMaterialTagRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub color: Option<Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeMaterialTagRequest {
    pub target_tag_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SetMaterialTagsRequest {
    #[serde(default)]
    pub tag_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkMaterialTagsRequest {
    pub ids: Vec<String>,
    #[serde(default)]
    pub tag_ids: Vec<String>,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadingProgress {
    pub material_id: String,
    pub reader_kind: String,
    pub locator: serde_json::Value,
    pub progress_ratio: f64,
    pub status: String,
    pub last_opened_at: String,
    pub completed_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertReadingProgressRequest {
    pub reader_kind: String,
    #[serde(default = "empty_json_object")]
    pub locator: serde_json::Value,
    pub progress_ratio: f64,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialImportJob {
    pub id: String,
    pub source_kind: String,
    pub source_uri: Option<String>,
    pub normalized_source_url: Option<String>,
    pub file_id: Option<String>,
    pub input_hash: Option<String>,
    pub file_sha256: Option<String>,
    pub content_sha256: Option<String>,
    pub status: String,
    pub progress: f64,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub result_material_id: Option<String>,
    pub preview: serde_json::Value,
    pub metadata: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMaterialImportJobRequest {
    #[serde(default)]
    pub id: Option<String>,
    pub source_kind: String,
    #[serde(default)]
    pub source_uri: Option<String>,
    #[serde(default)]
    pub file_id: Option<String>,
    #[serde(default)]
    pub input_hash: Option<String>,
    #[serde(default)]
    pub file_sha256: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
    #[serde(default = "empty_json_object")]
    pub preview: serde_json::Value,
    #[serde(default = "empty_json_object")]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PatchMaterialImportJobRequest {
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub progress: Option<f64>,
    #[serde(default)]
    pub error_code: Option<Option<String>>,
    #[serde(default)]
    pub error_message: Option<Option<String>>,
    #[serde(default)]
    pub result_material_id: Option<Option<String>>,
    #[serde(default)]
    pub preview: Option<serde_json::Value>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListMaterialImportJobsQuery {
    pub status: Option<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DuplicateCheckRequest {
    pub source_url: Option<String>,
    pub content: Option<String>,
    pub content_sha256: Option<String>,
    pub file_sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DuplicateMatch {
    pub material_id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub source_type: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    pub matched_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateCheckResponse {
    pub duplicate: bool,
    pub normalized_source_url: Option<String>,
    pub content_sha256: Option<String>,
    pub file_sha256: Option<String>,
    pub matches: Vec<DuplicateMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkMaterialIdsRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkOperationResponse {
    pub affected: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResponse {
    pub deleted: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportJobOptions {
    pub import_job_id: Option<String>,
    pub duplicate_policy: Option<String>,
}

pub const MATERIAL_IMPORT_SOURCE_KINDS: &[&str] = &[
    "article",
    "url",
    "text_file",
    "book",
    "audio",
    "video",
    "subtitle",
    "youtube",
];

pub const MATERIAL_IMPORT_COMMIT_METADATA_KEY: &str = "import_commit";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialImportCommitState {
    #[serde(default)]
    pub duplicate_policy: Option<String>,
    #[serde(default)]
    pub commit_kind: Option<String>,
    #[serde(default)]
    pub target_material_id: Option<String>,
    #[serde(default)]
    pub result_material_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaterialImportRecoveryStrategy {
    NoRecordedSideEffect,
    SettleRecordedMaterial(String),
    SettleOpenExisting(String),
    VerifyTargetMaterialImportMarker(String),
}

pub fn is_supported_material_import_source_kind(source_kind: &str) -> bool {
    MATERIAL_IMPORT_SOURCE_KINDS.contains(&source_kind)
}

pub fn material_import_commit_state_from_metadata(
    metadata: &serde_json::Value,
) -> MaterialImportCommitState {
    metadata
        .get(MATERIAL_IMPORT_COMMIT_METADATA_KEY)
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default()
}

pub fn material_import_metadata_with_commit_state(
    metadata: serde_json::Value,
    state: &MaterialImportCommitState,
) -> serde_json::Value {
    let mut metadata = metadata
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);
    metadata.insert(
        MATERIAL_IMPORT_COMMIT_METADATA_KEY.to_string(),
        serde_json::to_value(state).expect("MaterialImportCommitState must serialize"),
    );
    serde_json::Value::Object(metadata)
}

pub fn material_import_effective_duplicate_policy(
    requested: Option<&str>,
    recorded: Option<&str>,
) -> Result<Option<String>, String> {
    if let (Some(requested), Some(recorded)) = (requested, recorded) {
        if requested != recorded {
            return Err(
                "duplicate_policy does not match the policy recorded for this import job"
                    .to_string(),
            );
        }
    }
    Ok(requested.or(recorded).map(str::to_string))
}

pub fn material_import_recorded_duplicate_policy(metadata: &serde_json::Value) -> Option<String> {
    material_import_commit_state_from_metadata(metadata).duplicate_policy
}

pub fn material_import_commit_recovery_strategy(
    state: &MaterialImportCommitState,
) -> MaterialImportRecoveryStrategy {
    if let Some(material_id) = state.result_material_id.clone() {
        return MaterialImportRecoveryStrategy::SettleRecordedMaterial(material_id);
    }
    if state.commit_kind.as_deref() == Some("open_existing") {
        if let Some(material_id) = state.target_material_id.clone() {
            return MaterialImportRecoveryStrategy::SettleOpenExisting(material_id);
        }
    }
    if let Some(material_id) = state.target_material_id.clone() {
        return MaterialImportRecoveryStrategy::VerifyTargetMaterialImportMarker(material_id);
    }
    MaterialImportRecoveryStrategy::NoRecordedSideEffect
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMaterialImportRequest {
    pub source_kind: String,
    #[serde(default)]
    pub source_uri: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub file_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub input_hash: Option<String>,
    #[serde(default)]
    pub file_sha256: Option<String>,
    #[serde(default)]
    pub content_sha256: Option<String>,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMaterialFileInfo {
    pub file_path: Option<String>,
    pub file_id: Option<String>,
    pub file_name: Option<String>,
    pub byte_size: Option<u64>,
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviewMaterialImportResponse {
    pub title: String,
    pub source_uri: Option<String>,
    pub file: PreviewMaterialFileInfo,
    pub paragraph_count: usize,
    pub content_snippet: Option<String>,
    pub duplicates: DuplicateCheckResponse,
    pub job: MaterialImportJob,
}

fn empty_json_object() -> serde_json::Value {
    serde_json::Value::Object(Default::default())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantConversationMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskType {
    MindMapGenerate,
    AssistantAgentTurn,
    PptOutline,
    PptSlides,
    ArticleAsk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskInput {
    pub article_id: String,
    pub display_language: String,
    pub max_depth: i32,
    pub evidence_mode: String,
    pub prefer_structure: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub task_type: AgentTaskType,
    pub status: AgentTaskStatus,
    pub article_id: String,
    pub input: AgentTaskInput,
    pub progress: f64,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub worker_session_id: Option<String>,
    #[serde(default)]
    pub artifact_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactType {
    MindMap,
    MindMapPreview,
    PptOutline,
    PptSlides,
    ArticleAnswer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: String,
    pub task_id: String,
    pub article_id: String,
    pub artifact_type: ArtifactType,
    pub version: String,
    pub content: serde_json::Value,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MindMapStatus {
    Applicable,
    Partial,
    NotApplicable,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticsContentType {
    Narrative,
    Lecture,
    Dialogue,
    Article,
    Lyrics,
    MusicOnly,
    Mixed,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticsCoverage {
    Full,
    Partial,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MindMapNodeType {
    Root,
    Theme,
    Topic,
    Event,
    Entity,
    Relation,
    Evidence,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceOffset {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapNode {
    pub id: String,
    pub title: String,
    pub node_type: MindMapNodeType,
    pub summary: String,
    pub confidence: f64,
    #[serde(default)]
    pub source_segment_ids: Vec<String>,
    #[serde(default)]
    pub source_offsets: Vec<SourceOffset>,
    #[serde(default)]
    pub time_range: Option<TimeRange>,
    #[serde(default)]
    pub children: Vec<MindMapNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMap {
    pub version: String,
    pub article_id: String,
    pub title: String,
    pub display_language: String,
    pub generation_mode: String,
    pub source_hash: String,
    pub summary: String,
    pub root: MindMapNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapDiagnostics {
    pub content_type: DiagnosticsContentType,
    pub coverage: DiagnosticsCoverage,
    #[serde(default)]
    pub notes: Vec<String>,
    pub window_count: usize,
    pub evidence_density: f64,
    #[serde(default)]
    pub low_confidence_node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MindMapResult {
    pub status: MindMapStatus,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub map: Option<MindMap>,
    pub diagnostics: MindMapDiagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleOverview {
    pub article_id: String,
    pub title: String,
    #[serde(default)]
    pub source_type: Option<String>,
    pub content_length: usize,
    pub segment_count: usize,
    pub has_timestamps: bool,
    pub has_segments: bool,
    #[serde(default)]
    pub language_hint: Option<String>,
    #[serde(default)]
    pub book_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleTextWindow {
    pub cursor: usize,
    pub next_cursor: usize,
    pub has_more: bool,
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
    #[serde(default)]
    pub source_segment_ids: Vec<String>,
    #[serde(default)]
    pub time_range: Option<TimeRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleSearchHit {
    pub segment_id: String,
    pub text: String,
    pub score: f64,
    #[serde(default)]
    pub start_time: Option<f64>,
    #[serde(default)]
    pub end_time: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleSearchResult {
    #[serde(default)]
    pub results: Vec<ArticleSearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleEvidenceItem {
    pub segment_id: String,
    pub text: String,
    #[serde(default)]
    pub start_time: Option<f64>,
    #[serde(default)]
    pub end_time: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleEvidenceResult {
    #[serde(default)]
    pub items: Vec<ArticleEvidenceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArticleSegment {
    pub id: String,
    pub article_id: String,
    pub order: i32,
    pub text: String,
    pub reading_text: Option<String>,
    pub translation: Option<String>,
    pub explanation: Option<SegmentExplanation>,
    /// Start time in seconds (for subtitles)
    #[serde(default)]
    pub start_time: Option<f64>,
    /// End time in seconds (for subtitles)
    #[serde(default)]
    pub end_time: Option<f64>,
    pub created_at: String,
    /// 是否是新段落开始（true则另起一行显示，false则紧跟上一段显示）
    #[serde(default)]
    pub is_new_paragraph: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentExplanation {
    pub translation: String,
    pub explanation: String,
    pub reading_text: Option<String>,
    #[serde(default)]
    pub vocabulary: Vec<VocabularyItem>,
    #[serde(default)]
    pub grammar_points: Vec<GrammarPoint>,
    pub cultural_context: Option<String>,
    pub difficulty_level: Option<String>,
    pub learning_tips: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabularyItem {
    pub word: String,
    pub meaning: String,
    pub usage: String,
    pub example: Option<String>,
    pub reading: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrammarPoint {
    pub point: String,
    pub explanation: String,
    pub example: Option<String>,
}

/// 收藏的单词
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteVocabulary {
    pub id: String,
    pub word: String,
    pub meaning: String,
    pub usage: String,
    #[serde(default)]
    pub explanation: Option<String>,
    pub example: Option<String>,
    pub reading: Option<String>,
    /// 来源文章ID（可选，文章删除后收藏仍保留）
    pub source_article_id: Option<String>,
    /// 来源文章标题（快照，便于显示）
    pub source_article_title: Option<String>,
    #[serde(default)]
    pub pack_ids: Vec<String>,
    #[serde(default = "default_srs_state")]
    pub srs_state: String,
    #[serde(default = "default_srs_ease_factor")]
    pub ease_factor: f64,
    #[serde(default = "default_zero")]
    pub repetitions: i32,
    #[serde(default = "default_zero")]
    pub interval_days: i32,
    #[serde(default = "default_srs_due_date")]
    pub due_date: String,
    #[serde(default)]
    pub last_reviewed_at: Option<String>,
    #[serde(default = "default_zero")]
    pub review_count: i32,
    pub created_at: String,
}

/// 单词包 - 用于组织和分享单词集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordPack {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub language_from: Option<String>,
    #[serde(default)]
    pub language_to: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub is_system: bool,
}

/// 收藏的语法点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteGrammar {
    pub id: String,
    pub point: String,
    pub explanation: String,
    pub example: Option<String>,
    /// 来源文章ID（可选，文章删除后收藏仍保留）
    pub source_article_id: Option<String>,
    /// 来源文章标题（快照，便于显示）
    pub source_article_title: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationRequest {
    pub text: String,
    pub target_language: String,
    pub context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResponse {
    pub translated_text: String,
    pub original_text: String,
    pub model_used: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisRequest {
    pub text: String,
    pub analysis_type: AnalysisType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnalysisType {
    Summary,
    KeyPoints,
    Vocabulary,
    Grammar,
    FullAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResponse {
    pub analysis_type: AnalysisType,
    pub result: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub part_type: String, // "text", "image_url", "file"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<ImageUrl>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_data: Option<FileData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_url: Option<VideoUrl>, // For Kimi model video input
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrl {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileData {
    pub mime_type: String,
    pub data: String, // Base64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" or "assistant"
    pub content: ChatContent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub model: String,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponse {
    pub content: String,
    pub model: String,
    pub tokens_used: Option<u32>,
}

/// 转录片段 (用于字幕提取)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionSegment {
    pub speaker: Option<String>,
    pub content: String,
    /// Start time in seconds
    #[serde(default)]
    pub start_time: Option<f64>,
    /// End time in seconds
    #[serde(default)]
    pub end_time: Option<f64>,
}

/// 转录结果 (用于字幕提取)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptionResult {
    pub segments: Vec<TranscriptionSegment>,
    pub full_text: String,
}

/// 书签 - 用于标记书籍阅读位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: String,
    /// 书籍文件路径
    pub book_path: String,
    /// 书籍类型: "txt" | "pdf" | "epub"
    pub book_type: String,
    /// 书签标题
    pub title: String,
    /// 可选笔记
    #[serde(default)]
    pub note: Option<String>,
    /// 用户选中的文字摘录
    #[serde(default)]
    pub selected_text: Option<String>,
    /// PDF/TXT 页码（从1开始）
    #[serde(default)]
    pub page_number: Option<i32>,
    /// EPUB CFI 位置字符串
    #[serde(default)]
    pub epub_cfi: Option<String>,
    /// 创建时间
    pub created_at: String,
    /// 书签颜色标签（可选）
    #[serde(default)]
    pub color: Option<String>,
}
