import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { LearningCandidateBox } from "./LearningCandidateBox";
import type { Article, LearningItem } from "../../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function createArticle(): Article {
  return {
    id: "article-1",
    title: "Academic Reading",
    content: "This can mitigate risk.",
    created_at: "2026-03-08T00:00:00Z",
    translated: false,
    segments: [],
  };
}

function createLearningItem(overrides: Partial<LearningItem> = {}): LearningItem {
  return {
    id: "item-1",
    material_id: "article-1",
    segment_id: "seg-1",
    item_type: "word",
    text: "mitigate",
    source_sentence: "This can mitigate risk.",
    context_before: "This can",
    context_after: "risk.",
    meaning_in_context: null,
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
    ...overrides,
  };
}

describe("LearningCandidateBox", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it("creates a candidate from selection and accepts it into a word pack", async () => {
    let currentItem = createLearningItem();
    invokeMock.mockImplementation((command: string, payload?: Record<string, unknown>) => {
      if (command === "list_learning_items_cmd") {
        return Promise.resolve([]);
      }
      if (command === "create_learning_item_from_selection_cmd") {
        return Promise.resolve(currentItem);
      }
      if (command === "segment_translate_explain_cmd") {
        return Promise.resolve({
          translation: "这可以降低风险。",
          explanation: "Mitigate means reduce severity or risk.",
          vocabulary: [
            {
              word: "mitigate",
              meaning: "减轻；降低",
              usage: "",
              example: "Early action can mitigate risk.",
            },
          ],
          grammar_points: [],
        });
      }
      if (command === "update_learning_item_cmd") {
        const updatePayload = payload?.payload as Partial<LearningItem>;
        currentItem = { ...currentItem, ...updatePayload, updated_at: "2026-03-08T00:00:01Z" };
        return Promise.resolve(currentItem);
      }
      if (command === "list_word_packs_cmd") {
        return Promise.resolve([{ id: "pack-1", name: "未分组", is_system: true }]);
      }
      if (command === "add_favorite_vocabulary_cmd") {
        return Promise.resolve({ id: "fav-1" });
      }
      return Promise.resolve(null);
    });

    render(
      <LearningCandidateBox
        article={createArticle()}
        selection={{
          materialId: "article-1",
          segmentId: "seg-1",
          selectedText: "mitigate",
          sourceSentence: "This can mitigate risk.",
          contextBefore: "This can",
          contextAfter: "risk.",
        }}
        open
        canUseAi
        targetLanguage="zh-CN"
        onOpenChange={() => {}}
        onError={() => {}}
        onSuccess={() => {}}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "加入候选" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "create_learning_item_from_selection_cmd",
        expect.objectContaining({
          payload: expect.objectContaining({
            material_id: "article-1",
            segment_id: "seg-1",
            selected_text: "mitigate",
            source_sentence: "This can mitigate risk.",
          }),
        })
      );
    });

    await userEvent.click(screen.getByRole("button", { name: "AI 补全" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "update_learning_item_cmd",
        expect.objectContaining({
          id: "item-1",
          payload: expect.objectContaining({
            meaning_in_context: "减轻；降低",
          }),
        })
      );
    });

    await userEvent.click(screen.getByRole("button", { name: "接受" }));
    await screen.findByText("未分组");
    await userEvent.click(screen.getByRole("button", { name: "确认" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "add_favorite_vocabulary_cmd",
        expect.objectContaining({
          word: "mitigate",
          sourceArticleId: "article-1",
          sourceArticleTitle: "Academic Reading",
          packIds: ["pack-1"],
        })
      );
      expect(invokeMock).toHaveBeenCalledWith(
        "update_learning_item_cmd",
        expect.objectContaining({
          id: "item-1",
          payload: { status: "accepted" },
        })
      );
    });
  });
});
