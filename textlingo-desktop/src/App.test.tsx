import type { ButtonHTMLAttributes } from "react";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";

const invokeMock = vi.fn();
const getApiClientMock = vi.fn();
const appShellMocks = vi.hoisted(() => ({
  capturedAgentOpenMaterial: null as null | ((materialId: string) => void),
  capturedDragDropHandler: null as null | ((event: { payload: { type: string; paths?: string[] } }) => unknown),
  onDragDropEvent: vi.fn(),
  dragDropUnlisten: vi.fn(),
}));

vi.stubGlobal("__APP_VERSION__", "test");

const authenticatedBackendSession = {
  configured: true,
  connected: true,
  authenticated: true,
  backend_url: "http://127.0.0.1:4000",
  user: {
    id: "user-1",
    email: "reader@example.com",
    display_name: "Reader",
    created_at: "2026-03-30T00:00:00Z",
    updated_at: "2026-03-30T00:00:00Z",
  },
  error: null,
};

beforeEach(() => {
  invokeMock.mockReset();
  getApiClientMock.mockReset();
  appShellMocks.capturedAgentOpenMaterial = null;
  appShellMocks.capturedDragDropHandler = null;
  appShellMocks.dragDropUnlisten.mockReset();
  appShellMocks.onDragDropEvent.mockReset();
  appShellMocks.onDragDropEvent.mockImplementation(async (handler) => {
    appShellMocks.capturedDragDropHandler = handler;
    return appShellMocks.dragDropUnlisten;
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.clearAllMocks();
});

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (...args: unknown[]) => appShellMocks.onDragDropEvent(...args),
  }),
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
  useAgentOpenMaterialListener: (handler: (materialId: string) => void) => {
    appShellMocks.capturedAgentOpenMaterial = handler;
  },
}));

describe("App onboarding", () => {
  it("waits for the packaged backend before restoring the session", async () => {
    vi.useFakeTimers();
    let backendStatusChecks = 0;
    invokeMock.mockImplementation((command: string) => {
      if (command === "packaged_backend_status_cmd") {
        backendStatusChecks += 1;
        return Promise.resolve({
          enabled: true,
          running: backendStatusChecks >= 3,
          message: backendStatusChecks >= 3 ? "ready" : null,
        });
      }
      if (command === "get_config") {
        return Promise.resolve({
          onboarding_completed: true,
          model_configs: [],
          prompt_features: [],
        });
      }
      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
      }
      if (command === "list_articles_cmd") return Promise.resolve([]);
      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);
    expect(screen.getByText("app.loading")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(400);
    });

    expect(screen.getByText("ArticleList")).toBeInTheDocument();
    expect(backendStatusChecks).toBe(3);
    expect(invokeMock).toHaveBeenCalledWith("backend_check_session_cmd");
  });

  it("leaves the loading screen when a startup command never settles", async () => {
    vi.useFakeTimers();
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return new Promise(() => {});
      }

      if (command === "backend_check_session_cmd") {
        return Promise.resolve({
          configured: true,
          connected: true,
          authenticated: false,
          backend_url: "http://127.0.0.1:19421",
          user: null,
          error: null,
        });
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);
    expect(screen.getByText("app.loading")).toBeInTheDocument();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(8_000);
    });

    expect(screen.getByText("OpenKoto Backend")).toBeInTheDocument();
    expect(screen.getByText("需要登录 Backend")).toBeInTheDocument();
  });

  it("blocks material loading until backend is configured and authenticated", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve(null);
      }

      if (command === "backend_check_session_cmd") {
        return Promise.resolve({
          configured: false,
          connected: false,
          authenticated: false,
          backend_url: null,
          user: null,
          error: null,
        });
      }

      if (command === "list_articles_cmd") {
        throw new Error("list_articles_cmd should not run before backend auth");
      }

      if (command === "import_book_cmd" || command === "get_article") {
        throw new Error(`${command} should not run before backend auth`);
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);

    expect(await screen.findByText("OpenKoto Backend")).toBeInTheDocument();
    expect(screen.getByText("需要配置 Backend 地址")).toBeInTheDocument();
    await waitFor(() => {
      expect(appShellMocks.capturedDragDropHandler).toEqual(expect.any(Function));
      expect(appShellMocks.capturedAgentOpenMaterial).toEqual(expect.any(Function));
    });

    await act(async () => {
      await appShellMocks.capturedDragDropHandler?.({
        payload: { type: "drop", paths: ["/tmp/dropped.pdf"] },
      });
    });
    act(() => {
      appShellMocks.capturedAgentOpenMaterial?.("article-remote");
    });

    expect(invokeMock).not.toHaveBeenCalledWith("list_articles_cmd");
    expect(invokeMock).not.toHaveBeenCalledWith("import_book_cmd", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("get_article", expect.anything());
  });

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

      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
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

  it("opens ktv export when the capability is enabled", async () => {
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

      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
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

    expect(screen.getByText("KtvExportPage")).toBeInTheDocument();
  });

  it("shows the signed-in backend account and returns to the login gate after logout", async () => {
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
    ];

    const authenticatedConfig = {
      onboarding_completed: true,
      active_model_id: undefined,
      model_configs: [],
      target_language: "zh-CN",
      interface_language: "zh",
      prompt_features: [],
      backend_url: "http://127.0.0.1:4000",
      auth_token: "token",
    };
    const loggedOutConfig = {
      ...authenticatedConfig,
      auth_token: undefined,
    };

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve(authenticatedConfig);
      }

      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
      }

      if (command === "list_articles_cmd") {
        return Promise.resolve(sampleArticles);
      }

      if (command === "backend_logout_cmd") {
        return Promise.resolve(loggedOutConfig);
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);

    expect(await screen.findByText("ArticleList")).toBeInTheDocument();
    expect(screen.getByText("Reader")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "账户" }));
    expect(await screen.findByText("reader@example.com")).toBeInTheDocument();

    await userEvent.click(screen.getByText("退出登录"));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("backend_logout_cmd");
    });
    expect(await screen.findByText("OpenKoto Backend")).toBeInTheDocument();
    expect(screen.getByText("需要登录 Backend")).toBeInTheDocument();
    expect(screen.queryByText("ArticleList")).not.toBeInTheDocument();
    expect(screen.queryByText("Article One")).not.toBeInTheDocument();
  });

  it("switches accounts by clearing the session and restores materials after re-authentication", async () => {
    const sampleArticles = [{
      id: "article-1", title: "Article One", content: "one", source_type: "article",
      source_url: null, media_path: null, book_path: null, book_type: null,
      created_at: "2026-03-30T00:00:00Z", translated: false,
      active_mind_map_artifact_id: null, segments: [],
    }];
    const authenticatedConfig = {
      onboarding_completed: true, active_model_id: undefined, model_configs: [],
      target_language: "zh-CN", interface_language: "zh", prompt_features: [],
      backend_url: "http://127.0.0.1:4000", auth_token: "account-a-token",
    };
    const loggedOutConfig = { ...authenticatedConfig, auth_token: undefined };
    let session: "authenticated" | "logged-out" = "authenticated";

    invokeMock.mockImplementation((command: string, args?: { email?: string; password?: string }) => {
      if (command === "get_config") return Promise.resolve(session === "authenticated" ? authenticatedConfig : loggedOutConfig);
      if (command === "backend_check_session_cmd") {
        return Promise.resolve(session === "authenticated" ? authenticatedBackendSession : {
          ...authenticatedBackendSession, authenticated: false, user: null, error: null,
        });
      }
      if (command === "list_articles_cmd") return Promise.resolve(session === "authenticated" ? sampleArticles : []);
      if (command === "backend_logout_cmd") {
        session = "logged-out";
        return Promise.resolve(loggedOutConfig);
      }
      if (command === "backend_login_cmd") {
        expect(args).toMatchObject({ email: "new@example.com", password: "new-password" });
        session = "authenticated";
        return Promise.resolve({ config: authenticatedConfig, user: authenticatedBackendSession.user, expires_at: "2026-03-31T00:00:00Z" });
      }
      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);
    expect(await screen.findByText("ArticleList")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "账户" }));
    await userEvent.click(screen.getByText("切换账户"));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("backend_logout_cmd"));
    expect(await screen.findByText("需要登录 Backend")).toBeInTheDocument();
    expect(screen.queryByText("ArticleList")).not.toBeInTheDocument();
    expect(screen.queryByText("Article One")).not.toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Email"), "new@example.com");
    await userEvent.type(screen.getByLabelText("Password"), "new-password");
    await userEvent.click(screen.getByRole("button", { name: "登录" }));

    expect(await screen.findByText("ArticleList")).toBeInTheDocument();
    expect(screen.getByText("Article One")).toBeInTheDocument();
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

      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
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

  it("opens an existing material from the agent event without fetching it again", async () => {
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
    ];

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve({
          onboarding_completed: true,
          active_model_id: undefined,
          model_configs: [],
          target_language: "zh-CN",
          interface_language: "en",
          prompt_features: [],
        });
      }

      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
      }

      if (command === "list_articles_cmd") {
        return Promise.resolve(sampleArticles);
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);

    expect(await screen.findByText("ArticleList")).toBeInTheDocument();
    await waitFor(() => {
      expect(appShellMocks.capturedAgentOpenMaterial).toEqual(expect.any(Function));
    });

    act(() => {
      appShellMocks.capturedAgentOpenMaterial?.("article-1");
    });

    expect(await screen.findByText("Reading Article One")).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("get_article", expect.anything());
  });

  it("fetches and opens a missing material from the agent event", async () => {
    const fetchedArticle = {
      id: "article-remote",
      title: "Fetched Article",
      content: "remote",
      source_type: "article",
      source_url: null,
      media_path: null,
      book_path: null,
      book_type: null,
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: null,
      segments: [],
    };

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve({
          onboarding_completed: true,
          active_model_id: undefined,
          model_configs: [],
          target_language: "zh-CN",
          interface_language: "en",
          prompt_features: [],
        });
      }

      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
      }

      if (command === "list_articles_cmd") {
        return Promise.resolve([]);
      }

      if (command === "get_article") {
        return Promise.resolve(fetchedArticle);
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    render(<App />);

    expect(await screen.findByText("ArticleList")).toBeInTheDocument();
    await waitFor(() => {
      expect(appShellMocks.capturedAgentOpenMaterial).toEqual(expect.any(Function));
    });

    act(() => {
      appShellMocks.capturedAgentOpenMaterial?.("article-remote");
    });

    expect(await screen.findByText("Reading Fetched Article")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_article", { id: "article-remote" });
  });

  it("opens a single dropped file after import and cleans up the drag listener", async () => {
    const importedArticle = {
      id: "book-1",
      title: "Dropped Book",
      content: "book",
      source_type: "book",
      source_url: null,
      media_path: null,
      book_path: "/tmp/dropped.pdf",
      book_type: "pdf",
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: null,
      segments: [],
    };
    let listCalls = 0;

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve({
          onboarding_completed: true,
          active_model_id: undefined,
          model_configs: [],
          target_language: "zh-CN",
          interface_language: "en",
          prompt_features: [],
        });
      }

      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
      }

      if (command === "list_articles_cmd") {
        listCalls += 1;
        return Promise.resolve(listCalls > 1 ? [importedArticle] : []);
      }

      if (command === "preview_material_import_cmd") {
        return Promise.resolve({ job: { id: "job-dropped" }, duplicates: { duplicate: false, matches: [] } });
      }

      if (command === "import_book_cmd") {
        return Promise.resolve(importedArticle);
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    const { unmount } = render(<App />);

    expect(await screen.findByText("ArticleList")).toBeInTheDocument();
    await waitFor(() => {
      expect(appShellMocks.capturedDragDropHandler).toEqual(expect.any(Function));
    });
    const registrationsBeforeDrop = appShellMocks.onDragDropEvent.mock.calls.length;

    await act(async () => {
      await appShellMocks.capturedDragDropHandler?.({
        payload: { type: "drop", paths: ["/tmp/dropped.pdf"] },
      });
    });

    expect(await screen.findByText("BookReader")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("import_book_cmd", {
      filePath: "/tmp/dropped.pdf",
      title: null,
      importJobId: "job-dropped",
      duplicatePolicy: "keep_copy",
    });
    expect(appShellMocks.onDragDropEvent).toHaveBeenCalledTimes(registrationsBeforeDrop);

    unmount();

    expect(appShellMocks.dragDropUnlisten).toHaveBeenCalledTimes(
      appShellMocks.onDragDropEvent.mock.calls.length,
    );
  });

  it("does not continue an in-flight dropped-file import after unmount", async () => {
    const importedArticle = {
      id: "book-1",
      title: "Dropped Book",
      content: "book",
      source_type: "book",
      source_url: null,
      media_path: null,
      book_path: "/tmp/dropped.pdf",
      book_type: "pdf",
      created_at: "2026-03-30T00:00:00Z",
      translated: false,
      active_mind_map_artifact_id: null,
      segments: [],
    };
    let listCalls = 0;
    let resolveImport!: (article: typeof importedArticle) => void;
    const importPromise = new Promise<typeof importedArticle>((resolve) => {
      resolveImport = resolve;
    });

    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") {
        return Promise.resolve({
          onboarding_completed: true,
          active_model_id: undefined,
          model_configs: [],
          target_language: "zh-CN",
          interface_language: "en",
          prompt_features: [],
        });
      }

      if (command === "backend_check_session_cmd") {
        return Promise.resolve(authenticatedBackendSession);
      }

      if (command === "list_articles_cmd") {
        listCalls += 1;
        return Promise.resolve([]);
      }

      if (command === "preview_material_import_cmd") {
        return Promise.resolve({ job: { id: "job-dropped" }, duplicates: { duplicate: false, matches: [] } });
      }

      if (command === "import_book_cmd") {
        return importPromise;
      }

      throw new Error(`Unexpected command: ${command}`);
    });

    const { unmount } = render(<App />);

    expect(await screen.findByText("ArticleList")).toBeInTheDocument();
    await waitFor(() => {
      expect(appShellMocks.capturedDragDropHandler).toEqual(expect.any(Function));
    });

    let dropPromise: Promise<unknown> | undefined;
    await act(async () => {
      dropPromise = appShellMocks.capturedDragDropHandler?.({
        payload: { type: "drop", paths: ["/tmp/dropped.pdf"] },
      }) as Promise<unknown> | undefined;
    });
    expect(dropPromise).toEqual(expect.any(Promise));
    expect(invokeMock).toHaveBeenCalledWith("import_book_cmd", {
      filePath: "/tmp/dropped.pdf",
      title: null,
      importJobId: "job-dropped",
      duplicatePolicy: "keep_copy",
    });
    expect(listCalls).toBe(1);

    unmount();

    await act(async () => {
      resolveImport(importedArticle);
      await dropPromise;
    });

    expect(listCalls).toBe(1);
    expect(appShellMocks.dragDropUnlisten).toHaveBeenCalledTimes(
      appShellMocks.onDragDropEvent.mock.calls.length,
    );
  });
});
