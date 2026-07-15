import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type {
  AssistantArtifact,
  AssistantTaskDetail,
  AssistantTasksApi,
} from "../../features/assistant";
import type { Article } from "../../types";
import { AssistantTaskCenter } from "./AssistantTaskCenter";

afterEach(cleanup);

const ARTICLE: Article = {
  id: "article-1",
  title: "Clinical Trial Reading",
  content: "Alpha beta gamma.",
  source_type: "article",
  created_at: "2026-07-15T08:00:00Z",
  translated: false,
};

function task(overrides: Partial<AssistantTaskDetail> = {}): AssistantTaskDetail {
  return {
    id: "task-1",
    task_type: "assistant_agent_turn",
    status: "running",
    article_id: ARTICLE.id,
    input: {
      prompt: "Explain the result",
      source_locator: {
        version: 1,
        kind: "text_range",
        start_offset: 0,
        end_offset: 5,
        quote: { exact: "Alpha" },
      },
      learning_item_id: "learning-1",
    },
    progress: 0.4,
    stage: "reasoning",
    message: "Reading evidence",
    error: null,
    worker_session_id: "worker-1",
    artifact_ids: [],
    created_at: "2026-07-15T08:00:00Z",
    updated_at: "2026-07-15T08:01:00Z",
    started_at: "2026-07-15T08:00:10Z",
    finished_at: null,
    retry_attempt: 1,
    retry_lineage: [],
    ...overrides,
  };
}

function apiFor(getCurrent: () => AssistantTaskDetail, artifacts: AssistantArtifact[] = []): AssistantTasksApi {
  return {
    list: vi.fn(async () => ({ items: [getCurrent()], total: 1 })),
    detail: vi.fn(async () => getCurrent()),
    timeline: vi.fn(async () => [{
      id: "event-1",
      task_id: getCurrent().id,
      event_type: "status_changed",
      status: getCurrent().status,
      from_status: "queued" as const,
      to_status: getCurrent().status,
      stage: "reasoning",
      message: "Worker accepted task",
      level: "info" as const,
      details: {},
      occurred_at: "2026-07-15T08:00:10Z",
    }]),
    artifacts: vi.fn(async () => artifacts),
    cancel: vi.fn(async () => getCurrent()),
    retry: vi.fn(async () => getCurrent()),
  };
}

describe("AssistantTaskCenter", () => {
  it("shows the current user's workflow, evidence timeline, and source navigation", async () => {
    const current = task();
    const api = apiFor(() => current);
    const onNavigateSource = vi.fn();

    render(<AssistantTaskCenter api={api} articles={[ARTICLE]} onNavigateSource={onNavigateSource} />);

    const list = await screen.findByRole("list", { name: "Assistant 任务列表" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(1);
    expect(within(list).getByRole("button")).toBeInTheDocument();
    expect(screen.getAllByText("Clinical Trial Reading").length).toBeGreaterThan(0);
    expect(screen.queryByText("other-user-secret-task")).not.toBeInTheDocument();
    expect(await screen.findByText("排队中 → 运行中")).toBeInTheDocument();
    expect(screen.getByText("尝试 1")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "任务输入来源 · 原文" }));
    expect(onNavigateSource).toHaveBeenCalledWith(expect.objectContaining({
      target: "source",
      articleId: ARTICLE.id,
      locator: expect.objectContaining({ kind: "text_range" }),
    }));
    await userEvent.click(screen.getByRole("button", { name: "任务输入来源 · 学习项" }));
    expect(onNavigateSource).toHaveBeenCalledWith(expect.objectContaining({
      target: "learning_item",
      learningItemId: "learning-1",
    }));
  });

  it("cancels a running task and retries a failed task as a new attempt", async () => {
    let current = task();
    const api = apiFor(() => current);
    vi.mocked(api.cancel).mockImplementation(async () => {
      current = task({ status: "cancelled", finished_at: "2026-07-15T08:02:00Z", progress: 0.4 });
      return current;
    });
    vi.mocked(api.retry).mockImplementation(async () => {
      current = task({
        id: "task-2",
        status: "queued",
        progress: 0,
        retry_of_task_id: "task-1",
        retry_root_task_id: "task-1",
        retry_attempt: 2,
        retry_lineage: [
          { task_id: "task-1", status: "failed", attempt: 1 },
          { task_id: "task-2", status: "queued", attempt: 2 },
        ],
      });
      return current;
    });

    render(<AssistantTaskCenter api={api} articles={[ARTICLE]} onNavigateSource={() => undefined} />);
    await userEvent.click(await screen.findByRole("button", { name: "取消任务" }));
    await waitFor(() => expect(api.cancel).toHaveBeenCalledWith("task-1"));
    await waitFor(() => expect(screen.getAllByText("已取消").length).toBeGreaterThan(1));

    current = task({ status: "failed", error: "Model request timed out" });
    await userEvent.click(screen.getByRole("button", { name: "刷新 Assistant 任务" }));
    await waitFor(() => expect(screen.getByText("失败原因")).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(() => expect(api.retry).toHaveBeenCalledWith("task-1"));
    expect(await screen.findByTestId("assistant-task-detail-task-2")).toBeInTheDocument();
    expect(screen.getByText("尝试 2")).toBeInTheDocument();
  });

  it("reports missing artifacts without hiding the task record", async () => {
    const current = task({ status: "succeeded", progress: 1, finished_at: "2026-07-15T08:02:00Z" });
    const api = apiFor(() => current, [{
      id: "artifact-file",
      task_id: current.id,
      article_id: ARTICLE.id,
      artifact_type: "file",
      version: "2",
      content: null,
      metadata: { file_name: "deleted-report.md" },
      file_available: false,
      created_at: current.created_at,
      updated_at: current.updated_at,
    }]);

    render(<AssistantTaskCenter api={api} articles={[ARTICLE]} onNavigateSource={() => undefined} />);
    expect(await screen.findByText("产物文件不可用")).toBeInTheDocument();
    expect(screen.getByText(/deleted-report\.md/)).toBeInTheDocument();
  });

  it("keeps task details visible when artifact loading fails independently", async () => {
    const current = task({ status: "succeeded", progress: 1, finished_at: "2026-07-15T08:02:00Z" });
    const api = apiFor(() => current);
    vi.mocked(api.artifacts).mockRejectedValue({ code: "assistant_artifact_not_found", message: "artifact file missing" });

    render(<AssistantTaskCenter api={api} articles={[ARTICLE]} onNavigateSource={() => undefined} />);

    expect(await screen.findByTestId("assistant-task-detail-task-1")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("任务产物不可用：artifact file missing");
  });

  it("keeps the failure boundary local when the task service is offline", async () => {
    const api = apiFor(() => task());
    vi.mocked(api.list).mockRejectedValue({ code: "offline", message: "Backend unavailable" });

    render(<AssistantTaskCenter api={api} articles={[ARTICLE]} onNavigateSource={() => undefined} />);

    expect(await screen.findByRole("alert")).toHaveTextContent("无法连接任务服务");
    expect(screen.getByRole("alert")).toHaveTextContent("Backend unavailable");
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
  });
});
