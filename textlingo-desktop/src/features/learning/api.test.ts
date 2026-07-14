import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { createLearningWorkbenchApi } from "./api";
import type { LearningWorkbenchItem } from "./types";

function createItem(overrides: Partial<LearningWorkbenchItem> = {}): LearningWorkbenchItem {
  return {
    id: "item-1",
    material_id: "material-1",
    segment_id: "segment-1",
    item_type: "word",
    text: "mitigate",
    source_sentence: "Early action can mitigate risk.",
    context_before: "Before",
    context_after: "After",
    meaning_in_context: null,
    definition_en: null,
    definition_zh: null,
    collocations: [],
    examples: [],
    tags: ["academic"],
    quality_flags: ["needs_verification"],
    status: "candidate",
    priority: 0,
    difficulty: null,
    ai_explanation: null,
    review_state: {},
    source_material_title: "Clinical paper",
    source_type: "article",
    source_segment_order: 0,
    accepted_at: null,
    rejected_at: null,
    created_at: "2026-07-14T00:00:00Z",
    updated_at: "2026-07-14T00:00:00Z",
    ...overrides,
  };
}

describe("learning workbench Tauri adapter", () => {
  beforeEach(() => invoke.mockReset());

  it("passes every workbench filter to the canonical learning-item list command", async () => {
    invoke.mockResolvedValueOnce([{ ...createItem(), quality_flags: undefined }]);

    const result = await createLearningWorkbenchApi().list({
      status: "candidate",
      item_type: "word",
      material_id: "material-1",
      source_type: "article",
      tag: "academic",
      quality_flag: "needs_verification",
      limit: 300,
      offset: 0,
    });

    expect(invoke).toHaveBeenCalledWith("list_learning_items_cmd", {
      query: {
        status: "candidate",
        item_type: "word",
        material_id: "material-1",
        source_type: "article",
        tag: "academic",
        quality_flag: "needs_verification",
        limit: 300,
        offset: 0,
      },
    });
    expect(result[0].quality_flags).toEqual([]);
  });

  it("uses the PR-10 update and partial-success bulk command contracts", async () => {
    invoke
      .mockResolvedValueOnce(createItem({ text: "mitigation" }))
      .mockResolvedValueOnce({
        succeeded: 1,
        failed: 1,
        results: [
          { id: "item-1", success: true, item: createItem({ status: "accepted" }) },
          { id: "item-2", success: false, error: { code: "invalid_transition", message: "cannot accept" } },
        ],
      });
    const api = createLearningWorkbenchApi();

    await expect(api.update("item-1", { text: "mitigation", quality_flags: [] })).resolves.toEqual(
      expect.objectContaining({ text: "mitigation", quality_flags: ["needs_verification"] }),
    );
    await expect(api.bulkOrganize([
      { id: "item-1", status: "accepted", favorite_type: "vocabulary", pack_ids: [] },
      { id: "item-2", status: "accepted", favorite_type: "grammar", pack_ids: [] },
    ])).resolves.toEqual(expect.objectContaining({ succeeded: 1, failed: 1 }));

    expect(invoke).toHaveBeenNthCalledWith(1, "update_learning_item_cmd", {
      id: "item-1",
      payload: { text: "mitigation", quality_flags: [] },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "bulk_organize_learning_items_cmd", {
      payload: {
        items: [
          { id: "item-1", status: "accepted", favorite_type: "vocabulary", pack_ids: [] },
          { id: "item-2", status: "accepted", favorite_type: "grammar", pack_ids: [] },
        ],
      },
    });
  });

  it("exposes local review, preview activity, and compatibility migration commands", async () => {
    invoke
      .mockResolvedValueOnce({ date: "2026-07-14", material_id: null, total_events: 2, event_counts: { read: 1, accept: 1 }, unique_learning_items: 1, events: [] })
      .mockResolvedValueOnce({ date: null, material_id: "material-1", total_events: 1, event_counts: { read: 1 }, unique_learning_items: 0, events: [] })
      .mockResolvedValueOnce({ id: "event-1", learning_item_id: "item-1", material_id: "material-1", event_type: "local_preview", metadata: {}, occurred_at: "2026-07-14T00:00:00Z" })
      .mockResolvedValueOnce({ dry_run: true, planned: 1, migrated: 0, already_migrated: 2, conflicts: [] });
    const api = createLearningWorkbenchApi();

    await api.getDailyReview?.(480);
    await api.getMaterialReview?.("material-1");
    await api.recordLocalPreview?.("item-1", { origin: "test" });
    await api.migrateLegacy?.(true);

    expect(invoke).toHaveBeenNthCalledWith(1, "get_daily_learning_review_cmd", { query: { timezone_offset_minutes: 480 } });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_material_learning_review_cmd", { materialId: "material-1" });
    expect(invoke).toHaveBeenNthCalledWith(3, "record_local_preview_cmd", {
      id: "item-1",
      payload: {
        metadata: { origin: "test" },
        idempotency_key: expect.stringMatching(/^local-preview:item-1:/),
      },
    });
    expect(invoke).toHaveBeenNthCalledWith(4, "migrate_legacy_learning_items_cmd", { payload: { dry_run: true } });
  });
});
