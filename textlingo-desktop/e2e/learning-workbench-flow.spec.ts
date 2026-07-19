import { expect, test } from "@playwright/test";

declare global {
  interface Window {
    __openkotoInvokeCalls: Array<{ command: string; args: unknown }>;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
    __TAURI_INTERNALS__: unknown;
  }
}

test("learning workbench filters, accepts, previews, and checks compatibility data", async ({ page }) => {
  await page.addInitScript(() => {
    const article = {
      id: "article-1",
      title: "Clinical Reading",
      content: "Early action can mitigate treatment-related risk.",
      source_type: "article",
      created_at: "2026-07-14T00:00:00Z",
      translated: false,
      segments: [],
    };
    let item = {
      id: "item-1",
      material_id: "article-1",
      segment_id: null,
      item_type: "word",
      text: "mitigate",
      source_sentence: "Early action can mitigate treatment-related risk.",
      context_before: "Early action can",
      context_after: "treatment-related risk.",
      meaning_in_context: "reduce severity",
      definition_en: "make less severe",
      definition_zh: "减轻",
      collocations: [],
      examples: [],
      tags: ["oncology"],
      quality_flags: ["needs_verification"],
      status: "candidate",
      priority: 0,
      difficulty: null,
      ai_explanation: null,
      review_state: {},
      source_material_title: "Clinical Reading",
      source_type: "article",
      source_segment_order: null,
      accepted_at: null,
      rejected_at: null,
      status_before_archive: null,
      merged_into_id: null,
      created_at: "2026-07-14T00:00:00Z",
      updated_at: "2026-07-14T00:00:00Z",
    };
    const calls: Array<{ command: string; args: unknown }> = [];
    const callbacks: Record<number, (...args: unknown[]) => unknown> = {};
    let nextCallbackId = 1;

    Object.assign(window, {
      __openkotoInvokeCalls: calls,
      __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener: () => {} },
      __TAURI_INTERNALS__: {
        metadata: {
          currentWindow: { label: "main" },
          currentWebview: { windowLabel: "main", label: "main" },
        },
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
          if (command === "get_config") {
            return {
              onboarding_completed: true,
              target_language: "zh-CN",
              interface_language: "zh-CN",
              active_model_id: "model-1",
              model_configs: [{ id: "model-1", name: "Mock", api_provider: "openai", api_key: "test", model: "mock", is_default: true }],
            };
          }
          if (command === "backend_check_session_cmd") {
            return {
              configured: true,
              connected: true,
              authenticated: true,
              backend_url: "http://127.0.0.1:19421",
              user: { id: "user-1", email: "reader@example.com", display_name: "Reader", created_at: "2026-07-14T00:00:00Z", updated_at: "2026-07-14T00:00:00Z" },
              error: null,
            };
          }
          if (command === "list_articles_cmd") return [article];
          if (command === "get_resource_server_info_cmd") return { base_url: "http://127.0.0.1:19420", token: "test-token" };
          if (command === "list_learning_items_cmd") {
            const query = (args.query || {}) as { status?: string };
            return !query.status || query.status === item.status ? [item] : [];
          }
          if (command === "get_daily_learning_review_cmd") {
            return { date: "2026-07-14", material_id: null, total_events: 2, event_counts: { create: 1, accept: 1 }, unique_learning_items: 1, events: [] };
          }
          if (command === "bulk_organize_learning_items_cmd") {
            const payload = args.payload as { items: Array<{ id: string; status?: string }> };
            item = { ...item, status: payload.items[0].status || item.status, accepted_at: "2026-07-14T00:01:00Z" };
            return { succeeded: 1, failed: 0, results: [{ id: item.id, success: true, item }] };
          }
          if (command === "record_local_preview_cmd") {
            return { id: "event-1", learning_item_id: item.id, material_id: article.id, event_type: "local_preview", metadata: {}, occurred_at: "2026-07-14T00:02:00Z" };
          }
          if (command === "migrate_legacy_learning_items_cmd") {
            return { dry_run: true, planned: 1, migrated: 0, already_migrated: 0, conflicts: [] };
          }
          return null;
        },
      },
    });
  });

  await page.goto("http://127.0.0.1:1420/");
  await page.getByRole("button", { name: "Learning Workbench", exact: true }).click();
  await expect(page.getByTestId("learning-workbench")).toBeVisible();
  await expect(page.getByText("mitigate", { exact: true })).toBeVisible();
  await expect(page.getByTestId("learning-item-item-1").getByText("待人工核验", { exact: true })).toBeVisible();
  await expect(page.getByText("活动 2")).toBeVisible();

  await page.getByLabel("选择 mitigate").check();
  await page.getByRole("button", { name: "批量接受" }).click();
  await expect(page.getByText("暂无匹配的学习项")).toBeVisible();
  await page.getByLabel("筛选状态").selectOption("accepted");
  await expect(page.getByText("mitigate", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "本地预习" }).click();
  await page.getByRole("button", { name: "兼容数据检查" }).click();
  await expect(page.getByText(/计划 1，完成 0，冲突 0/)).toBeVisible();

  const calls = await page.evaluate(() => window.__openkotoInvokeCalls);
  expect(calls).toEqual(expect.arrayContaining([
    expect.objectContaining({ command: "bulk_organize_learning_items_cmd" }),
    expect.objectContaining({ command: "record_local_preview_cmd" }),
    expect.objectContaining({ command: "migrate_legacy_learning_items_cmd", args: { payload: { dry_run: true } } }),
  ]));
});
