import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { ArticleReader } from "./ArticleReader";
import { ArticleMindMapPanel } from "./ArticleMindMapPanel";
import { AgentTask, Article, Artifact, MindMapResult } from "../../types";

const invokeMock = vi.fn();
const listenerMap = new Map<string, (event: { payload: unknown }) => void>();
const mindElixirListeners = new Map<string, (...args: any[]) => void>();
let mindElixirCurrentData: any = null;
const localStorageStore = new Map<string, string>();

const translations: Record<string, string> = {
  "articleReader.explanation": "讲解",
  "articleReader.mindMap": "思维导图",
  "articleReader.chat": "对话",
  "articleReader.assistantPanel.compact": "1/3",
  "articleReader.assistantPanel.wide": "2/3",
  "articleReader.assistantPanel.full": "全屏",
  "articleReader.mindMapPanel.title": "生成文章思维导图",
  "articleReader.mindMapPanel.description": "从原始内容提取主题结构，并保留与文章的证据关联。",
  "articleReader.mindMapPanel.generate": "生成思维导图",
  "articleReader.mindMapPanel.generateAgain": "重新生成",
  "articleReader.mindMapPanel.notApplicable": "当前内容不适合生成思维导图",
  "articleReader.mindMapPanel.agentStatus": "Agent 状态",
  "articleReader.mindMapPanel.agentHealth.unhealthy": "异常",
  "articleReader.mindMapPanel.selectedNode": "选中节点",
  "articleReader.mindMapPanel.unsavedEdits": "本地编辑未保存",
  "articleReader.mindMapPanel.nodeCount": "节点数",
  "articleReader.mindMapPanel.coverage": "覆盖度",
  "articleReader.mindMapPanel.evidenceDensity": "证据密度",
  "articleReader.mindMapPanel.confidence": "置信度",
  "articleReader.mindMapPanel.evidenceCount": "证据片段",
  "articleReader.mindMapPanel.save": "保存脑图",
  "articleReader.mindMapPanel.saving": "保存中...",
  "articleReader.mindMapPanel.showDetails": "显示节点详情",
  "articleReader.mindMapPanel.hideDetails": "隐藏节点详情",
  "articleReader.mindMapPanel.showLogs": "显示运行日志",
  "articleReader.mindMapPanel.hideLogs": "隐藏运行日志",
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("mind-elixir/style.css", () => ({}));

vi.mock("mind-elixir", () => {
  type MockNode = {
    topic: string;
    note?: string;
    children?: MockNode[];
  };

  class MindElixirMock {
    static SIDE = 2;

    el: HTMLElement;
    currentData: { nodeData: MockNode } | null = null;
    bus = {
      addListener: vi.fn((eventName: string, handler: (...args: any[]) => void) => {
        mindElixirListeners.set(eventName, handler);
      }),
      removeListener: vi.fn((eventName: string) => {
        mindElixirListeners.delete(eventName);
      }),
    };

    constructor(options: { el: HTMLElement }) {
      this.el = options.el;
    }

    init(data: { nodeData: MockNode }) {
      this.currentData = data;
      mindElixirCurrentData = data;
      const renderNode = (node: MockNode): string =>
        [node.topic, node.note, ...(node.children ?? []).flatMap(renderNode)].filter(Boolean).join(" ");
      this.el.textContent = renderNode(data.nodeData);
      return undefined;
    }

    getData() {
      return mindElixirCurrentData ?? this.currentData ?? { nodeData: { topic: "", children: [] } };
    }

    toCenter() {}
    destroy() {}
  }

  return {
    __esModule: true,
    default: MindElixirMock,
    SIDE: 2,
  };
});

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => translations[key] ?? fallback ?? key,
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (eventName: string, handler: (event: { payload: unknown }) => void) => {
    listenerMap.set(eventName, handler);
    return () => listenerMap.delete(eventName);
  }),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: vi.fn(),
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
    config: {
      target_language: "zh-CN",
      active_model_id: "model-1",
      model_configs: [
        {
          id: "model-1",
          name: "Model",
          api_provider: "openai",
          api_key: "key",
          model: "gpt-4o-mini",
          is_default: true,
        },
      ],
    },
  }),
}));

vi.mock("./ArticleChatAssistant", () => ({
  ArticleChatAssistant: () => <div data-testid="article-chat-assistant" />,
}));

vi.mock("./ArticleExplanationPanel", () => ({
  ArticleExplanationPanel: () => <div data-testid="article-explanation-panel" />,
}));

vi.mock("./VideoSubtitlePlayer", () => ({
  VideoSubtitlePlayer: () => <div data-testid="video-subtitle-player" />,
}));

function createArticle(overrides: Partial<Article> = {}): Article {
  return {
    id: "article-1",
    title: "Sample Article",
    content: "Alpha beta gamma. Delta epsilon zeta.",
    created_at: "2026-03-07T00:00:00Z",
    translated: false,
    segments: [
      {
        id: "seg-1",
        article_id: "article-1",
        order: 0,
        text: "Alpha beta gamma.",
        created_at: "2026-03-07T00:00:00Z",
      },
    ],
    ...overrides,
  };
}

function createTask(overrides: Partial<AgentTask> = {}): AgentTask {
  return {
    id: "task-1",
    task_type: "mind_map_generate",
    status: "queued",
    article_id: "article-1",
    input: {
      article_id: "article-1",
      display_language: "zh-CN",
      max_depth: 3,
      evidence_mode: "strict",
      prefer_structure: "topic_tree",
    },
    progress: 0,
    stage: "queued",
    artifact_ids: [],
    created_at: "2026-03-07T00:00:00Z",
    updated_at: "2026-03-07T00:00:00Z",
    ...overrides,
  };
}

function createMindMapResult(overrides: Partial<MindMapResult> = {}): MindMapResult {
  return {
    status: "applicable",
    reason: null,
    map: {
      version: "1",
      article_id: "article-1",
      title: "Sample Article",
      display_language: "zh-CN",
      generation_mode: "evidence_first",
      source_hash: "sha256:test",
      summary: "Summary",
      root: {
        id: "root",
        title: "Sample Article",
        node_type: "root",
        summary: "Summary",
        confidence: 0.9,
        source_segment_ids: ["seg-1"],
        source_offsets: [],
        children: [
          {
            id: "node-theme",
            title: "Core Theme",
            node_type: "theme",
            summary: "Theme summary",
            confidence: 0.8,
            source_segment_ids: ["seg-1"],
            source_offsets: [],
            children: [],
          },
        ],
      },
    },
    diagnostics: {
      content_type: "article",
      coverage: "full",
      notes: [],
      window_count: 1,
      evidence_density: 1,
      low_confidence_node_ids: [],
    },
    ...overrides,
  };
}

function createArtifact(result: MindMapResult): Artifact {
  return {
    id: "artifact-1",
    task_id: "task-1",
    article_id: "article-1",
    artifact_type: "mind_map",
    version: "1",
    content: result,
    created_at: "2026-03-07T00:00:00Z",
    updated_at: "2026-03-07T00:00:00Z",
  };
}

describe("ArticleMindMapPanel", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenerMap.clear();
    mindElixirListeners.clear();
    mindElixirCurrentData = null;
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

  it("renders the new mind map tab in the article reader", async () => {
    invokeMock.mockResolvedValue(null);

    render(<ArticleReader article={createArticle()} />);

    expect(screen.getByRole("button", { name: "1/3" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "2/3" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "全屏" })).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "思维导图" }));

    expect(screen.getByRole("button", { name: "生成思维导图" })).toBeInTheDocument();
  });

  it("switches assistant panel layout modes and persists the choice", async () => {
    invokeMock.mockResolvedValue(null);

    render(<ArticleReader article={createArticle()} />);

    const shell = screen.getByTestId("article-reader-shell");
    const mainPane = screen.getByTestId("article-reader-main-pane");
    const assistantPane = screen.getByTestId("article-reader-assistant-pane");

    expect(shell).toHaveAttribute("data-assistant-mode", "compact");
    expect(mainPane).toHaveAttribute("data-hidden", "false");
    expect(assistantPane).toHaveAttribute("data-assistant-mode", "compact");

    await userEvent.click(screen.getByRole("button", { name: "2/3" }));
    expect(shell).toHaveAttribute("data-assistant-mode", "wide");
    expect(window.localStorage.getItem("article-reader-assistant-mode")).toBe("wide");

    await userEvent.click(screen.getByRole("button", { name: "全屏" }));
    expect(shell).toHaveAttribute("data-assistant-mode", "full");
    expect(mainPane).toHaveAttribute("data-hidden", "true");
    expect(assistantPane).toHaveAttribute("data-assistant-mode", "full");
    expect(window.localStorage.getItem("article-reader-assistant-mode")).toBe("full");

    cleanup();

    render(<ArticleReader article={createArticle()} />);
    expect(screen.getByTestId("article-reader-shell")).toHaveAttribute("data-assistant-mode", "full");
  });

  it("keeps the wide assistant layout when switching from chat to mind map", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "healthy",
          logs: [],
        });
      }
      if (command === "get_artifact_cmd") {
        return Promise.resolve(createArtifact(createMindMapResult({})));
      }
      return Promise.resolve(null);
    });

    render(<ArticleReader article={createArticle({ active_mind_map_artifact_id: "artifact-1" })} />);

    const shell = screen.getByTestId("article-reader-shell");
    const assistantPane = screen.getByTestId("article-reader-assistant-pane");

    await userEvent.click(screen.getByRole("button", { name: "2/3" }));
    await userEvent.click(screen.getByRole("button", { name: "对话" }));
    await userEvent.click(screen.getByRole("button", { name: "思维导图" }));

    expect(shell).toHaveAttribute("data-assistant-mode", "wide");
    expect(assistantPane).toHaveAttribute("data-assistant-mode", "wide");
    expect(assistantPane.className).toContain("basis-[64%]");
    expect(assistantPane.className).toContain("min-w-0");
    expect(assistantPane.className).toContain("overflow-hidden");
  });

  it("shows a generate CTA before a task exists and starts a task on click", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "stopped",
          logs: [],
        });
      }
      if (command === "create_mind_map_task_cmd") {
        return Promise.resolve(createTask());
      }
      return Promise.resolve(null);
    });

    render(<ArticleMindMapPanel article={createArticle()} targetLanguage="zh-CN" />);

    await userEvent.click(screen.getByRole("button", { name: "生成思维导图" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "create_mind_map_task_cmd",
        expect.objectContaining({
          articleId: "article-1",
          displayLanguage: "zh-CN",
        }),
      );
    });
  });

  it("shows progress updates while generation is running", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "healthy",
          worker_session_id: "worker-1",
          logs: [
            {
              timestamp: "2026-03-07T00:00:00Z",
              level: "info",
              source: "worker",
              message: "worker ready",
            },
          ],
        });
      }
      if (command === "create_mind_map_task_cmd") {
        return Promise.resolve(createTask());
      }
      return Promise.resolve(null);
    });

    render(<ArticleMindMapPanel article={createArticle()} targetLanguage="zh-CN" />);

    await userEvent.click(screen.getByRole("button", { name: "生成思维导图" }));

    listenerMap.get("agent-task-updated")?.({
      payload: createTask({
        status: "running",
        stage: "reading",
        progress: 0.42,
        message: "Reading source windows",
      }),
    });

    expect(await screen.findByText("Reading source windows")).toBeInTheDocument();
    expect(screen.getByText("42%")).toBeInTheDocument();
    expect(screen.getByText("Agent 状态")).toBeInTheDocument();
    expect(screen.getByText("worker ready")).toBeInTheDocument();
  });

  it("renders a not-applicable empty state from the saved artifact", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "healthy",
          logs: [],
        });
      }
      if (command === "get_artifact_cmd") {
        return Promise.resolve(
          createArtifact(
            createMindMapResult({
              status: "not_applicable",
              reason: "music_only",
              map: null,
              diagnostics: {
                content_type: "music_only",
                coverage: "none",
                notes: ["No stable semantic content detected."],
                window_count: 1,
                evidence_density: 0,
                low_confidence_node_ids: [],
              },
            }),
          ),
        );
      }
      return Promise.resolve(null);
    });

    render(
      <ArticleMindMapPanel
        article={createArticle({ active_mind_map_artifact_id: "artifact-1" })}
        targetLanguage="zh-CN"
      />,
    );

    expect(await screen.findByText("当前内容不适合生成思维导图")).toBeInTheDocument();
    expect(screen.getByText("No stable semantic content detected.")).toBeInTheDocument();
  });

  it("renders a successful tree result from the saved artifact", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "healthy",
          logs: [],
        });
      }
      if (command === "get_artifact_cmd") {
        return Promise.resolve(createArtifact(createMindMapResult({})));
      }
      return Promise.resolve(null);
    });

    render(
      <ArticleMindMapPanel
        article={createArticle({ active_mind_map_artifact_id: "artifact-1" })}
        targetLanguage="zh-CN"
      />,
    );

    expect(await screen.findByTestId("mind-elixir-canvas")).toBeInTheDocument();
    expect(screen.getByTestId("mind-elixir-canvas")).toBeInTheDocument();
    expect(screen.getByText("Core Theme", { exact: false })).toBeInTheDocument();
    expect(screen.getByText("选中节点")).toBeInTheDocument();
  });

  it("collapses details and logs by default in compact mode", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "healthy",
          logs: [
            {
              timestamp: "2026-03-07T00:00:11Z",
              level: "info",
              source: "runtime",
              message: "ready",
            },
          ],
        });
      }
      if (command === "get_artifact_cmd") {
        return Promise.resolve(createArtifact(createMindMapResult({})));
      }
      return Promise.resolve(null);
    });

    render(
      <ArticleMindMapPanel
        article={createArticle({ active_mind_map_artifact_id: "artifact-1" })}
        targetLanguage="zh-CN"
        panelMode="compact"
      />,
    );

    expect(await screen.findByTestId("mind-map-panel-root")).toHaveAttribute("data-panel-mode", "compact");
    expect(screen.getByRole("button", { name: "显示节点详情" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "显示运行日志" })).toBeInTheDocument();
    expect(screen.queryByText("选中节点")).not.toBeInTheDocument();
    expect(screen.queryByText("ready")).not.toBeInTheDocument();
  });

  it("loads an artifact from task updates even if the task status is interrupted", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "healthy",
          logs: [],
        });
      }
      if (command === "create_mind_map_task_cmd") {
        return Promise.resolve(createTask());
      }
      if (command === "get_artifact_cmd") {
        return Promise.resolve(createArtifact(createMindMapResult({})));
      }
      return Promise.resolve(null);
    });

    render(<ArticleMindMapPanel article={createArticle()} targetLanguage="zh-CN" />);

    await userEvent.click(screen.getByRole("button", { name: "生成思维导图" }));

    listenerMap.get("agent-task-updated")?.({
      payload: createTask({
        status: "interrupted",
        artifact_ids: ["artifact-1"],
        progress: 1,
        stage: "done",
        message: "Mind map generated",
      }),
    });

    expect(await screen.findByText("Core Theme", { exact: false })).toBeInTheDocument();
    expect(screen.getByTestId("mind-elixir-canvas")).toBeInTheDocument();
  });

  it("saves local mind map edits as a new artifact", async () => {
    const baseResult = createMindMapResult({});

    invokeMock.mockImplementation((command: string, args: any) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "healthy",
          logs: [],
        });
      }
      if (command === "get_artifact_cmd") {
        return Promise.resolve(createArtifact(baseResult));
      }
      if (command === "artifact_save_cmd") {
        return Promise.resolve({
          ...createArtifact({
            ...baseResult,
            map: {
              ...baseResult.map!,
              title: "Edited Root",
              root: {
                ...baseResult.map!.root,
                title: "Edited Root",
              },
            },
          }),
          id: "artifact-2",
          task_id: args.taskId,
        });
      }
      return Promise.resolve(null);
    });

    render(
      <ArticleMindMapPanel
        article={createArticle({ active_mind_map_artifact_id: "artifact-1" })}
        targetLanguage="zh-CN"
      />,
    );

    expect(await screen.findByTestId("mind-elixir-canvas")).toBeInTheDocument();

    // Wait for the MindElixir useEffect to register the operation listener
    await waitFor(() => {
      expect(mindElixirListeners.has("operation")).toBe(true);
    });

    mindElixirCurrentData = {
      nodeData: {
        id: "root",
        topic: "Edited Root",
        note: "Summary",
        metadata: {
          summary: "Summary",
          confidence: 0.9,
          nodeType: "root",
          sourceSegmentIds: ["seg-1"],
          sourceOffsets: [],
        },
        children: [
          {
            id: "node-theme",
            topic: "Core Theme",
            note: "Theme summary",
            metadata: {
              summary: "Theme summary",
              confidence: 0.8,
              nodeType: "theme",
              sourceSegmentIds: ["seg-1"],
              sourceOffsets: [],
            },
            children: [],
          },
        ],
      },
    };

    await act(async () => {
      mindElixirListeners.get("operation")?.({
        name: "finishEdit",
        obj: {
          id: "root",
          topic: "Edited Root",
          note: "Summary",
          metadata: {
            summary: "Summary",
            confidence: 0.9,
            nodeType: "root",
            sourceSegmentIds: ["seg-1"],
            sourceOffsets: [],
          },
          children: [
            {
              id: "node-theme",
              topic: "Core Theme",
              note: "Theme summary",
              metadata: {
                summary: "Theme summary",
                confidence: 0.8,
                nodeType: "theme",
                sourceSegmentIds: ["seg-1"],
                sourceOffsets: [],
              },
              children: [],
            },
          ],
        },
        origin: "Sample Article",
      });
    });

    await waitFor(() => {
      expect(screen.getByText("本地编辑未保存")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "保存脑图" })).toBeEnabled();
    });

    await userEvent.click(screen.getByRole("button", { name: "保存脑图" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith(
        "artifact_save_cmd",
        expect.objectContaining({
          articleId: "article-1",
          content: expect.objectContaining({
            map: expect.objectContaining({
              title: "Edited Root",
            }),
          }),
        }),
      );
    });

    expect(await screen.findByText("产物 ID: artifact-2")).toBeInTheDocument();
  });

  it("loads and renders worker status and recent logs", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_agent_worker_status_cmd") {
        return Promise.resolve({
          health: "unhealthy",
          worker_session_id: "worker-42",
          started_at: "2026-03-07T00:00:00Z",
          last_heartbeat_at: "2026-03-07T00:00:10Z",
          logs: [
            {
              timestamp: "2026-03-07T00:00:11Z",
              level: "warn",
              source: "stderr",
              message: "heartbeat delayed",
            },
          ],
        });
      }
      return Promise.resolve(null);
    });

    render(<ArticleMindMapPanel article={createArticle()} targetLanguage="zh-CN" />);

    expect(await screen.findByText("Agent 状态")).toBeInTheDocument();
    expect(screen.getByText("异常")).toBeInTheDocument();
    expect(screen.getByText("worker-42")).toBeInTheDocument();
    expect(screen.getByText("heartbeat delayed")).toBeInTheDocument();
  });
});
