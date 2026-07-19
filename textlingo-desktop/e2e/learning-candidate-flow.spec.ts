import { expect, test } from "@playwright/test";

declare global {
  interface Window {
    __openkotoInvokeCalls: Array<{ command: string; args: unknown }>;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
    __TAURI_INTERNALS__: unknown;
  }
}

test.describe("Learning candidate flow", () => {
  test("open article -> select text -> create candidate -> accept into pack", async ({ page }) => {
    await page.addInitScript(() => {
      const article = {
        id: "article-1",
        title: "Academic Reading",
        content: "Macrophages can mitigate inflammatory damage.",
        source_type: "article",
        created_at: "2026-03-08T00:00:00Z",
        translated: false,
        segments: [
          {
            id: "seg-1",
            article_id: "article-1",
            order: 0,
            text: "Macrophages can mitigate inflammatory damage.",
            created_at: "2026-03-08T00:00:00Z",
            is_new_paragraph: true,
          },
        ],
      };
      let learningItem = {
        id: "item-1",
        material_id: "article-1",
        segment_id: "seg-1",
        item_type: "word",
        text: "mitigate",
        source_sentence: "Macrophages can mitigate inflammatory damage.",
        context_before: "Macrophages can",
        context_after: "inflammatory damage.",
        meaning_in_context: "",
        definition_en: null,
        definition_zh: null,
        collocations: [],
        examples: [],
        tags: ["reader"],
        status: "candidate",
        priority: 0,
        difficulty: null,
        ai_explanation: null,
        review_state: {},
        source_material_title: "Academic Reading",
        source_segment_order: 0,
        accepted_at: null,
        rejected_at: null,
        created_at: "2026-03-08T00:00:00Z",
        updated_at: "2026-03-08T00:00:00Z",
      };
      const calls: Array<{ command: string; args: unknown }> = [];
      const callbacks: Record<number, (...args: unknown[]) => unknown> = {};
      let nextCallbackId = 1;

      Object.assign(window, {
        __openkotoInvokeCalls: calls,
        __TAURI_EVENT_PLUGIN_INTERNALS__: {
          unregisterListener: () => {},
        },
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
          unregisterCallback: (id: number) => {
            delete callbacks[id];
          },
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
                model_configs: [
                  {
                    id: "model-1",
                    name: "Mock Model",
                    api_provider: "openai",
                    api_key: "test",
                    model: "gpt-4o-mini",
                    is_default: true,
                  },
                ],
              };
            }
            if (command === "backend_check_session_cmd") {
              return {
                configured: true,
                connected: true,
                authenticated: true,
                backend_url: "http://127.0.0.1:4000",
                user: {
                  id: "user-1",
                  email: "reader@example.com",
                  display_name: "Reader",
                  created_at: "2026-03-08T00:00:00Z",
                  updated_at: "2026-03-08T00:00:00Z",
                },
                error: null,
              };
            }
            if (command === "list_articles_cmd") return [article];
            if (command === "get_article") return article;
            if (command === "get_resource_server_info_cmd") {
              return { base_url: "http://127.0.0.1:19420", token: "test-token" };
            }
            if (command === "list_learning_items_cmd") return [];
            if (command === "create_learning_item_from_selection_cmd") {
              return learningItem;
            }
            if (command === "list_word_packs_cmd") {
              return [{ id: "pack-1", name: "未分组", is_system: true }];
            }
            if (command === "accept_learning_item_cmd") {
              learningItem = {
                ...learningItem,
                status: "accepted",
                accepted_at: "2026-03-08T00:00:01Z",
                updated_at: "2026-03-08T00:00:01Z",
              };
              return {
                learning_item: learningItem,
                favorite: { type: "vocabulary", id: "favorite-1", pack_ids: ["pack-1"] },
              };
            }
            return null;
          },
        },
      });
    });

    await page.goto("http://127.0.0.1:1420/");
    await expect(page.getByRole("heading", { name: "Academic Reading" })).toBeVisible();
    await page.getByRole("button", { name: "开始阅读" }).click();

    const segment = page.getByText("Macrophages can mitigate inflammatory damage.");
    await expect(segment).toBeVisible();
    await segment.evaluate((element) => {
      const textNode = element.firstChild;
      if (!textNode) throw new Error("segment text node missing");
      const text = textNode.textContent || "";
      const start = text.indexOf("mitigate");
      const range = document.createRange();
      range.setStart(textNode, start);
      range.setEnd(textNode, start + "mitigate".length);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      element.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });

    const candidateBox = page.getByTestId("learning-candidate-box");
    await expect(candidateBox).toBeVisible();
    await expect(candidateBox.getByText("mitigate")).toBeVisible();
    await candidateBox.getByRole("button", { name: "加入候选" }).click();
    await candidateBox.getByRole("button", { name: "接受", exact: true }).click();
    await page.getByRole("button", { name: "确认" }).click();

    const calls = await page.evaluate(() => window.__openkotoInvokeCalls);
    expect(calls).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          command: "create_learning_item_from_selection_cmd",
          args: expect.objectContaining({
            payload: expect.objectContaining({
              material_id: "article-1",
              segment_id: "seg-1",
              selected_text: "mitigate",
              source_sentence: "Macrophages can mitigate inflammatory damage.",
            }),
          }),
        }),
        expect.objectContaining({
          command: "accept_learning_item_cmd",
          args: expect.objectContaining({
            id: "item-1",
            payload: {
              favorite_type: "vocabulary",
              pack_ids: ["pack-1"],
            },
          }),
        }),
      ]),
    );
  });
});
