import { expect, test } from "@playwright/test";

declare global {
  interface Window {
    __openkotoInvokeCalls: Array<{ command: string; args: Record<string, unknown> }>;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
    __TAURI_INTERNALS__: unknown;
  }
}

test("Assistant task center observes, controls, and traces local workflows", async ({ page }) => {
  await page.addInitScript(() => {
    const now = "2026-07-15T08:00:00Z";
    const article = {
      id: "article-1",
      title: "Clinical Evidence",
      content: "Genomic evidence changes clinical interpretation.",
      source_type: "article",
      created_at: now,
      translated: false,
      metadata: {},
      tags: [],
      segments: [{
        id: "segment-1",
        article_id: "article-1",
        order: 0,
        text: "Genomic evidence changes clinical interpretation.",
        created_at: now,
        is_new_paragraph: true,
      }],
    };
    const sourceInput = {
      prompt: "Trace the claim",
      source_locator: {
        version: 1,
        reader_kind: "article",
        kind: "text_range",
        segment_id: "segment-1",
        segment_order: 0,
        start_offset: 0,
        end_offset: 7,
        quote: { exact: "Genomic" },
      },
      learning_item_id: "learning-1",
    };
    const baseTask = {
      article_id: "article-1",
      artifact_ids: [],
      created_at: now,
      updated_at: now,
      root_task_id: null,
      retry_of_task_id: null,
      attempt: 1,
      output_version: "1",
    };
    let tasks = [
      { ...baseTask, id: "task-running", task_type: "assistant_agent_turn", status: "running", progress: 0.45, stage: "reasoning", message: "Reading evidence", input_snapshot: sourceInput, started_at: now, finished_at: null },
      { ...baseTask, id: "task-failed", task_type: "mind_map_generate", status: "failed", progress: 0.7, stage: "render", message: "Render failed", error: "Worker timed out", input_snapshot: { prompt: "Map the article" }, started_at: now, finished_at: now },
      { ...baseTask, id: "task-success", task_type: "structured_report", status: "succeeded", progress: 1, stage: "completed", message: "Report ready", input_snapshot: {}, artifact_ids: ["artifact-missing"], started_at: now, finished_at: now },
    ];
    const linkedItem = {
      id: "learning-1",
      material_id: "article-1",
      segment_id: "segment-1",
      item_type: "word",
      text: "Genomic",
      source_sentence: "Genomic evidence changes clinical interpretation.",
      context_before: null,
      context_after: null,
      meaning_in_context: "related to the genome",
      definition_en: null,
      definition_zh: null,
      collocations: [],
      examples: [],
      tags: [],
      quality_flags: [],
      status: "accepted",
      priority: 0,
      difficulty: null,
      ai_explanation: null,
      review_state: {},
      source_material_title: "Clinical Evidence",
      source_type: "article",
      source_segment_order: 0,
      accepted_at: now,
      rejected_at: null,
      created_at: now,
      updated_at: now,
    };
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const callbacks: Record<number, (...args: unknown[]) => unknown> = {};
    let nextCallbackId = 1;

    const detailFor = (taskId: string) => {
      const task = tasks.find((candidate) => candidate.id === taskId);
      if (!task) throw new Error(`missing mock task ${taskId}`);
      const rootId = task.root_task_id || task.id;
      return {
        task,
        retry_lineage: tasks
          .filter((candidate) => candidate.id === rootId || candidate.root_task_id === rootId)
          .map((candidate) => ({ id: candidate.id, status: candidate.status, attempt: candidate.attempt, created_at: candidate.created_at })),
      };
    };

    Object.assign(window, {
      __openkotoInvokeCalls: calls,
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
        callbacks,
        transformCallback: (callback: (...args: unknown[]) => unknown) => {
          const id = nextCallbackId++;
          callbacks[id] = callback;
          return id;
        },
        unregisterCallback: (id: number) => { delete callbacks[id]; },
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          calls.push({ command, args });
          if (command === "plugin:event|listen") return calls.length;
          if (command === "plugin:event|unlisten") return null;
          if (command === "packaged_backend_status_cmd") return { enabled: true, running: true, message: "ready" };
          if (command === "get_config") return { onboarding_completed: true, interface_language: "zh-CN", target_language: "zh-CN", model_configs: [] };
          if (command === "backend_check_session_cmd") return { configured: true, connected: true, authenticated: true, backend_url: "http://127.0.0.1:19421", user: { id: "user-1", email: "reader@example.com", display_name: "Reader", created_at: now, updated_at: now }, error: null };
          if (command === "list_articles_cmd") return [article];
          if (command === "get_article") return article;
          if (command === "material_library_get_reading_progress_cmd") return null;
          if (command === "get_resource_server_info_cmd") return { base_url: "http://127.0.0.1:19420", token: "test-token" };
          if (command === "list_annotations_cmd") return [];
          if (command === "assistant_task_list_cmd") {
            const query = (args.query || {}) as { status?: string; article_id?: string };
            const visibleTasks = tasks.filter((task) => (!query.status || task.status === query.status) && (!query.article_id || task.article_id === query.article_id));
            return { items: visibleTasks, total: visibleTasks.length, limit: 100, offset: 0 };
          }
          if (command === "assistant_task_detail_cmd") return detailFor(String(args.taskId));
          if (command === "assistant_task_timeline_cmd") {
            const task = tasks.find((candidate) => candidate.id === args.taskId)!;
            return [{ id: `event-${task.id}`, task_id: task.id, event_type: "status_changed", from_status: "queued", to_status: task.status, stage: task.stage, message: task.message, level: task.status === "failed" ? "error" : "info", metadata: {}, created_at: now }];
          }
          if (command === "assistant_task_artifacts_cmd") {
            return args.taskId === "task-success" ? [{ id: "artifact-missing", task_id: "task-success", article_id: "article-1", artifact_type: "file", output_version: "2", content: null, metadata: { file_name: "deleted-report.md" }, file_available: false, created_at: now, updated_at: now }] : [];
          }
          if (command === "assistant_task_cancel_cmd") {
            tasks = tasks.map((task) => task.id === args.taskId ? { ...task, status: "cancelled", finished_at: now } : task) as typeof tasks;
            return detailFor(String(args.taskId));
          }
          if (command === "assistant_task_retry_cmd") {
            const original = tasks.find((task) => task.id === args.taskId)!;
            const retried = { ...original, id: "task-retry", status: "queued", progress: 0, error: null, stage: "queued", message: "Retry queued", root_task_id: original.root_task_id || original.id, retry_of_task_id: original.id, attempt: 2, created_at: now, updated_at: now, started_at: null, finished_at: null };
            tasks = [retried, ...tasks] as typeof tasks;
            return detailFor(retried.id);
          }
          if (command === "list_learning_items_cmd") {
            const query = (args.query || {}) as { status?: string };
            return query.status === "accepted" ? [linkedItem] : [];
          }
          if (command === "get_learning_item_cmd") return linkedItem;
          if (command === "get_daily_learning_review_cmd") return { total_events: 0, event_counts: {}, unique_learning_items: 0, events: [] };
          return null;
        },
      },
    });
  });

  await page.goto("http://127.0.0.1:1420/");
  await page.getByRole("button", { name: "Assistant" }).click();
  const taskList = page.getByRole("list", { name: "Assistant 任务列表" });
  await expect(taskList.getByRole("listitem")).toHaveCount(3);
  await expect(page.getByText("other-user-secret-task")).toHaveCount(0);

  await page.getByRole("button", { name: "取消任务" }).click();
  await expect(page.getByTestId("assistant-task-detail-task-running").getByText("已取消", { exact: true })).toBeVisible();

  await taskList.getByRole("button").filter({ hasText: "生成思维导图" }).click();
  await expect(page.getByRole("heading", { name: "失败原因" })).toBeVisible();
  await page.getByRole("button", { name: "重试", exact: true }).click();
  await expect(page.getByTestId("assistant-task-detail-task-retry")).toBeVisible();
  await expect(page.getByTestId("assistant-task-detail-task-retry").getByText("尝试 2", { exact: true })).toBeVisible();

  await taskList.getByRole("button").filter({ hasText: "结构化报告" }).click();
  await expect(page.getByText("产物文件不可用")).toBeVisible();
  await expect(page.getByText(/deleted-report\.md/)).toBeVisible();

  await taskList.getByRole("button").filter({ hasText: "Assistant 任务" }).click();
  await page.getByRole("button", { name: "任务输入来源 · 原文" }).click();
  await expect(page.getByLabel("主导航")).toHaveCount(0);
  await expect(page.getByText("Genomic evidence changes clinical interpretation.").first()).toBeVisible();
  await page.getByRole("button", { name: "任务" }).click();
  await expect(page.getByText("当前素材的工作流")).toBeVisible();
  await page.getByRole("button", { name: "返回" }).click();

  await expect(page.getByTestId("assistant-task-center")).toBeVisible();
  await taskList.getByRole("button").filter({ hasText: "Assistant 任务" }).click();
  await page.getByRole("button", { name: "任务输入来源 · 学习项" }).click();
  await expect(page.getByTestId("learning-workbench")).toBeVisible();
  await expect(page.getByLabel("编辑学习内容")).toHaveValue("Genomic");

  const calls = await page.evaluate(() => window.__openkotoInvokeCalls);
  expect(calls).toEqual(expect.arrayContaining([
    expect.objectContaining({ command: "assistant_task_cancel_cmd", args: { taskId: "task-running" } }),
    expect.objectContaining({ command: "assistant_task_retry_cmd", args: { taskId: "task-failed" } }),
    expect.objectContaining({ command: "get_learning_item_cmd", args: { id: "learning-1" } }),
  ]));
});
