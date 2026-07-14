import { expect, test } from "@playwright/test";

declare global {
  interface Window {
    __openkotoInvokeCalls: Array<{ command: string; args: Record<string, unknown> }>;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
    __TAURI_INTERNALS__: unknown;
  }
}

test.describe("Material workbench", () => {
  test("search, continue reading, tags, bulk archive, jobs, and progress bridge", async ({ page }) => {
    await page.addInitScript(() => {
      const now = "2026-07-11T00:00:00Z";
      const progress = {
        material_id: "material-reading",
        reader_kind: "article",
        locator: { kind: "segment", segment_order: 0, total_segments: 2, segment_id: "segment-1" },
        progress_ratio: 0.5,
        status: "reading",
        last_opened_at: now,
        completed_at: null,
        updated_at: now,
      };
      const articles = [
        {
          id: "material-reading",
          title: "Genomics Reading",
          content: "Genomic evidence changes clinical interpretation. A second sentence keeps the reader active.",
          source_type: "article",
          source_url: "https://example.com/genomics",
          created_at: now,
          translated: false,
          metadata: {},
          tags: [{ id: "tag-research", name: "Research", color: "#2255aa", created_at: now, updated_at: now }],
          reading_progress: progress,
          archived_at: null,
          segments: [
            { id: "segment-1", article_id: "material-reading", order: 0, text: "Genomic evidence changes clinical interpretation.", created_at: now, is_new_paragraph: true },
            { id: "segment-2", article_id: "material-reading", order: 1, text: "A second sentence keeps the reader active.", created_at: now, is_new_paragraph: false },
          ],
        },
        {
          id: "material-unread",
          title: "Daily English Notes",
          content: "A short note for daily language practice.",
          source_type: "text_file",
          source_url: "file:///tmp/daily.md",
          created_at: "2026-07-10T00:00:00Z",
          translated: false,
          metadata: {},
          tags: [],
          reading_progress: null,
          archived_at: null,
          segments: [{ id: "segment-3", article_id: "material-unread", order: 0, text: "A short note for daily language practice.", created_at: now, is_new_paragraph: true }],
        },
      ];
      const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
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
            if (command === "packaged_backend_status_cmd") {
              return { enabled: true, running: true, message: "mock packaged backend is healthy" };
            }
            if (command === "get_config") {
              return {
                onboarding_completed: true,
                interface_language: "zh-CN",
                target_language: "zh-CN",
                ui_font_family: "Inter, sans-serif",
                reader_font_family: "Georgia, serif",
                active_model_id: "model-1",
                model_configs: [{ id: "model-1", name: "Mock", api_provider: "openai", api_key: "test", model: "gpt-4o-mini", is_default: true }],
              };
            }
            if (command === "backend_check_session_cmd") {
              return {
                configured: true,
                connected: true,
                authenticated: true,
                backend_url: "http://127.0.0.1:19421",
                user: { id: "user-1", email: "reader@example.com", display_name: "Reader", created_at: now, updated_at: now },
                error: null,
              };
            }
            if (command === "list_articles_cmd") return articles;
            if (command === "get_learning_activity_heatmap_cmd") {
              return {
                start_date: "2026-04-19",
                end_date: "2026-07-11",
                days: Array.from({ length: 84 }, (_, index) => ({
                  date: new Date(Date.UTC(2026, 3, 19 + index)).toISOString().slice(0, 10),
                  read_materials: index === 83 ? 1 : 0,
                  learning_actions: 0,
                  activity_score: index === 83 ? 1 : 0,
                })),
              };
            }
            if (command === "get_article") return articles.find((article) => article.id === args.id) || null;
            if (command === "material_library_list_tags_cmd") {
              return [{ id: "tag-research", name: "Research", color: "#2255aa", material_count: 1, created_at: now, updated_at: now }];
            }
            if (command === "material_library_list_import_jobs_cmd") {
              return [{
                id: "job-1",
                source_kind: "text_file",
                status: "succeeded",
                progress: 1,
                created_at: now,
                updated_at: now,
                preview: { title: "Daily English Notes", duplicates: { duplicate: false, matches: [] } },
                error_code: null,
                error_message: null,
              }];
            }
            if (command === "material_library_get_reading_progress_cmd") return progress;
            if (command === "material_library_upsert_reading_progress_cmd") return progress;
            if (command === "material_library_bulk_archive_cmd") return { affected: 1 };
            if (command === "material_library_bulk_tags_cmd") return { affected: 1 };
            if (command === "get_resource_server_info_cmd") return { base_url: "http://127.0.0.1:19420", token: "test-token" };
            if (command === "list_learning_items_cmd") return [];
            return null;
          },
        },
      });
    });

    await page.emulateMedia({ colorScheme: "dark" });
    await page.goto("http://127.0.0.1:1420/");
    await expect(page.getByText("继续阅读").first()).toBeVisible();
    await expect(page.getByText("Genomics Reading").first()).toBeVisible();
    await expect.poll(() => page.evaluate(() => document.documentElement.classList.contains("dark"))).toBe(true);
    await expect.poll(() => page.evaluate(() => document.documentElement.style.getPropertyValue("--font-sans"))).toBe("Inter, sans-serif");
    await expect.poll(() => page.evaluate(() => document.documentElement.style.getPropertyValue("--openkoto-reader-font-family"))).toBe("Georgia, serif");
    await page.emulateMedia({ colorScheme: "light" });
    await expect.poll(() => page.evaluate(() => document.documentElement.classList.contains("dark"))).toBe(false);
    await page.setViewportSize({ width: 1440, height: 900 });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.setViewportSize({ width: 700, height: 800 });
    await expect.poll(async () => Math.round((await page.getByRole("complementary", { name: "主导航" }).boundingBox())?.width ?? 0)).toBe(72);
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.setViewportSize({ width: 1280, height: 720 });
    await expect.poll(() => page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.getByRole("button", { name: "素材库" }).first().click();
    await page.getByRole("tab", { name: /导入任务/ }).click();
    await expect(page.getByRole("region", { name: "最近导入任务" })).toContainText("已完成");

    await page.getByRole("tab", { name: /^素材/ }).click();

    const allMaterials = page.locator('section[aria-labelledby="all-materials-title"]');
    await page.getByRole("textbox", { name: "搜索素材" }).fill("Daily");
    await expect(allMaterials.getByText("Daily English Notes")).toBeVisible();
    await expect(allMaterials.getByText("Genomics Reading")).toHaveCount(0);
    await page.getByRole("button", { name: "清除筛选" }).click();

    await page.getByRole("checkbox", { name: "选择 Genomics Reading" }).check();
    await page.getByRole("tab", { name: /标签/ }).click();
    await page.getByRole("checkbox", { name: "批量选择 Research" }).check();
    await page.getByRole("button", { name: "应用到 1 项" }).click();
    await page.getByRole("button", { name: "确认应用" }).click();

    await page.getByRole("tab", { name: /^素材/ }).click();
    await page.getByRole("checkbox", { name: "选择 Genomics Reading" }).check();
    await page.getByRole("button", { name: "归档", exact: true }).click();
    await page.getByRole("button", { name: "归档", exact: true }).last().click();

    await page.getByRole("textbox", { name: "搜索素材" }).fill("Genomics");
    await page.getByRole("button", { name: /Genomics Reading/ }).first().click();
    await expect(page.getByText("Genomic evidence changes clinical interpretation.")).toBeVisible();
    await expect(page.getByRole("complementary", { name: "主导航" })).toHaveCount(0);
    await page.getByText("A second sentence keeps the reader active.").click();
    await expect.poll(async () => (await page.evaluate(() => window.__openkotoInvokeCalls))
      .filter((call) => call.command === "material_library_upsert_reading_progress_cmd").length).toBeGreaterThan(0);
    await page.getByRole("button", { name: "返回" }).click();
    await expect(page.getByRole("textbox", { name: "搜索素材" })).toHaveValue("Genomics");

    const calls = await page.evaluate(() => window.__openkotoInvokeCalls);
    expect(calls).toEqual(expect.arrayContaining([
      expect.objectContaining({ command: "material_library_bulk_tags_cmd", args: { request: { ids: ["material-reading"], tag_ids: ["tag-research"], mode: "add" } } }),
      expect.objectContaining({ command: "material_library_bulk_archive_cmd", args: { request: { ids: ["material-reading"] } } }),
      expect.objectContaining({ command: "material_library_get_reading_progress_cmd", args: { materialId: "material-reading" } }),
      expect.objectContaining({ command: "material_library_upsert_reading_progress_cmd" }),
    ]));
  });
});
