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
    const moveImpact = {
      inserted_blocks: 0,
      updated_blocks: 0,
      deleted_blocks: 0,
      moved_blocks: 1,
      changed_segments: 0,
      deleted_segments: 0,
      stale_readings: 0,
      stale_translations: 0,
      stale_explanations: 0,
      affected_annotations: 0,
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
    let pendingImpact = impact;

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
          if (command === "preview_material_edit_cmd") {
            const request = args.payload as { blocks: typeof document.blocks };
            const requestIds = request.blocks.map((block) => block.id);
            const documentIds = document.blocks.map((block) => block.id);
            const isMoveOnly = requestIds.length === documentIds.length
              && requestIds.some((id, index) => id !== documentIds[index]);
            pendingImpact = isMoveOnly ? moveImpact : impact;
            return {
              base_revision: document.current_revision,
              next_revision: document.current_revision + 1,
              content_sha256: "b".repeat(64),
              preview_token: "preview-e2e",
              impact: pendingImpact,
            };
          }
          if (command === "commit_material_edit_cmd") {
            const request = args.payload as { blocks: typeof document.blocks };
            document = { ...document, current_revision: document.current_revision + 1, content_sha256: "b".repeat(64), blocks: request.blocks };
            article = { ...article, content: request.blocks.map((block) => block.text).join("\n\n"), material_revision: document.current_revision, content_sha256: document.content_sha256 };
            return { document, impact: pendingImpact };
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

  const editorBlocks = page.getByTestId("material-block-editor").locator(".tiptap > *");
  await expect(editorBlocks).toHaveCount(2);
  await expect(editorBlocks.nth(0)).toContainText("HIF review");
  await expect(editorBlocks.nth(1)).toContainText("HIF signalling changes transcription.");

  const firstBlockBox = await editorBlocks.nth(0).boundingBox();
  expect(firstBlockBox).not.toBeNull();
  await page.mouse.move(
    firstBlockBox!.x + Math.min(80, firstBlockBox!.width / 2),
    firstBlockBox!.y + firstBlockBox!.height / 2,
  );
  const dragHandle = page.getByTestId("material-block-drag-handle");
  await expect(dragHandle).toBeVisible();
  const handleBox = await dragHandle.boundingBox();
  const secondBlockBox = await editorBlocks.nth(1).boundingBox();
  expect(handleBox).not.toBeNull();
  expect(secondBlockBox).not.toBeNull();
  const handleGapX = (firstBlockBox!.x + handleBox!.x + handleBox!.width) / 2;
  await page.mouse.move(
    handleGapX,
    handleBox!.y + handleBox!.height / 2,
    { steps: 10 },
  );
  await page.waitForTimeout(50);
  await expect(dragHandle).toBeVisible();
  await page.mouse.move(
    handleBox!.x + handleBox!.width / 2,
    handleBox!.y + handleBox!.height / 2,
    { steps: 4 },
  );
  await expect(dragHandle).toBeVisible();
  const handleCenterX = handleBox!.x + handleBox!.width / 2;
  const handleCenterY = handleBox!.y + handleBox!.height / 2;
  expect(handleCenterX).toBeLessThan(firstBlockBox!.x);
  await page.mouse.down();
  await page.mouse.move(handleCenterX, handleCenterY + 8, { steps: 3 });
  const dragPreview = page.getByTestId("material-block-drag-preview");
  await expect(dragPreview).toBeVisible();
  await expect(dragPreview).toContainText("HIF review");
  await expect(editorBlocks.nth(0)).toHaveClass(/openkoto-editor-drag-source/);
  await page.mouse.move(
    handleCenterX,
    secondBlockBox!.y + secondBlockBox!.height - 2,
    { steps: 12 },
  );
  await expect(page.getByTestId("material-block-drop-indicator")).toBeVisible();
  await expect(dragPreview).toBeVisible();
  await expect(dragPreview).toContainText("HIF review");
  await page.mouse.up();

  await expect(page.getByText("松开以导入", { exact: true })).toHaveCount(0);
  await expect(dragPreview).toHaveCount(0);
  await expect(page.getByTestId("material-block-drop-indicator")).toHaveCount(0);
  await expect(page.getByTestId("material-block-editor").locator(".ProseMirror-selectednode")).toHaveCount(0);
  await expect(page.getByTestId("material-block-editor").locator(".openkoto-editor-drag-source")).toHaveCount(0);
  await expect.poll(() => editorBlocks.evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.blockId)))
    .toEqual(["block-body", "block-heading"]);
  await expect(editorBlocks.nth(0)).toContainText("HIF signalling changes transcription.");
  await expect(editorBlocks.nth(1)).toContainText("HIF review");

  await page.getByRole("button", { name: "撤销" }).click();
  await expect.poll(() => editorBlocks.evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.blockId)))
    .toEqual(["block-heading", "block-body"]);
  await expect(editorBlocks.nth(0)).toContainText("HIF review");
  await expect(editorBlocks.nth(1)).toContainText("HIF signalling changes transcription.");
  await page.getByRole("button", { name: "重做" }).click();
  await expect.poll(() => editorBlocks.evaluateAll((nodes) => nodes.map((node) => (node as HTMLElement).dataset.blockId)))
    .toEqual(["block-body", "block-heading"]);
  await expect(editorBlocks.nth(0)).toContainText("HIF signalling changes transcription.");
  await expect(editorBlocks.nth(1)).toContainText("HIF review");

  await page.getByRole("button", { name: /检查影响/ }).click();
  const moveImpactDialog = page.getByRole("dialog", { name: "编辑影响" });
  await expect(moveImpactDialog).toContainText("新增块0");
  await expect(moveImpactDialog).toContainText("修改块0");
  await expect(moveImpactDialog).toContainText("删除块0");
  await expect(moveImpactDialog).toContainText("移动块1");
  await page.getByRole("button", { name: "完成" }).click();
  await page.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByText("没有未保存修改")).toBeVisible();

  const moveCalls = await page.evaluate(() => window.__openkotoEditorCalls
    .filter((call) => call.command === "preview_material_edit_cmd" || call.command === "commit_material_edit_cmd"));
  expect(moveCalls.map((call) => call.command)).toEqual([
    "preview_material_edit_cmd",
    "preview_material_edit_cmd",
    "commit_material_edit_cmd",
  ]);
  for (const call of moveCalls) {
    const payload = call.args.payload as { blocks: Array<{ id: string; block_order: number }> };
    expect(payload.blocks.map((block) => block.id)).toEqual(["block-body", "block-heading"]);
    expect(payload.blocks.map((block) => block.block_order)).toEqual([0, 1]);
  }

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
