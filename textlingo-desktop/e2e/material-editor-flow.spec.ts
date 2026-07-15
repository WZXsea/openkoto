import { expect, test } from "@playwright/test";

declare global {
  interface Window {
    __openkotoEditorCalls: Array<{ command: string; args: Record<string, unknown> }>;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
    __TAURI_INTERNALS__: unknown;
  }
}

test("structured material editor previews impact, commits, restores, and protects dirty navigation", async ({ page }) => {
  await page.addInitScript(() => {
    const now = "2026-07-15T12:00:00Z";
    const noActivity = Array.from({ length: 84 }, (_, index) => ({
      date: new Date(Date.UTC(2026, 3, 23 + index)).toISOString().slice(0, 10),
      read_materials: 0,
      learning_actions: 0,
      activity_score: 0,
    }));
    const impact = {
      inserted_blocks: 1,
      updated_blocks: 0,
      deleted_blocks: 0,
      moved_blocks: 0,
      changed_segments: 1,
      deleted_segments: 0,
      stale_readings: 0,
      stale_translations: 1,
      stale_explanations: 0,
      affected_annotations: 1,
      affected_learning_items: 0,
    };
    let document = {
      material_id: "material-editor",
      title: "HIF review",
      current_revision: 2,
      content_sha256: "a".repeat(64),
      blocks: [
        { id: "block-heading", block_type: "heading", block_order: 0, text: "HIF review", attrs: { level: 2 }, segments: [] },
        { id: "block-body", block_type: "paragraph", block_order: 1, text: "HIF signalling changes transcription.", attrs: {}, segments: [] },
      ],
      segments: [],
      derived_summary: { translations: 1 },
    };
    let article = {
      id: "material-editor",
      title: "HIF review",
      content: "HIF review\n\nHIF signalling changes transcription.",
      source_type: "article",
      created_at: now,
      translated: true,
      material_revision: document.current_revision,
      content_sha256: document.content_sha256,
      metadata: {},
      tags: [],
      segments: [{ id: "segment-1", article_id: "material-editor", order: 0, text: "HIF signalling changes transcription.", translation: "HIF 信号改变转录。", created_at: now, is_new_paragraph: true }],
    };
    const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
    const callbacks: Record<number, (...args: unknown[]) => unknown> = {};
    let nextCallbackId = 1;

    Object.assign(window, {
      __openkotoEditorCalls: calls,
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
          if (command === "get_learning_activity_heatmap_cmd") return { start_date: noActivity[0].date, end_date: noActivity.at(-1)?.date, days: noActivity };
          if (command === "material_library_list_tags_cmd" || command === "material_library_list_import_jobs_cmd" || command === "list_learning_items_cmd" || command === "list_annotations_cmd") return [];
          if (command === "material_library_get_reading_progress_cmd") return null;
          if (command === "material_library_upsert_reading_progress_cmd") return null;
          if (command === "get_resource_server_info_cmd") return { base_url: "http://127.0.0.1:19420", token: "test-token" };
          if (command === "get_material_document_cmd") return document;
          if (command === "get_material_draft_cmd") return null;
          if (command === "save_material_draft_cmd") return { material_id: document.material_id, ...(args.payload as object), updated_at: now, is_stale: false };
          if (command === "delete_material_draft_cmd") return null;
          if (command === "preview_material_edit_cmd") return { base_revision: document.current_revision, next_revision: 3, content_sha256: "b".repeat(64), preview_token: "preview-e2e", impact };
          if (command === "commit_material_edit_cmd") {
            const request = args.payload as { blocks: typeof document.blocks };
            document = { ...document, current_revision: 3, content_sha256: "b".repeat(64), blocks: request.blocks };
            article = { ...article, content: request.blocks.map((block) => block.text).join("\n\n"), material_revision: document.current_revision, content_sha256: document.content_sha256 };
            return { document, impact };
          }
          if (command === "list_material_revisions_cmd") return [
            { revision: 3, parent_revision: 2, action: "edit", content_sha256: "b".repeat(64), change_summary: { inserted_blocks: 1 }, created_at: now },
            { revision: 2, parent_revision: 1, action: "edit", content_sha256: "a".repeat(64), change_summary: { updated_blocks: 1 }, created_at: "2026-07-15T11:00:00Z" },
          ];
          if (command === "get_material_revision_cmd") return {
            revision: args.revision,
            parent_revision: "revision-1",
            action: "edit",
            content_sha256: "a".repeat(64),
            change_summary: { updated_blocks: 1 },
            created_at: "2026-07-15T11:00:00Z",
            snapshot: { blocks: [{ id: "block-body", block_type: "paragraph", block_order: 0, text: "Earlier HIF text.", attrs: {} }] },
          };
          if (command === "restore_material_revision_cmd") {
            document = { ...document, current_revision: 4, blocks: [{ id: "block-body", block_type: "paragraph", block_order: 0, text: "Earlier HIF text.", attrs: {}, segments: [] }] };
            return { document, impact: { ...impact, inserted_blocks: 0, stale_translations: 0, affected_annotations: 0 } };
          }
          return null;
        },
      },
    });
  });

  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("http://127.0.0.1:1420/");
  await page.getByRole("complementary", { name: "主导航" }).getByRole("button", { name: /素材|Material Library/ }).click();
  await page.getByRole("button", { name: /HIF review/ }).first().click();
  await page.getByRole("button", { name: /编辑|Edit/ }).click();

  await expect(page.getByRole("region", { name: "正文编辑器" })).toBeVisible();
  await expect(page.getByRole("complementary", { name: "主导航" })).toHaveCount(0);
  await page.getByRole("button", { name: /添加段落/ }).click();
  await page.getByRole("button", { name: /检查影响/ }).click();
  await expect(page.getByRole("dialog", { name: "编辑影响" })).toContainText("翻译待更新");
  await page.getByRole("button", { name: "完成" }).click();

  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "确认正文变更" })).toContainText("批注受影响");
  await page.getByRole("button", { name: "确认保存" }).click();
  await expect(page.getByText("没有未保存修改")).toBeVisible();

  await page.getByRole("button", { name: /版本历史/ }).click();
  await page.getByRole("button", { name: /修改 1/ }).click();
  await expect(page.getByText("Earlier HIF text.")).toBeVisible();
  await page.getByRole("button", { name: "恢复此版本" }).click();
  await page.getByRole("button", { name: "确认恢复" }).click();
  await expect(page.getByTestId("material-block-editor")).toContainText("Earlier HIF text.");

  await page.setViewportSize({ width: 700, height: 800 });
  await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.getByRole("button", { name: "返回阅读" }).click();
  await expect(page.getByTestId("article-reader-scroll")).toBeVisible();

  const calls = await page.evaluate(() => window.__openkotoEditorCalls);
  expect(calls).toEqual(expect.arrayContaining([
    expect.objectContaining({ command: "get_material_document_cmd" }),
    expect.objectContaining({ command: "preview_material_edit_cmd" }),
    expect.objectContaining({ command: "commit_material_edit_cmd" }),
    expect.objectContaining({ command: "list_material_revisions_cmd" }),
    expect.objectContaining({ command: "restore_material_revision_cmd" }),
  ]));
});
