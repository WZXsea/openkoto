import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@tauri-apps/api/core", () => ({ invoke }));

import { createAssistantTasksApi } from "./api";

describe("Assistant task Tauri adapter", () => {
  beforeEach(() => invoke.mockReset());

  it("preserves list filters and normalizes persisted observability fields", async () => {
    invoke.mockResolvedValueOnce({
      items: [{
        id: "task-2",
        task_type: "assistant_agent_turn",
        status: "running",
        article_id: "article-1",
        input: { ignored: true },
        input_snapshot: { prompt: "Explain the result" },
        progress: 65,
        root_task_id: "task-1",
        retry_of_task_id: "task-1",
        attempt: 2,
        artifact_ids: ["artifact-1"],
        created_at: "2026-07-15T08:00:00Z",
        updated_at: "2026-07-15T08:01:00Z",
      }],
      total: 1,
      limit: 25,
      offset: 0,
    });
    const api = createAssistantTasksApi();

    const response = await api.list({ status: "running", article_id: "article-1", limit: 25, offset: 0 });

    expect(invoke).toHaveBeenCalledWith("assistant_task_list_cmd", {
      query: { status: "running", article_id: "article-1", limit: 25, offset: 0 },
    });
    expect(response).toEqual(expect.objectContaining({ total: 1, limit: 25, offset: 0 }));
    expect(response.items[0]).toEqual(expect.objectContaining({
      input: { prompt: "Explain the result" },
      progress: 0.65,
      retry_root_task_id: "task-1",
      retry_attempt: 2,
    }));
  });

  it("uses detail action commands and prioritizes timeline transitions", async () => {
    const task = {
      id: "task-1",
      task_type: "mind_map_generate",
      status: "failed",
      article_id: "article-1",
      input_snapshot: {},
      artifact_ids: [],
      progress: 0.4,
      created_at: "2026-07-15T08:00:00Z",
      updated_at: "2026-07-15T08:01:00Z",
    };
    invoke
      .mockResolvedValueOnce({ task, retry_lineage: [{ id: "task-1", status: "failed", attempt: 1 }] })
      .mockResolvedValueOnce([{ id: "event-1", task_id: "task-1", event_type: "status_changed", from_status: "running", to_status: "failed", metadata: { exit_code: 1, level: "warn" }, created_at: "2026-07-15T08:01:00Z" }])
      .mockResolvedValueOnce({ ...task, status: "cancelled" })
      .mockResolvedValueOnce({ ...task, id: "task-2", status: "queued", attempt: 2 });
    const api = createAssistantTasksApi();

    await expect(api.detail("task-1")).resolves.toEqual(expect.objectContaining({ retry_lineage: [expect.objectContaining({ task_id: "task-1", attempt: 1 })] }));
    await expect(api.timeline("task-1")).resolves.toEqual([expect.objectContaining({
      status: "failed",
      from_status: "running",
      to_status: "failed",
      level: "warn",
      details: { exit_code: 1, level: "warn" },
      occurred_at: "2026-07-15T08:01:00Z",
    })]);
    await api.cancel("task-1");
    await api.retry("task-1");

    expect(invoke).toHaveBeenNthCalledWith(1, "assistant_task_detail_cmd", { taskId: "task-1" });
    expect(invoke).toHaveBeenNthCalledWith(2, "assistant_task_timeline_cmd", { taskId: "task-1" });
    expect(invoke).toHaveBeenNthCalledWith(3, "assistant_task_cancel_cmd", { taskId: "task-1" });
    expect(invoke).toHaveBeenNthCalledWith(4, "assistant_task_retry_cmd", { taskId: "task-1" });
  });

  it("loads complete retry lineage by root task id for direct task details", async () => {
    const directDetail = {
      id: "task-2",
      task_type: "assistant_agent_turn",
      status: "failed",
      article_id: "article-1",
      input_snapshot: {},
      artifact_ids: [],
      progress: 0.8,
      root_task_id: "task-1",
      retry_of_task_id: "task-1",
      attempt: 2,
      created_at: "2026-07-15T08:01:00Z",
      updated_at: "2026-07-15T08:02:00Z",
    };
    invoke
      .mockResolvedValueOnce(directDetail)
      .mockResolvedValueOnce({ items: [
        { ...directDetail, id: "task-1", status: "failed", root_task_id: "task-1", retry_of_task_id: null, attempt: 1 },
        directDetail,
      ], total: 2, limit: 200, offset: 0 });

    await expect(createAssistantTasksApi().detail("task-2")).resolves.toEqual(expect.objectContaining({
      retry_lineage: [
        expect.objectContaining({ task_id: "task-1", attempt: 1 }),
        expect.objectContaining({ task_id: "task-2", attempt: 2 }),
      ],
    }));
    expect(invoke).toHaveBeenNthCalledWith(2, "assistant_task_list_cmd", {
      query: { root_task_id: "task-1", limit: 200, offset: 0 },
    });
  });

  it("normalizes artifact output versions and missing file state", async () => {
    invoke.mockResolvedValueOnce({ items: [{
      id: "artifact-1",
      task_id: "task-1",
      material_id: "article-1",
      artifact_type: "file",
      output_version: 3,
      content: null,
      metadata: { file_name: "report.md" },
      file_available: false,
      created_at: "2026-07-15T08:00:00Z",
      updated_at: "2026-07-15T08:00:00Z",
    }] });

    await expect(createAssistantTasksApi().artifacts("task-1")).resolves.toEqual([
      expect.objectContaining({ article_id: "article-1", version: "3", file_available: false }),
    ]);
    expect(invoke).toHaveBeenCalledWith("assistant_task_artifacts_cmd", { taskId: "task-1" });
  });
});
