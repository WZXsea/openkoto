export const IMPORT_JOB_STATUSES = [
  "queued",
  "validating",
  "parsing",
  "preview_ready",
  "committing",
  "succeeded",
  "failed_retryable",
  "failed_terminal",
  "cancelled",
] as const;

export type ImportJobStatus = (typeof IMPORT_JOB_STATUSES)[number];
export type MaterialTagBulkMode = "add" | "remove" | "replace";
export type DuplicateResolutionAction = "cancel" | "open_existing" | "replace" | "keep_copy";

export interface ManagedMaterialTag {
  id: string;
  name: string;
  color: string;
  materialCount?: number;
}

export interface DuplicateMatch {
  materialId: string;
  matchedBy: string[];
  title?: string;
  sourceType?: string | null;
  sourceUrl?: string | null;
}

export interface ImportPreview {
  title?: string | null;
  summary?: string | null;
  duplicateMatches?: DuplicateMatch[];
}

export interface MaterialImportJob {
  id: string;
  sourceKind: string;
  status: ImportJobStatus;
  progress?: number | null;
  createdAt: string;
  updatedAt?: string | null;
  preview?: ImportPreview | null;
  errorCode?: string | null;
  errorMessage?: string | null;
}

export interface MaterialTagsApi {
  createTag(input: { name: string; color: string }): Promise<void>;
  renameTag(input: { tagId: string; name: string; color: string }): Promise<void>;
  deleteTag(tagId: string): Promise<void>;
  mergeTags(input: { sourceTagId: string; targetTagId: string }): Promise<void>;
  applyTags(input: { materialIds: string[]; tagIds: string[]; mode: MaterialTagBulkMode }): Promise<void>;
}

export interface MaterialImportJobsApi {
  retryJob(jobId: string): Promise<void>;
  cancelJob(jobId: string): Promise<void>;
  resolveJob?(jobId: string, action: DuplicateResolutionAction): Promise<{ id: string }>;
  openPreview?(job: MaterialImportJob): void;
}

const jobStatusLabels: Record<ImportJobStatus, string> = {
  queued: "等待中",
  validating: "校验中",
  parsing: "解析中",
  preview_ready: "待确认",
  committing: "正在写入",
  succeeded: "已完成",
  failed_retryable: "可重试失败",
  failed_terminal: "导入失败",
  cancelled: "已取消",
};

export function getImportJobStatusLabel(status: ImportJobStatus): string {
  return jobStatusLabels[status];
}

export function getImportJobProgress(progress: number | null | undefined): number | null {
  if (typeof progress !== "number" || !Number.isFinite(progress)) return null;
  return Math.round(Math.max(0, Math.min(1, progress > 1 ? progress / 100 : progress)) * 100);
}

export function formatImportJobTime(value: string | null | undefined): string {
  if (!value || Number.isNaN(Date.parse(value))) return "时间未知";
  return new Intl.DateTimeFormat("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
