import type { ButtonHTMLAttributes } from "react";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import App from "./App";

const invokeMock = vi.fn();
const getApiClientMock = vi.fn();

vi.stubGlobal("__APP_VERSION__", "test");

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("./components/features/ArticleList", () => ({
  ArticleList: ({
    articles,
    onSelectArticle,
  }: {
    articles: Array<{ id: string; title: string }>;
    onSelectArticle: (article: { id: string; title: string }) => void;
  }) => (
    <div>
      <div>ArticleList</div>
      {articles.map((article) => (
        <button key={article.id} type="button" onClick={() => onSelectArticle(article)}>
          {article.title}
        </button>
      ))}
    </div>
  ),
}));

vi.mock("./components/features/ArticleReader", () => ({
  ArticleReader: ({
    article,
    hasNext,
    onBack,
    onNext,
    onOpenKtvExport,
  }: {
    article: { title: string };
    hasNext?: boolean;
    onBack: () => void;
    onNext: () => void;
    onOpenKtvExport?: () => void;
  }) => (
    <div>
      <div>ArticleReader</div>
      <div>Reading {article.title}</div>
      <button type="button" onClick={onBack}>
        Back to list
      </button>
      <button type="button" onClick={onNext} disabled={!hasNext}>
        Next Article
      </button>
      <button type="button" onClick={onOpenKtvExport}>
        Open KTV Export
      </button>
    </div>
  ),
}));

vi.mock("./components/features/BookReader", () => ({
  BookReader: () => <div>BookReader</div>,
}));

vi.mock("./components/features/KtvExportPage", () => ({
  KtvExportPage: () => <div>KtvExportPage</div>,
}));

vi.mock("./components/features/NewMaterialDialog", () => ({
  NewMaterialDialog: () => null,
}));

vi.mock("./components/features/FavoritesPage", () => ({
  FavoritesPage: ({
    onSelectArticle,
  }: {
    onSelectArticle: (article: { id: string; title: string }) => void;
  }) => (
    <div>
      <div>FavoritesPage</div>
      <button
        type="button"
        onClick={() => onSelectArticle({ id: "article-1", title: "Article One" })}
      >
        Open Favorite Article
      </button>
    </div>
  ),
}));

vi.mock("./components/features/SettingsDialog", () => ({
  SettingsButton: () => <button type="button">settings</button>,
}));

vi.mock("./components/features/ApiQuickSwitcher", () => ({
  ApiQuickSwitcher: ({ onConfigChange }: { onConfigChange: () => void }) => (
    <button type="button" onClick={onConfigChange}>
      Reload Config
    </button>
  ),
}));

vi.mock("./components/features/UpdateChecker", () => ({
  UpdateChecker: () => null,
}));

vi.mock("./components/ui/button", () => ({
  Button: ({ children, ...props }: ButtonHTMLAttributes<HTMLButtonElement>) => (
    <button type="button" {...props}>
      {children}
    </button>
  ),
}));

vi.mock("./components/features/OnboardingDialog", () => ({
  OnboardingDialog: ({
    isOpen,
    onFinish,
  }: {
    isOpen: boolean;
    onFinish: () => void;
  }) =>
    isOpen ? (
      <div>
        <div>Onboarding Visible</div>
        <button type="button" onClick={onFinish}>
          Finish Onboarding
        </button>
      </div>
    ) : null,
}));

vi.mock("./lib/api", () => ({
  getApiClient: (...args: unknown[]) => getApiClientMock(...args),
}));

vi.mock("./lib/hooks/useAgentOpenMaterialListener", () => ({
  useAgentOpenMaterialListener: () => undefined,
}));

describe("App onboarding", () => {
  it("does not reopen onboarding in the same session after the user finishes it", async () => {
    const completedConfig = {
      onboarding_completed: true,
      active_model_id: undefined,
      model_configs: [],
      target_language: "zh-CN",
      interface_language: "en",
      prompt_features: [],
    };

    let configState: "missing" | "completed" = "missing";

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve(configState === "completed" ? completedConfig : null);
      }

      if (command === "list_articles_cmd") {
        return Promise.resolve([]);
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);

    expect(await screen.findByText("Onboarding Visible")).toBeInTheDocument();

    configState = "completed";
    await userEvent.click(screen.getByRole("button", { name: "Finish Onboarding" }));

    await waitFor(() => {
      expect(screen.queryByText("Onboarding Visible")).not.toBeInTheDocument();
    });

    configState = "missing";
    await userEvent.click(screen.getByRole("button", { name: "Reload Config" }));

    await waitFor(() => {
      expect(screen.queryByText("Onboarding Visible")).not.toBeInTheDocument();
    });
  });

  it("keeps ktv export unavailable during phase 1", async () => {
    const sampleVideoArticle = {
      id: "video-1",
      title: "Sample Video",
      content: "hello",
      source_type: "local_video",
      source_url: "file:///tmp/video.mp4",
      media_path: "/tmp/video.mp4",
      book_path: null,
      book_type: null,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: null,
      segments: [
        {
          id: "segment-1",
          article_id: "video-1",
          order: 0,
          text: "こんにちは",
          reading_text: "コンニチハ",
          translation: "你好",
          start_time: 0,
          end_time: 2,
          created_at: "2026-03-30T00:00:00Z",
        },
      ],
    };

    const validConfig = {
      onboarding_completed: true,
      active_model_id: "model-1",
      model_configs: [
        {
          id: "model-1",
          name: "Primary",
          api_key: "secret",
          api_provider: "google",
          model: "gemini-2.0-flash",
          is_default: true,
        },
      ],
      target_language: "zh-CN",
      interface_language: "en",
      prompt_features: [],
    };

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve(validConfig);
      }

      if (command === "list_articles_cmd") {
        return Promise.resolve([sampleVideoArticle]);
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "Sample Video" }));

    expect(await screen.findByText("ArticleReader")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Open KTV Export" }));

    expect(screen.queryByText("KtvExportPage")).not.toBeInTheDocument();
    expect(screen.getByText("ArticleReader")).toBeInTheDocument();
  });

  it("keeps the favorites return target after switching articles in the reader", async () => {
    const sampleArticles = [
      {
        id: "article-1",
        title: "Article One",
        content: "one",
        source_type: "article",
        source_url: null,
        media_path: null,
        book_path: null,
        book_type: null,
        created_at: "2026-03-30T00:00:00Z",
        translated: false,
        active_mind_map_artifact_id: null,
        segments: [],
      },
      {
        id: "article-2",
        title: "Article Two",
        content: "two",
        source_type: "article",
        source_url: null,
        media_path: null,
        book_path: null,
        book_type: null,
        created_at: "2026-03-31T00:00:00Z",
        translated: false,
        active_mind_map_artifact_id: null,
        segments: [],
      },
    ];

    const validConfig = {
      onboarding_completed: true,
      active_model_id: "model-1",
      model_configs: [
        {
          id: "model-1",
          name: "Primary",
          api_key: "secret",
          api_provider: "google",
          model: "gemini-2.0-flash",
          is_default: true,
        },
      ],
      target_language: "zh-CN",
      interface_language: "en",
      prompt_features: [],
    };

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve(validConfig);
      }

      if (command === "list_articles_cmd") {
        return Promise.resolve(sampleArticles);
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);

    await userEvent.click(await screen.findByRole("button", { name: "收藏夹" }));
    expect(await screen.findByText("FavoritesPage")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Open Favorite Article" }));
    expect(await screen.findByText("Reading Article One")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Next Article" }));
    expect(await screen.findByText("Reading Article Two")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Back to list" }));
    expect(await screen.findByText("FavoritesPage")).toBeInTheDocument();
    expect(screen.queryByText("ArticleList")).not.toBeInTheDocument();
  });
});
