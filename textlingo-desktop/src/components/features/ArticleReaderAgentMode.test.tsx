import { cleanup, render, screen, waitFor } from "@testing-library/react";
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

    expect(openMock).toHaveBeenCalled();
    expect(invokeMock).toHaveBeenCalledWith("import_article_subtitles_cmd", {
      articleId: "article-1",
      subtitlePath: "/tmp/sample.srt",
    });
    expect(onUpdate).toHaveBeenCalledTimes(1);
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
