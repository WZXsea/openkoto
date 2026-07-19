import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { createAnnotationsApi } from "./api";

const createPayload = {
  material_id: "material-1",
  kind: "highlight" as const,
  locator: {
    version: 1 as const,
    kind: "text_range" as const,
    start_offset: 0,
    end_offset: 8,
    quote: { exact: "language" },
  },
  source_text: "language",
  client_request_id: "annotation-request-1",
};

describe("annotations Tauri adapter", () => {
  beforeEach(() => invoke.mockReset());

  it("uses the list, update, delete and conversion command contracts", async () => {
    invoke.mockResolvedValue([]);
    const api = createAnnotationsApi();

    await api.list({ material_id: "material-1", kind: "vocabulary", tag: "academic", q: "mitigate", limit: 20, offset: 10 });
    await api.update("annotation-1", { note: "review", color: "#facc15", tags: ["academic"] });
    await api.remove("annotation-1");
    await api.convertToLearningItem("annotation-1");

    expect(invoke).toHaveBeenNthCalledWith(1, "list_annotations_cmd", {
      query: { material_id: "material-1", kind: "vocabulary", tag: "academic", q: "mitigate", limit: 20, offset: 10 },
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "update_annotation_cmd", {
      id: "annotation-1",
      payload: { note: "review", color: "#facc15", tags: ["academic"] },
    });
    expect(invoke).toHaveBeenNthCalledWith(3, "delete_annotation_cmd", { id: "annotation-1" });
    expect(invoke).toHaveBeenNthCalledWith(4, "convert_annotation_to_learning_item_cmd", { id: "annotation-1" });
  });

  it("preserves a caller-stable client request id across an idempotent retry", async () => {
    invoke.mockResolvedValue({ id: "annotation-1" });
    const api = createAnnotationsApi();

    await api.create(createPayload);
    await api.create(createPayload);

    expect(invoke).toHaveBeenNthCalledWith(1, "create_annotation_cmd", { payload: createPayload });
    expect(invoke).toHaveBeenNthCalledWith(2, "create_annotation_cmd", { payload: createPayload });
  });
});
