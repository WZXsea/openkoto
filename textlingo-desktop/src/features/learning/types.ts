import type { LearningItem, LearningItemStatus, LearningItemType } from "../../types";

export const LEARNING_ITEM_STATUSES = ["candidate", "accepted", "rejected", "archived"] as const;
export const LEARNING_ITEM_TYPES = ["word", "phrase", "sentence", "grammar"] as const;

export type CanonicalLearningItemStatus = (typeof LEARNING_ITEM_STATUSES)[number];
export type CanonicalLearningItemType = (typeof LEARNING_ITEM_TYPES)[number];

export type LearningQualityFlag =
  | "needs_verification"
  | "possible_duplicate"
  | "insufficient_context"
  | (string & {});

export interface LearningWorkbenchItem extends LearningItem {
  quality_flags: LearningQualityFlag[];
  source_type?: string | null;
  merged_into_id?: string | null;
  status_before_archive?: CanonicalLearningItemStatus | null;
}

export interface ListLearningWorkbenchItemsQuery {
  status?: LearningItemStatus;
  item_type?: LearningItemType;
  material_id?: string;
  source_type?: string;
  source?: string;
  tag?: string;
  quality_flag?: LearningQualityFlag;
  limit?: number;
  offset?: number;
}

export interface UpdateLearningWorkbenchItemInput {
  item_type?: LearningItemType;
  text?: string;
  source_sentence?: string | null;
  context_before?: string | null;
  context_after?: string | null;
  meaning_in_context?: string | null;
  definition_en?: string | null;
  definition_zh?: string | null;
  tags?: string[];
  quality_flags?: LearningQualityFlag[];
}

export interface BulkOrganizeLearningItemInput {
  id: string;
  status?: CanonicalLearningItemStatus;
  tags?: string[];
  quality_flags?: LearningQualityFlag[];
  merge_into_id?: string;
  favorite_type?: "vocabulary" | "grammar";
  pack_ids?: string[];
}

export interface BulkLearningItemError {
  code: string;
  message: string;
}

export interface BulkOrganizeLearningItemResult {
  id: string;
  success: boolean;
  item?: LearningWorkbenchItem;
  merged_into_id?: string | null;
  error?: BulkLearningItemError;
}

export interface BulkOrganizeLearningItemsResponse {
  succeeded: number;
  failed: number;
  results: BulkOrganizeLearningItemResult[];
}

export interface LearningMaterialOption {
  id: string;
  title: string;
  sourceType?: string;
}

export interface LearningActivityEvent {
  id: string;
  learning_item_id?: string | null;
  material_id?: string | null;
  event_type: string;
  metadata: Record<string, unknown>;
  occurred_at: string;
}

export interface LearningReview {
  date?: string | null;
  material_id?: string | null;
  total_events: number;
  event_counts: Record<string, number>;
  unique_learning_items: number;
  events: LearningActivityEvent[];
}

export interface LegacyLearningItemMigrationConflict {
  source_type: string;
  source_id: string;
  code: string;
  message: string;
}

export interface LegacyLearningItemMigrationResult {
  dry_run: boolean;
  planned: number;
  migrated: number;
  already_migrated: number;
  conflicts: LegacyLearningItemMigrationConflict[];
}

export interface LearningItemDraft {
  itemType: LearningItemType;
  text: string;
  meaningInContext: string;
  definitionZh: string;
  definitionEn: string;
  tagsInput: string;
  needsVerification: boolean;
}

export const QUALITY_FLAG_LABELS: Record<string, string> = {
  needs_verification: "待人工核验",
  possible_duplicate: "疑似重复",
  insufficient_context: "上下文不足",
};

export const LEARNING_ITEM_STATUS_LABELS: Record<CanonicalLearningItemStatus, string> = {
  candidate: "候选",
  accepted: "已接受",
  rejected: "已拒绝",
  archived: "已归档",
};

export const LEARNING_ITEM_TYPE_LABELS: Record<CanonicalLearningItemType, string> = {
  word: "单词",
  phrase: "短语",
  sentence: "句子",
  grammar: "语法",
};

export function normalizeTags(value: string): string[] {
  return Array.from(new Set(value.split(/[,\n，]/).map((tag) => tag.trim()).filter(Boolean)));
}

export function createLearningItemDraft(item: LearningWorkbenchItem): LearningItemDraft {
  return {
    itemType: item.item_type,
    text: item.text,
    meaningInContext: item.meaning_in_context ?? "",
    definitionZh: item.definition_zh ?? "",
    definitionEn: item.definition_en ?? "",
    tagsInput: item.tags.join(", "),
    needsVerification: item.quality_flags.includes("needs_verification"),
  };
}

export function restoreStatusFor(item: LearningWorkbenchItem): CanonicalLearningItemStatus {
  const previous = item.status_before_archive;
  return previous && previous !== "archived" ? previous : "candidate";
}
