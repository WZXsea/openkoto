import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ArticleReader } from "./ArticleReader";
import type { Article } from "../../types";

const invokeMock = vi.fn();
const openMock = vi.fn();
const localStorageStore = new Map<string, string>();
const configMock = vi.hoisted(() => ({
  current: {
    target_language: "zh-CN",
    active_model_id: "model-1",
    model_configs: [{ id: "model-1", name: "Model", api_provider: "openai", api_key: "key", model: "gpt-4o-mini", is_default: true }],
  } as any,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(),
  open: (...args: unknown[]) => openMock(...args),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, fallbackOrOptions?: string | Record<string, unknown>) =>
      typeof fallbackOrOptions === "string" ? fallbackOrOptions : key,
  }),
}));

vi.mock("docx", () => ({
  Document: class {},
  HeadingLevel: {},
  Packer: { toBlob: vi.fn() },
  Paragraph: class {},
  TextRun: class {},
}));

vi.mock("../../lib/hooks", () => ({
  useConfig: () => ({
    config: configMock.current,
  }),
}));

vi.mock("./ArticleChatAssistant", () => ({
  ArticleChatAssistant: () => <div data-testid="article-chat-assistant" />,
}));

vi.mock("./ArticleExplanationPanel", () => ({
  ArticleExplanationPanel: () => <div data-testid="article-explanation-panel" />,
}));

vi.mock("./ArticleMindMapPanel", () => ({
  ArticleMindMapPanel: () => <div data-testid="article-mind-map-panel" />,
}));

vi.mock("./VideoSubtitlePlayer", () => ({
  VideoSubtitlePlayer: ({
    onImportSubtitles,
    onViewModeChange,
  }: {
    onImportSubtitles?: () => void;
    onViewModeChange?: (mode: "original" | "bilingual" | "translation") => void;
  }) => (
    <div data-testid="video-subtitle-player">
      {onViewModeChange ? (
        <button
          type="button"
          data-testid="player-view-mode-trigger"
          onClick={() => onViewModeChange("bilingual")}
        >
          player view mode
        </button>
      ) : null}
      {onImportSubtitles ? (
        <button type="button" onClick={onImportSubtitles}>
          Import subtitles
        </button>
      ) : null}
    </div>
  ),
}));

function createArticle(overrides: Partial<Article> = {}): Article {
  return {
    id: "article-1",
    title: "Sample Article",
    content: "Alpha beta gamma.",
    created_at: "2026-03-08T00:00:00Z",
    translated: false,
    segments: [
      {
        id: "seg-1",
        article_id: "article-1",
        order: 0,
        text: "Alpha beta gamma.",
        created_at: "2026-03-08T00:00:00Z",
      },
    ],
    ...overrides,
  };
}

describe("ArticleReader agent mode", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_resource_server_info_cmd") {
        return Promise.resolve({
          base_url: "http://127.0.0.1:19420",
          token: "test-token",
        });
      }
      if (command === "preview_material_import_cmd") {
        return Promise.resolve({
          title: "Sample Article",
          source_uri: "file:///tmp/sample.srt",
          paragraph_count: 1,
          content_snippet: "Imported subtitle",
          file: { file_name: "sample.srt", sha256: "a".repeat(64) },
          duplicates: { duplicate: false, matches: [] },
          job: { id: "job-1" },
        });
      }
      if (command === "list_learning_items_cmd") {
        return Promise.resolve([]);
      }
      return Promise.resolve(undefined);
    });
    openMock.mockReset();
    configMock.current = {
      target_language: "zh-CN",
      active_model_id: "model-1",
      model_configs: [{ id: "model-1", name: "Model", api_provider: "openai", api_key: "key", model: "gpt-4o-mini", is_default: true }],
    };
    localStorageStore.clear();
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: (key: string) => localStorageStore.get(key) ?? null,
        setItem: (key: string, value: string) => {
          localStorageStore.set(key, value);
        },
        removeItem: (key: string) => {
          localStorageStore.delete(key);
        },
      },
      configurable: true,
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("renders agent in the existing tab row without a separate top mode switch", async () => {
    render(<ArticleReader article={createArticle()} />);

    expect(screen.getByRole("button", { name: "讲解" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "对话" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Agent" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "快问" })).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Agent" }));

    expect(screen.getByText("当前支持")).toBeInTheDocument();
    expect(screen.getByText("查看当前素材")).toBeInTheDocument();
  });

  it("lets media articles import subtitles from a local srt file", async () => {
    const onUpdate = vi.fn();
    openMock.mockResolvedValue("/tmp/sample.srt");
    const importedArticle = {
      ...createArticle({
        media_path: "/tmp/sample.mp4",
        segments: [],
      }),
      segments: [
        {
          id: "seg-2",
          article_id: "article-1",
          order: 0,
          text: "Imported subtitle",
          created_at: "2026-03-08T00:00:00Z",
          start_time: 0,
          end_time: 1,
        },
      ],
      content: "Imported subtitle",
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_resource_server_info_cmd") {
        return Promise.resolve({
          base_url: "http://127.0.0.1:19420",
          token: "test-token",
        });
      }
      if (command === "preview_material_import_cmd") {
        return Promise.resolve({
          title: "Sample Article",
          source_uri: "file:///tmp/sample.srt",
          paragraph_count: 1,
          content_snippet: "Imported subtitle",
          file: { file_name: "sample.srt", sha256: "a".repeat(64) },
          duplicates: { duplicate: false, matches: [] },
          job: { id: "job-1" },
        });
      }
      if (command === "import_article_subtitles_cmd" || command === "get_article") {
        return Promise.resolve(importedArticle);
      }
      return Promise.resolve(undefined);
    });

    render(
      <ArticleReader
        article={createArticle({
          media_path: "/tmp/sample.mp4",
          segments: [],
        })}
        onUpdate={onUpdate}
      />
    );

    await userEvent.click(screen.getByRole("button", { name: "Import subtitles" }));
    await userEvent.click(await screen.findByRole("button", { name: "确认并导入" }));

    expect(openMock).toHaveBeenCalled();
    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("import_article_subtitles_cmd", {
        articleId: "article-1",
        subtitlePath: "/tmp/sample.srt",
        importJobId: "job-1",
        duplicatePolicy: "keep_copy",
      });
      expect(onUpdate).toHaveBeenCalledTimes(1);
    });
  });

  it("creates a learning candidate from scoped segment text selection", async () => {
    invokeMock.mockImplementation((command: string, payload?: Record<string, unknown>) => {
      if (command === "get_resource_server_info_cmd") {
        return Promise.resolve({
          base_url: "http://127.0.0.1:19420",
          token: "test-token",
        });
      }
      if (command === "list_learning_items_cmd") {
        return Promise.resolve([]);
      }
      if (command === "create_learning_item_from_selection_cmd") {
        return Promise.resolve({
          id: "learning-item-1",
          material_id: "article-1",
          segment_id: "seg-1",
          item_type: "word",
          text: "beta",
          source_sentence: "Alpha beta gamma.",
          collocations: [],
          examples: [],
          tags: ["reader"],
          status: "candidate",
          priority: 0,
          review_state: {},
          created_at: "2026-03-08T00:00:00Z",
          updated_at: "2026-03-08T00:00:00Z",
        });
      }
      return Promise.resolve(payload);
    });

    render(<ArticleReader article={createArticle()} />);

    const segment = await screen.findByText("Alpha beta gamma.");
    const textNode = segment.firstChild;
    expect(textNode).toBeTruthy();

    const range = document.createRange();
    range.setStart(textNode as ChildNode, 6);
    range.setEnd(textNode as ChildNode, 10);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);

    fireEvent.mouseUp(segment);

    expect(await screen.findByTestId("learning-candidate-box")).toBeInTheDocument();
    expect(screen.getByText("beta")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "加入候选" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "create_learning_item_from_selection_cmd",
        expect.objectContaining({
          payload: expect.objectContaining({
            material_id: "article-1",
            segment_id: "seg-1",
            selected_text: "beta",
            source_sentence: "Alpha beta gamma.",
          }),
        })
      );
    });
  });

  it("moves the view mode control into the player area for media articles", () => {
    render(
      <ArticleReader
        article={createArticle({
          media_path: "/tmp/sample.mp4",
        })}
      />
    );

    expect(screen.getByTestId("player-view-mode-trigger")).toBeInTheDocument();
    expect(screen.queryByTestId("reader-toolbar-view-mode-trigger")).not.toBeInTheDocument();
  });

  it("does not auto invoke AI explanation when no model is configured", async () => {
    configMock.current = {
      target_language: "zh-CN",
      active_model_id: undefined,
      model_configs: [],
    };
    invokeMock.mockResolvedValue(undefined);

    render(<ArticleReader article={createArticle()} />);

    await userEvent.click(screen.getByText("Alpha beta gamma."));

    expect(invokeMock).not.toHaveBeenCalledWith(
      "segment_translate_explain_cmd",
      expect.anything(),
    );
  });

  it("keeps the top toolbar view mode control for non-media articles", () => {
    render(<ArticleReader article={createArticle()} />);

    expect(screen.getByTestId("reader-toolbar-view-mode-trigger")).toBeInTheDocument();
    expect(screen.queryByTestId("player-view-mode-trigger")).not.toBeInTheDocument();
  });

  it("reports the visible article segment while scrolling", async () => {
    const onProgressChange = vi.fn();
    const segments = [
      { id: "seg-1", article_id: "article-1", order: 0, text: "First segment.", created_at: "2026-03-08T00:00:00Z", is_new_paragraph: true },
      { id: "seg-2", article_id: "article-1", order: 1, text: "Second segment.", created_at: "2026-03-08T00:00:00Z", is_new_paragraph: true },
      { id: "seg-3", article_id: "article-1", order: 2, text: "Third segment.", created_at: "2026-03-08T00:00:00Z", is_new_paragraph: true },
    ];
    render(<ArticleReader article={createArticle({ segments })} onProgressChange={onProgressChange} />);
    const scrollContainer = screen.getByTestId("article-reader-scroll");
    vi.spyOn(scrollContainer, "getBoundingClientRect").mockReturnValue({
      top: 0, bottom: 400, left: 0, right: 800, width: 800, height: 400, x: 0, y: 0, toJSON: () => ({}),
    });
    const segmentElements = Array.from(scrollContainer.querySelectorAll<HTMLElement>("[data-reader-segment-id]"));
    [100, 300, 500].forEach((bottom, index) => {
      vi.spyOn(segmentElements[index], "getBoundingClientRect").mockReturnValue({
        top: bottom - 80, bottom, left: 0, right: 700, width: 700, height: 80, x: 0, y: bottom - 80, toJSON: () => ({}),
      });
    });

    fireEvent.scroll(scrollContainer);

    await waitFor(() => {
      expect(onProgressChange).toHaveBeenCalledWith(expect.objectContaining({
        reader_kind: "article",
        locator: expect.objectContaining({ kind: "segment", segment_id: "seg-2", segment_order: 1 }),
        progress_ratio: 2 / 3,
      }));
    }, { timeout: 2_000 });
  });

  it("uses the configured batch explanation concurrency", async () => {
    const segments = Array.from({ length: 6 }, (_, index) => ({
      id: `seg-${index + 1}`,
      article_id: "article-1",
      order: index,
      text: `Segment ${index + 1}`,
      created_at: "2026-03-08T00:00:00Z",
      is_new_paragraph: true,
    }));
    let activeRequests = 0;
    let maxActiveRequests = 0;
    const pendingResolvers: Array<() => void> = [];

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve({
          target_language: "zh-CN",
          batch_translation_concurrency: 5,
        });
      }

      if (command === "segment_translate_explain_cmd") {
        activeRequests += 1;
        maxActiveRequests = Math.max(maxActiveRequests, activeRequests);
        return new Promise((resolve) => {
          pendingResolvers.push(() => {
            activeRequests -= 1;
            resolve({
              translation: "Translated",
              explanation: "Explained",
              reading_text: "Reading",
            });
          });
        });
      }

      if (command === "update_article_segment") {
        return Promise.resolve(undefined);
      }

      if (command === "load_article") {
        return Promise.resolve(JSON.stringify(createArticle({ segments })));
      }

      return Promise.resolve(undefined);
    });

    render(<ArticleReader article={createArticle({ segments })} />);

    await userEvent.click(screen.getByRole("button", { name: "articleReader.analyzeAll" }));
    await userEvent.click(screen.getByRole("button", { name: "articleReader.analyze" }));

    await waitFor(() => {
      expect(pendingResolvers).toHaveLength(5);
    });
    expect(maxActiveRequests).toBe(5);

    pendingResolvers.splice(0).forEach((resolve) => resolve());

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("segment_translate_explain_cmd", expect.any(Object));
    });
  });
});
