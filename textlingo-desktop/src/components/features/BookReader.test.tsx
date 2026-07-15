import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { BookReader } from "./BookReader";
import { Article } from "../../types";

const invokeMock = vi.fn();
const localStorageStore = new Map<string, string>();
const configMock = vi.hoisted(() => ({
  current: {
    target_language: "zh-CN",
    active_model_id: "model-1",
    model_configs: [
      {
        id: "model-1",
        api_provider: "openai",
        api_key: "secret",
        model: "gpt-4o-mini",
      },
    ],
  } as any,
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(),
}));

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (_key: string, fallback?: string) => fallback ?? _key,
  }),
}));

vi.mock("../../lib/hooks", () => ({
  useConfig: () => ({
    config: configMock.current,
  }),
}));

vi.mock("./TxtReader", () => ({
  TxtReader: ({ initialProgress, onProgressChange }: { initialProgress?: { reader_kind: string }; onProgressChange?: (progress: unknown) => void }) => (
    <button
      data-testid="txt-reader"
      data-reader-kind={initialProgress?.reader_kind}
      onClick={() => onProgressChange?.({ reader_kind: "txt", locator: { kind: "page", page: 1, total_pages: 1 }, progress_ratio: 1, status: "completed" })}
    >
      TXT Reader
    </button>
  ),
}));

vi.mock("./PdfReader", () => ({
  PdfReader: () => <div data-testid="pdf-reader">PDF Reader</div>,
}));

vi.mock("./EpubReader", () => ({
  EpubReader: () => <div data-testid="epub-reader">EPUB Reader</div>,
}));

vi.mock("./ArticleChatAssistant", () => ({
  ArticleChatAssistant: () => <div data-testid="article-chat-assistant">Chat Assistant</div>,
}));

vi.mock("./ArticleMindMapPanel", () => ({
  ArticleMindMapPanel: ({ panelMode }: { panelMode?: string }) => (
    <div data-testid="article-mind-map-panel" data-panel-mode={panelMode ?? "unknown"}>
      Mind Map Panel
    </div>
  ),
}));

function createBookArticle(overrides: Partial<Article> = {}): Article {
  return {
    id: "book-1",
    title: "Book Title",
    content: "Book content",
    created_at: "2026-03-07T00:00:00Z",
    translated: false,
    book_type: "txt",
    book_path: "/tmp/book.txt",
    ...overrides,
  };
}

describe("BookReader", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_resource_server_info_cmd") {
        return Promise.resolve({
          base_url: "http://127.0.0.1:19420",
          token: "test-token",
        });
      }
      return Promise.resolve({});
    });
    vi.stubGlobal("confirm", vi.fn(() => false));
    vi.stubGlobal("alert", vi.fn());
    localStorageStore.clear();
    configMock.current = {
      target_language: "zh-CN",
      active_model_id: "model-1",
      model_configs: [
        {
          id: "model-1",
          api_provider: "openai",
          api_key: "secret",
          model: "gpt-4o-mini",
        },
      ],
    };
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

  it("renders mind map and chat tabs for books and defaults to the mind map", async () => {
    render(<BookReader article={createBookArticle()} />);

    expect(screen.getByRole("button", { name: "思维导图" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "对话" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1/3" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "2/3" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "全屏" })).toBeInTheDocument();
    expect(screen.getByTestId("article-mind-map-panel")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "对话" }));
    expect(screen.getByTestId("article-chat-assistant")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "全屏" }));
    expect(screen.getByTestId("book-reader-shell")).toHaveAttribute("data-assistant-mode", "full");
    expect(screen.getByTestId("book-reader-main-pane")).toHaveAttribute("data-hidden", "true");
  });

  it("reuses the same assistant shell for pdf books", () => {
    render(<BookReader article={createBookArticle({ book_type: "pdf", book_path: "/tmp/book.pdf" })} />);

    expect(screen.getByTestId("book-reader-shell")).toBeInTheDocument();
    expect(screen.getByTestId("pdf-reader")).toBeInTheDocument();
    expect(screen.getByTestId("article-mind-map-panel")).toBeInTheDocument();
  });

  it("keeps the imported original immutable and opens an editable derivative through the route callback", async () => {
    const onOpenEditableDerivative = vi.fn().mockResolvedValue(undefined);
    render(
      <BookReader
        article={createBookArticle({ book_type: "pdf", book_path: "/tmp/book.pdf" })}
        onOpenEditableDerivative={onOpenEditableDerivative}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "创建或打开可编辑副本" }));
    expect(onOpenEditableDerivative).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("pdf-reader")).toBeInTheDocument();
  });

  it("passes the reading-progress contract through to the active book reader", async () => {
    const onProgressChange = vi.fn();
    render(
      <BookReader
        article={createBookArticle()}
        initialProgress={{ reader_kind: "txt", locator: { kind: "page", page: 2, total_pages: 4 }, progress_ratio: 0.5, status: "reading" }}
        onProgressChange={onProgressChange}
      />,
    );

    const reader = screen.getByTestId("txt-reader");
    expect(reader).toHaveAttribute("data-reader-kind", "txt");
    await userEvent.click(reader);
    expect(onProgressChange).toHaveBeenCalledWith(expect.objectContaining({ locator: { kind: "page", page: 1, total_pages: 1 }, status: "completed" }));
  });

  it("keeps a back button available when the assistant is restored in full mode", async () => {
    localStorageStore.set("book-reader-assistant-mode", "full");
    const onBack = vi.fn();

    render(
      <BookReader
        article={createBookArticle({ book_type: "pdf", book_path: "/tmp/book.pdf" })}
        onBack={onBack}
      />,
    );

    expect(screen.getByTestId("book-reader-shell")).toHaveAttribute("data-assistant-mode", "full");
    expect(screen.getByTestId("book-reader-main-pane")).toHaveAttribute("data-hidden", "true");

    const assistantPane = screen.getByTestId("book-reader-assistant-pane");
    const backButton = within(assistantPane).getByRole("button", { name: "返回素材列表" });
    expect(backButton).toBeInTheDocument();

    await userEvent.click(backButton);
    expect(onBack).toHaveBeenCalledTimes(1);
  });

  it("renders a local-reading message instead of AI panels when no model is configured", () => {
    configMock.current = {
      target_language: "zh-CN",
      active_model_id: undefined,
      model_configs: [],
    };

    render(<BookReader article={createBookArticle()} />);

    expect(screen.queryByTestId("article-mind-map-panel")).not.toBeInTheDocument();
    expect(screen.getByText("基础阅读可用。配置 AI 模型后可启用翻译、讲解和分析。")).toBeInTheDocument();
  });

  it("enables pdf translation when the capability is enabled", async () => {
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "check_pdf_translation_files") {
        return {};
      }
      if (command === "get_resource_server_info_cmd") {
        return {
          base_url: "http://127.0.0.1:19420",
          token: "test-token",
        };
      }
      if (command === "get_config") {
        return {
          target_language: "zh-CN",
          active_model_id: "model-1",
          model_configs: [
            {
              id: "model-1",
              api_provider: "openai",
              api_key: "secret",
              model: "gpt-4o-mini",
            },
          ],
        };
      }
      if (command === "translate_pdf_document") {
        return {
          success: true,
          mono_pdf: "/tmp/book-mono.pdf",
          dual_pdf: "/tmp/book-dual.pdf",
          original_pdf: "/tmp/book.pdf",
        };
      }

      return {};
    });

    render(<BookReader article={createBookArticle({ book_type: "pdf", book_path: "/tmp/book.pdf" })} />);

    const translateButton = screen.getByRole("button", { name: "翻译全文" });
    expect(translateButton).toBeEnabled();

    expect(invokeMock.mock.calls.some(([command]) => command === "check_plugin_installed_cmd")).toBe(false);
    await userEvent.click(translateButton);
    expect(invokeMock.mock.calls.some(([command]) => command === "translate_pdf_document")).toBe(true);
  });
});
