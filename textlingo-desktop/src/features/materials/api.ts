import { invoke } from "@tauri-apps/api/core";

import type { Article } from "../../types";
import { parseSourceLocator, type ReadingProgressUpdate } from "../reader";
import type {
  DuplicateMatch,
  MaterialImportJob,
  MaterialImportJobsApi,
  MaterialTagBulkMode,
  MaterialTagsApi,
  ManagedMaterialTag,
} from "./materialManagement";

export interface MaterialsApi {
  archive(ids: string[]): Promise<void>;
  remove(ids: string[]): Promise<void>;
  listTags(): Promise<ManagedMaterialTag[]>;
  tags: MaterialTagsApi;
  listImportJobs(): Promise<MaterialImportJob[]>;
  jobs: MaterialImportJobsApi;
  getReadingProgress(materialId: string): Promise<ReadingProgressUpdate | undefined>;
  upsertReadingProgress(materialId: string, update: ReadingProgressUpdate): Promise<void>;
}

interface TauriReadingProgress {
  reader_kind?: unknown;
  locator?: unknown;
  progress_ratio?: unknown;
  status?: string;
}

interface TauriMaterialTag {
  id: string;
  name: string;
  color?: string | null;
  material_count?: number | null;
}

interface TauriDuplicateMatch {
  material_id: string;
  title?: string | null;
  source_type?: string | null;
  source_url?: string | null;
  matched_by?: string[];
}

interface TauriDuplicateCheck {
  duplicate?: boolean;
  matches?: TauriDuplicateMatch[];
}

interface TauriImportJob {
  id: string;
  source_kind: string;
  status: MaterialImportJob["status"];
  progress?: number | null;
  created_at: string;
  updated_at?: string | null;
  preview?: {
    title?: string | null;
    summary?: string | null;
    duplicates?: TauriDuplicateCheck;
  } | null;
  error_code?: string | null;
  error_message?: string | null;
}

function asManagedTag(tag: TauriMaterialTag): ManagedMaterialTag {
  return {
    id: tag.id,
    name: tag.name,
    color: tag.color || "#2563eb",
    materialCount: tag.material_count ?? undefined,
  };
}

function asDuplicateMatch(match: TauriDuplicateMatch): DuplicateMatch {
  return {
    materialId: match.material_id,
    title: match.title || undefined,
    sourceType: match.source_type,
    sourceUrl: match.source_url,
    matchedBy: Array.isArray(match.matched_by) ? match.matched_by : [],
  };
}

function asReadingProgress(progress: TauriReadingProgress | null): ReadingProgressUpdate | undefined {
  const locator = parseSourceLocator(progress?.locator);
  if (!progress
    || !["article", "pdf", "epub", "txt", "media"].includes(String(progress.reader_kind))
    || !locator
    || typeof progress.progress_ratio !== "number"
    || !Number.isFinite(progress.progress_ratio)) return undefined;
  return {
    reader_kind: progress.reader_kind as ReadingProgressUpdate["reader_kind"],
    locator,
    progress_ratio: Math.min(1, Math.max(0, progress.progress_ratio)),
    status: progress.status === "completed" ? "completed" : "reading",
  };
}

function asImportJob(job: TauriImportJob): MaterialImportJob {
  const duplicates = job.preview?.duplicates?.matches;
  return {
    id: job.id,
    sourceKind: job.source_kind,
    status: job.status,
    progress: job.progress,
    createdAt: job.created_at,
    updatedAt: job.updated_at,
    preview: job.preview ? {
      title: job.preview.title,
      summary: job.preview.summary,
      duplicateMatches: duplicates?.map(asDuplicateMatch),
    } : null,
    errorCode: job.error_code,
    errorMessage: job.error_message,
  };
}

/** Tauri command adapter for the material workbench. */
export function createMaterialsApi(): MaterialsApi {
  return {
    archive: async (ids) => {
      await invoke("material_library_bulk_archive_cmd", { request: { ids } });
    },
    remove: async (ids) => {
      await invoke("material_library_bulk_delete_cmd", { request: { ids } });
    },
    listTags: async () => (await invoke<TauriMaterialTag[]>("material_library_list_tags_cmd")).map(asManagedTag),
    tags: {
      createTag: async ({ name, color }) => {
        await invoke("material_library_create_tag_cmd", { request: { name, color } });
      },
      renameTag: async ({ tagId, name, color }) => {
        await invoke("material_library_patch_tag_cmd", { id: tagId, request: { name, color } });
      },
      deleteTag: async (tagId) => {
        await invoke("material_library_delete_tag_cmd", { id: tagId });
      },
      mergeTags: async ({ sourceTagId, targetTagId }) => {
        await invoke("material_library_merge_tag_cmd", { sourceTagId, request: { target_tag_id: targetTagId } });
      },
      applyTags: async ({ materialIds, tagIds, mode }) => {
        await invoke("material_library_bulk_tags_cmd", {
          request: { ids: materialIds, tag_ids: tagIds, mode: mode satisfies MaterialTagBulkMode },
        });
      },
    },
    listImportJobs: async () => (await invoke<TauriImportJob[]>("material_library_list_import_jobs_cmd", {
      query: { limit: "50" },
    })).map(asImportJob),
    jobs: {
      retryJob: async (jobId) => {
        await invoke("material_library_resume_import_job_cmd", { id: jobId, duplicatePolicy: null });
      },
      resolveJob: async (jobId, action) => {
        return invoke<Article>("material_library_resume_import_job_cmd", { id: jobId, duplicatePolicy: action });
      },
      cancelJob: async (jobId) => {
        await invoke("material_library_cancel_import_job_cmd", { id: jobId });
      },
    },
    getReadingProgress: async (materialId) => {
      const progress = await invoke<TauriReadingProgress | null>("material_library_get_reading_progress_cmd", { materialId });
      return asReadingProgress(progress);
    },
    upsertReadingProgress: async (materialId, update) => {
      await invoke("material_library_upsert_reading_progress_cmd", {
        materialId,
        request: update,
      });
    },
  };
}

export type PreviewMaterialImportRequest = {
  sourceKind: "article" | "url" | "text_file" | "book" | "audio" | "video" | "subtitle" | "youtube";
  sourceUri?: string;
  content?: string;
  filePath?: string;
  title?: string;
  metadata?: Record<string, unknown>;
};

export interface PreviewMaterialImportResult {
  jobId: string;
  title: string;
  sourceUri?: string;
  contentSnippet?: string;
  paragraphCount: number;
  file: PreviewMaterialFileInfo;
  duplicates: DuplicateMatch[];
}

export interface PreviewMaterialFileInfo {
  filePath?: string;
  fileId?: string;
  fileName?: string;
  byteSize?: number;
  sha256?: string;
}

interface TauriPreviewMaterialFileInfo {
  file_path?: string | null;
  file_id?: string | null;
  file_name?: string | null;
  byte_size?: number | null;
  sha256?: string | null;
}

function asPreviewFileInfo(file?: TauriPreviewMaterialFileInfo | null): PreviewMaterialFileInfo {
  return {
    filePath: file?.file_path || undefined,
    fileId: file?.file_id || undefined,
    fileName: file?.file_name || undefined,
    byteSize: typeof file?.byte_size === "number" ? file.byte_size : undefined,
    sha256: file?.sha256 || undefined,
  };
}

function contentSnippet(content?: string): string | undefined {
  const normalized = content?.replace(/\s+/g, " ").trim();
  return normalized ? normalized.slice(0, 500) : undefined;
}

export async function previewMaterialImport(request: PreviewMaterialImportRequest): Promise<PreviewMaterialImportResult> {
  const response = await invoke<{
    title?: string | null;
    source_uri?: string | null;
    content_snippet?: string | null;
    file?: TauriPreviewMaterialFileInfo | null;
    paragraph_count?: number | null;
    job: TauriImportJob;
    duplicates?: TauriDuplicateCheck;
  }>("preview_material_import_cmd", {
    request: {
      source_kind: request.sourceKind,
      source_uri: request.sourceUri,
      content: request.content,
      file_path: request.filePath,
      title: request.title,
      metadata: request.metadata,
    },
  });
  const matches = Array.isArray(response.duplicates?.matches) ? response.duplicates.matches : [];
  const file = asPreviewFileInfo(response.file);
  return {
    jobId: response.job.id,
    title: response.title?.trim() || request.title?.trim() || file.fileName || request.sourceUri || "未命名素材",
    sourceUri: response.source_uri || request.sourceUri,
    contentSnippet: response.content_snippet?.trim() || contentSnippet(request.content),
    paragraphCount: typeof response.paragraph_count === "number" ? response.paragraph_count : 0,
    file,
    duplicates: matches.map(asDuplicateMatch),
  };
}

export async function cancelMaterialImportJob(jobId: string): Promise<void> {
  await invoke("material_library_cancel_import_job_cmd", { id: jobId });
}

export type ImportedArticle = Article;
