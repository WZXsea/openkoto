import type { Article } from "../../types";

export const MATERIAL_TYPES = [
  "article",
  "web",
  "text",
  "book",
  "video",
  "audio",
] as const;

export type MaterialType = (typeof MATERIAL_TYPES)[number];
export type ReadingStatus = "unread" | "in_progress" | "completed" | "archived";
export type ImportStatus = "ready" | "importing" | "failed";
export type MaterialSort = "recent" | "title" | "progress" | "created";

export interface MaterialTag {
  id?: string;
  name?: string;
  label?: string;
  value?: string;
}

export interface ReadingProgress {
  reader_kind?: string;
  locator?: Record<string, unknown>;
  progress_ratio?: number;
  status?: ReadingStatus | "reading";
  last_opened_at?: string;
  completed_at?: string | null;
}

/**
 * Optional fields are intentionally local to this feature. Existing Article
 * records remain valid while later backend revisions can populate them.
 */
export interface MaterialMetadata {
  tags?: Array<string | MaterialTag>;
  source_name?: string;
  /** Backend form is an object; number remains for transitional local records. */
  reading_progress?: ReadingProgress | number | null;
  progress_ratio?: number;
  reading_status?: ReadingStatus;
  status?: ReadingStatus;
  last_opened_at?: string;
  completed_at?: string | null;
  import_status?: ImportStatus;
  import_error?: string;
  archived_at?: string | null;
}

export type MaterialArticle = Article & MaterialMetadata;

export interface MaterialFilters {
  query: string;
  type: MaterialType | "all";
  readingStatus: ReadingStatus | "all";
  tag: string | "all";
  createdFrom: string;
  createdTo: string;
  sort: MaterialSort;
}

export const DEFAULT_MATERIAL_FILTERS: MaterialFilters = {
  query: "",
  type: "all",
  readingStatus: "all",
  tag: "all",
  createdFrom: "",
  createdTo: "",
  sort: "recent",
};

export interface BulkMaterialActionHandlers {
  archiveMaterials?: (ids: string[]) => Promise<void>;
  deleteMaterials?: (ids: string[]) => Promise<void>;
}
