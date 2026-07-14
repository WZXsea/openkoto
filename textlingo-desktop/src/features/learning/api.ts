import { invoke } from "@tauri-apps/api/core";

import type {
  BulkOrganizeLearningItemInput,
  BulkOrganizeLearningItemsResponse,
  LearningActivityEvent,
  LearningReview,
  LegacyLearningItemMigrationResult,
  LearningWorkbenchItem,
  ListLearningWorkbenchItemsQuery,
  UpdateLearningWorkbenchItemInput,
} from "./types";

export interface LearningWorkbenchApi {
  list(query?: ListLearningWorkbenchItemsQuery): Promise<LearningWorkbenchItem[]>;
  update(id: string, payload: UpdateLearningWorkbenchItemInput): Promise<LearningWorkbenchItem>;
  bulkOrganize(items: BulkOrganizeLearningItemInput[]): Promise<BulkOrganizeLearningItemsResponse>;
  getDailyReview?(timezoneOffsetMinutes: number): Promise<LearningReview>;
  getMaterialReview?(materialId: string): Promise<LearningReview>;
  recordLocalPreview?(id: string, metadata?: Record<string, unknown>): Promise<LearningActivityEvent>;
  migrateLegacy?(dryRun: boolean): Promise<LegacyLearningItemMigrationResult>;
}

function normalizeItem(item: LearningWorkbenchItem): LearningWorkbenchItem {
  return {
    ...item,
    quality_flags: Array.isArray(item.quality_flags) ? item.quality_flags : [],
  };
}

function normalizeBulkResponse(response: BulkOrganizeLearningItemsResponse): BulkOrganizeLearningItemsResponse {
  const results = Array.isArray(response.results) ? response.results : [];
  return {
    ...response,
    succeeded: Number.isFinite(response.succeeded)
      ? response.succeeded
      : results.filter((result) => result.success).length,
    failed: Number.isFinite(response.failed)
      ? response.failed
      : results.filter((result) => !result.success).length,
    results: results.map((result) => ({
      ...result,
      item: result.item ? normalizeItem(result.item) : undefined,
    })),
  };
}

/** Tauri command adapter for the PR-10 learning workbench. */
export function createLearningWorkbenchApi(): LearningWorkbenchApi {
  return {
    list: async (query) => {
      const items = await invoke<LearningWorkbenchItem[]>("list_learning_items_cmd", {
        query: query ?? null,
      });
      return items.map(normalizeItem);
    },
    update: async (id, payload) => normalizeItem(await invoke<LearningWorkbenchItem>("update_learning_item_cmd", {
      id,
      payload,
    })),
    bulkOrganize: async (items) => normalizeBulkResponse(await invoke<BulkOrganizeLearningItemsResponse>(
      "bulk_organize_learning_items_cmd",
      { payload: { items } },
    )),
    getDailyReview: (timezoneOffsetMinutes) => invoke<LearningReview>("get_daily_learning_review_cmd", {
      query: { timezone_offset_minutes: timezoneOffsetMinutes },
    }),
    getMaterialReview: (materialId) => invoke<LearningReview>("get_material_learning_review_cmd", {
      materialId,
    }),
    recordLocalPreview: (id, metadata = {}) => invoke<LearningActivityEvent>("record_local_preview_cmd", {
      id,
      payload: {
        metadata,
        idempotency_key: `local-preview:${id}:${crypto.randomUUID()}`,
      },
    }),
    migrateLegacy: (dryRun) => invoke<LegacyLearningItemMigrationResult>("migrate_legacy_learning_items_cmd", {
      payload: { dry_run: dryRun },
    }),
  };
}

export const learningWorkbenchApi = createLearningWorkbenchApi();
