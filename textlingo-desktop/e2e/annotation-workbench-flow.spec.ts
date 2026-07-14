import { expect, test } from "@playwright/test";

declare global {
  interface Window {
    __openkotoInvokeCalls: Array<{ command: string; args: Record<string, unknown> }>;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
    __TAURI_INTERNALS__: unknown;
  }
}

test("annotation workbench edits, converts, and returns to the source", async ({ page }) => {
  await page.addInitScript(() => {
    const now = "2026-07-14T00:00:00Z";
    const article = {
      id: "material-1",
      title: "Annotation Source",
      content: "Genomic evidence changes clinical interpretation.",
      source_type: "article",
      created_at: now,
      translated: false,
      metadata: {},
      tags: [],
      segments: [{ id: "11111111-1111-4111-8111-111111111111", article_id: "material-1", order: 0, text: "Genomic evidence changes clinical interpretation.", created_at: now, is_new_paragraph: true }],
    };
    let annotation = {
      id: "annotation-1",
      material_id: "material-1",
      segment_id: "11111111-1111-4111-8111-111111111111",
      kind: "highlight",
      locator: { version: 1, reader_kind: "article", kind: "text_range", segment_id: "11111111-1111-4111-8111-111111111111", segment_order: 0, start_offset: 0, end_offset: 7, quote: { exact: "Genomic" } },
      source_text: "Genomic evidence changes clinical interpretation.",
      color: "#facc15",
      note: "Initial note",
      tags: ["research"],
      learning_item_id: null,
      created_at: now,
      updated_at: now,
    };
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const callbacks: Record<number, (...args: unknown[]) => unknown> = {};
    let nextCallbackId = 1;

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
          if (command === "material_library_list_tags_cmd" || command === "material_library_list_import_jobs_cmd") return [];
          if (command === "material_library_get_reading_progress_cmd") return null;
          if (command === "list_learning_items_cmd") return [];
          if (command === "list_annotations_cmd") return [annotation];
          if (command === "update_annotation_cmd") {
            annotation = { ...annotation, ...(args.payload as object), updated_at: now };
            return annotation;
          }
          if (command === "convert_annotation_to_learning_item_cmd") {
            annotation = { ...annotation, learning_item_id: "learning-1" };
            return { annotation, learning_item: { id: "learning-1", text: "Genomic", source_sentence: annotation.source_text } };
          }
          if (command === "get_resource_server_info_cmd") return { base_url: "http://127.0.0.1:19420", token: "test-token" };
          return null;
        },
      },
    });
  });

  await page.goto("http://127.0.0.1:1420/");
  await page.getByRole("button", { name: "批注" }).click();
  await expect(page.getByRole("region", { name: "批注工作台" })).toContainText("Genomic evidence");

  await page.getByLabel("Genomic evidence changes clinical interpretation. 笔记").fill("Evidence note");
  await page.getByLabel("保存批注 Genomic evidence changes clinical interpretation.").click();
  await page.getByLabel("转换为学习候选 Genomic evidence changes clinical interpretation.").click();
  await page.getByLabel("回到原文 Genomic evidence changes clinical interpretation.").click();

  await expect(page.getByText("Genomic evidence changes clinical interpretation.").first()).toBeVisible();
  await expect(page.getByTestId("article-annotation-status")).toBeVisible();
  const calls = await page.evaluate(() => window.__openkotoInvokeCalls);
  expect(calls).toEqual(expect.arrayContaining([
    expect.objectContaining({ command: "update_annotation_cmd" }),
    expect.objectContaining({ command: "convert_annotation_to_learning_item_cmd" }),
  ]));
});
