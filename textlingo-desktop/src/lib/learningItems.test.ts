import { afterEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  createLearningItem,
  createLearningItemFromSelection,
  deleteLearningItem,
  listLearningItems,
  updateLearningItem,
} from "./learningItems";

describe("learningItems API helper", () => {
  afterEach(() => invokeMock.mockReset());

  it("routes learning item helpers to Tauri commands", async () => {
    invokeMock.mockResolvedValue({ id: "item-1" });

    await listLearningItems({ status: "candidate", item_type: "word", limit: 50 });
    expect(invokeMock).toHaveBeenCalledWith("list_learning_items_cmd", {
      query: { status: "candidate", item_type: "word", limit: 50 },
    });

    await createLearningItem({
      item_type: "word",
      text: "mitigate",
      source_sentence: "This can mitigate risk.",
      status: "candidate",
      tags: ["academic"],
    });
    expect(invokeMock).toHaveBeenCalledWith("create_learning_item_cmd", {
      payload: {
        item_type: "word",
        text: "mitigate",
        source_sentence: "This can mitigate risk.",
        status: "candidate",
        tags: ["academic"],
      },
    });

    await createLearningItemFromSelection({
      material_id: "3a8f4371-0f10-4d2a-8140-287f8c052441",
      segment_id: "4e79490f-17b2-42eb-86d4-97bf2548eff7",
      selected_text: "mitigate",
      source_sentence: "This can mitigate risk.",
    });
    expect(invokeMock).toHaveBeenCalledWith("create_learning_item_from_selection_cmd", {
      payload: {
        material_id: "3a8f4371-0f10-4d2a-8140-287f8c052441",
        segment_id: "4e79490f-17b2-42eb-86d4-97bf2548eff7",
        selected_text: "mitigate",
        source_sentence: "This can mitigate risk.",
      },
    });

    await updateLearningItem("item-1", { status: "accepted", meaning_in_context: "reduce" });
    expect(invokeMock).toHaveBeenCalledWith("update_learning_item_cmd", {
      id: "item-1",
      payload: { status: "accepted", meaning_in_context: "reduce" },
    });

    await deleteLearningItem("item-1");
    expect(invokeMock).toHaveBeenCalledWith("delete_learning_item_cmd", { id: "item-1" });
  });
});
