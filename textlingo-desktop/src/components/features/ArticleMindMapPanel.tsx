import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import MindElixir, { type MindElixirData, type MindElixirInstance, type NodeObj } from "mind-elixir";
import "mind-elixir/style.css";
import { AlertCircle, BookOpen, Loader2, RefreshCw, Save, Sparkles, TerminalSquare } from "lucide-react";

import { Button } from "../ui/button";
import {
  AgentTask,
  AgentWorkerStatusSnapshot,
  Artifact,
  Article,
  MindMapNode,
  MindMapNodeType,
  MindMapResult,
  WorkerLogEntry,
} from "../../types";

export type AssistantPanelMode = "compact" | "wide" | "full";

interface ArticleMindMapPanelProps {
  article: Article;
  targetLanguage: string;
  panelMode?: AssistantPanelMode;
}

type MindNodeMetadata = {
  summary: string;
  confidence: number;
  nodeType: MindMapNodeType;
  sourceSegmentIds: string[];
  sourceOffsets: MindMapNode["source_offsets"];
  timeRange?: MindMapNode["time_range"];
};

function formatTimestamp(value?: string) {
  if (!value) {
    return "—";
  }

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return date.toLocaleString();
}

function mapLocale(language: string) {
  if (language.startsWith("zh")) {
    return "zh_CN";
  }
  if (language.startsWith("ja")) {
    return "ja";
  }
  return "en";
}

function toMindElixirNode(node: MindMapNode): NodeObj<MindNodeMetadata> {
  return {
    id: node.id,
    topic: node.title,
    expanded: true,
    tags: [node.node_type.toUpperCase()],
    note: node.summary,
    metadata: {
      summary: node.summary,
      confidence: node.confidence,
      nodeType: node.node_type,
      sourceSegmentIds: node.source_segment_ids,
      sourceOffsets: node.source_offsets,
      timeRange: node.time_range,
    },
    children: node.children.map(toMindElixirNode),
  };
}

function fromMindElixirNode(node: NodeObj<MindNodeMetadata>): MindMapNode {
  const metadata = node.metadata;

  return {
    id: node.id,
    title: node.topic,
    node_type: metadata?.nodeType ?? "topic",
    summary: metadata?.summary ?? node.note ?? "",
    confidence: metadata?.confidence ?? 0.6,
    source_segment_ids: metadata?.sourceSegmentIds ?? [],
    source_offsets: metadata?.sourceOffsets ?? [],
    ...(metadata?.timeRange ? { time_range: metadata.timeRange } : {}),
    children: ((node.children ?? []) as NodeObj<MindNodeMetadata>[]).map(fromMindElixirNode),
  };
}

function toMindElixirData(result: MindMapResult): MindElixirData {
  if (!result.map) {
    throw new Error("Mind map data is required");
  }

  return {
    nodeData: toMindElixirNode(result.map.root),
  };
}

function nodeFromMindNode(node: NodeObj<MindNodeMetadata>): MindMapNode {
  return fromMindElixirNode(node);
}

function countNodes(node: MindMapNode): number {
  return 1 + node.children.reduce((sum, child) => sum + countNodes(child), 0);
}

function StatusBadge({
  health,
  labels,
}: {
  health: AgentWorkerStatusSnapshot["health"];
  labels: Record<AgentWorkerStatusSnapshot["health"], string>;
}) {
  const styles: Record<AgentWorkerStatusSnapshot["health"], string> = {
    healthy: "bg-green-500/15 text-green-700",
    starting: "bg-amber-500/15 text-amber-700",
    unhealthy: "bg-destructive/15 text-destructive",
    stopped: "bg-muted text-muted-foreground",
  };

  return (
    <span className={`rounded-full px-2 py-1 text-[11px] font-medium ${styles[health]}`}>
      {labels[health]}
    </span>
  );
}

function AgentLogPanel({
  status,
  title,
  labels,
  collapsed,
  onToggle,
}: {
  status: AgentWorkerStatusSnapshot | null;
  title: string;
  labels: {
    session: string;
    startedAt: string;
    lastHeartbeat: string;
    empty: string;
    showLogs: string;
    hideLogs: string;
    health: Record<AgentWorkerStatusSnapshot["health"], string>;
  };
  collapsed: boolean;
  onToggle: () => void;
}) {
  if (!status) {
    return null;
  }

  const logs = status.logs.slice(-8).reverse();

  return (
    <div className="mt-4 rounded-2xl border border-border bg-card/70 p-4">
      <div className="flex items-center justify-between gap-3">
        <div className="flex items-center gap-2">
          <TerminalSquare size={16} className="text-muted-foreground" />
          <p className="text-xs font-semibold tracking-wide text-foreground">{title}</p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" size="sm" className="h-7 px-2 text-[11px]" onClick={onToggle}>
            {collapsed ? labels.showLogs : labels.hideLogs}
          </Button>
          <StatusBadge health={status.health} labels={labels.health} />
        </div>
      </div>

      {!collapsed && (
        <>
          <div className="mt-3 grid grid-cols-1 gap-2 text-[11px] text-muted-foreground">
            <div className="flex justify-between gap-3">
              <span>{labels.session}</span>
              <span className="font-mono text-foreground">{status.worker_session_id || "—"}</span>
            </div>
            <div className="flex justify-between gap-3">
              <span>{labels.startedAt}</span>
              <span>{formatTimestamp(status.started_at)}</span>
            </div>
            <div className="flex justify-between gap-3">
              <span>{labels.lastHeartbeat}</span>
              <span>{formatTimestamp(status.last_heartbeat_at)}</span>
            </div>
          </div>

          <div className="mt-4 rounded-xl border border-border bg-background">
            <div className="border-b border-border px-3 py-2 text-[11px] font-medium text-muted-foreground">
              Logs
            </div>
            {logs.length === 0 ? (
              <div className="px-3 py-4 text-[11px] text-muted-foreground">{labels.empty}</div>
            ) : (
              <div className="max-h-44 overflow-y-auto px-3 py-2 font-mono text-[11px]">
                {logs.map((entry: WorkerLogEntry, index) => (
                  <div key={`${entry.timestamp}-${index}`} className="py-1 text-muted-foreground">
                    <span className="mr-2 text-foreground">[{entry.level}]</span>
                    <span className="mr-2">{entry.source}</span>
                    <span>{entry.message}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        </>
      )}
      {collapsed && (
        <div className="mt-3 rounded-xl border border-dashed border-border bg-background px-3 py-3 text-[11px] text-muted-foreground">
          {labels.empty}
        </div>
      )}
    </div>
  );
}

function MindMapDetails({
  node,
  t,
}: {
  node: MindMapNode | null;
  t: ReturnType<typeof useTranslation>["t"];
}) {
  if (!node) {
    return null;
  }

  return (
    <div className="mt-4 rounded-2xl border border-border bg-card/70 p-4">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-xs font-medium uppercase tracking-[0.14em] text-muted-foreground">
            {t("articleReader.mindMapPanel.selectedNode", "选中节点")}
          </p>
          <p className="mt-1 text-sm font-semibold text-foreground">{node.title}</p>
        </div>
        <span className="rounded-full bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary">
          {node.node_type}
        </span>
      </div>
      <p className="mt-3 text-xs leading-5 text-muted-foreground">{node.summary}</p>
      <div className="mt-4 grid grid-cols-2 gap-3 text-[11px] text-muted-foreground">
        <div className="rounded-xl bg-background px-3 py-2">
          <p>{t("articleReader.mindMapPanel.confidence", "置信度")}</p>
          <p className="mt-1 font-medium text-foreground">{Math.round(node.confidence * 100)}%</p>
        </div>
        <div className="rounded-xl bg-background px-3 py-2">
          <p>{t("articleReader.mindMapPanel.evidenceCount", "证据片段")}</p>
          <p className="mt-1 font-medium text-foreground">{node.source_segment_ids.length}</p>
        </div>
      </div>
    </div>
  );
}

function EditableMindMap({
  result,
  language,
  newNodeLabel,
  panelMode,
  onSelectionChange,
  onDirtyChange,
  onDataChange,
}: {
  result: MindMapResult;
  language: string;
  newNodeLabel: string;
  panelMode: AssistantPanelMode;
  onSelectionChange: (node: MindMapNode | null) => void;
  onDirtyChange: (dirty: boolean) => void;
  onDataChange: (root: MindMapNode) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<MindElixirInstance | null>(null);
  const emitSelectionChange = useEffectEvent(onSelectionChange);
  const emitDirtyChange = useEffectEvent(onDirtyChange);
  const emitDataChange = useEffectEvent(onDataChange);

  useEffect(() => {
    if (!containerRef.current || !result.map) {
      return;
    }

    const editor = new MindElixir({
      el: containerRef.current,
      editable: true,
      direction: MindElixir.SIDE,
      toolBar: true,
      contextMenu: true,
      keypress: true,
      allowUndo: true,
      locale: mapLocale(language),
      newTopicName: newNodeLabel,
      overflowHidden: false,
    });
    const data = toMindElixirData(result);
    const initError = editor.init(data);
    if (initError) {
      throw initError;
    }
    editor.toCenter();

    const syncSelection = (nodes: NodeObj<unknown>[]) => {
      const firstNode = nodes[0] as NodeObj<MindNodeMetadata> | undefined;
      emitSelectionChange(firstNode ? nodeFromMindNode(firstNode) : null);
    };
    const syncDirty = () => {
      const current = editor.getData();
      emitDataChange(nodeFromMindNode(current.nodeData as NodeObj<MindNodeMetadata>));
      emitDirtyChange(true);
    };

    editor.bus.addListener("selectNodes", syncSelection);
    editor.bus.addListener("operation", syncDirty);
    editorRef.current = editor;
    emitSelectionChange(result.map.root);
    emitDirtyChange(false);

    return () => {
      editor.bus.removeListener("selectNodes", syncSelection);
      editor.bus.removeListener("operation", syncDirty);
      editor.destroy();
      editorRef.current = null;
    };
  }, [language, newNodeLabel, result]);

  return (
    <div className="mt-4 rounded-[28px] border border-border bg-card/70 p-3">
      <div className="rounded-[22px] bg-[radial-gradient(circle_at_top,_rgba(236,205,156,0.18),_transparent_58%),linear-gradient(180deg,rgba(255,255,255,0.92),rgba(248,244,238,0.98))] p-3">
        <div
          ref={containerRef}
          data-testid="mind-elixir-canvas"
          className={`overflow-hidden rounded-[18px] border border-border/60 bg-white/70 ${
            panelMode === "compact"
              ? "h-[440px]"
              : panelMode === "wide"
                ? "h-[560px]"
                : "h-[calc(100vh-260px)] min-h-[680px]"
          }`}
        />
      </div>
    </div>
  );
}

export function ArticleMindMapPanel({
  article,
  targetLanguage,
  panelMode = "wide",
}: ArticleMindMapPanelProps) {
  const { t } = useTranslation();
  const [task, setTask] = useState<AgentTask | null>(null);
  const [artifact, setArtifact] = useState<Artifact | null>(null);
  const [result, setResult] = useState<MindMapResult | null>(null);
  const [workerStatus, setWorkerStatus] = useState<AgentWorkerStatusSnapshot | null>(null);
  const [isCreatingTask, setIsCreatingTask] = useState(false);
  const [isLoadingArtifact, setIsLoadingArtifact] = useState(false);
  const [isSavingArtifact, setIsSavingArtifact] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedNode, setSelectedNode] = useState<MindMapNode | null>(null);
  const [isDirty, setIsDirty] = useState(false);
  const [draftResult, setDraftResult] = useState<MindMapResult | null>(null);
  const [showDetails, setShowDetails] = useState(panelMode !== "compact");
  const [showLogs, setShowLogs] = useState(panelMode !== "compact");

  const loadCurrentArtifact = useEffectEvent(async (artifactId: string) => {
    setIsLoadingArtifact(true);
    try {
      const nextArtifact = await invoke<Artifact>("get_artifact_cmd", {
        articleId: article.id,
        artifactId,
      });
      setArtifact(nextArtifact);
      const nextResult = nextArtifact.content as MindMapResult;
      setResult(nextResult);
      setDraftResult(nextResult);
      setSelectedNode(nextResult.map?.root ?? null);
      setIsDirty(false);
    } catch (err) {
      setError(typeof err === "string" ? err : "Failed to load mind map artifact");
    } finally {
      setIsLoadingArtifact(false);
    }
  });

  const applyTaskUpdate = useEffectEvent((nextTask: AgentTask) => {
    if (nextTask.article_id !== article.id) {
      return;
    }

    setTask(nextTask);
    if (nextTask.status === "failed" && nextTask.error) {
      setError(nextTask.error);
    }

    const latestArtifactId = nextTask.artifact_ids[nextTask.artifact_ids.length - 1];
    if (latestArtifactId) {
      void loadCurrentArtifact(latestArtifactId);
    }
  });

  const workerLabels = {
    session: t("articleReader.mindMapPanel.agentSession", "会话"),
    startedAt: t("articleReader.mindMapPanel.agentStartedAt", "启动时间"),
    lastHeartbeat: t("articleReader.mindMapPanel.agentHeartbeat", "最近心跳"),
    empty: t("articleReader.mindMapPanel.agentLogEmpty", "当前还没有日志"),
    showLogs: t("articleReader.mindMapPanel.showLogs", "显示运行日志"),
    hideLogs: t("articleReader.mindMapPanel.hideLogs", "隐藏运行日志"),
    health: {
      healthy: t("articleReader.mindMapPanel.agentHealth.healthy", "正常"),
      starting: t("articleReader.mindMapPanel.agentHealth.starting", "启动中"),
      unhealthy: t("articleReader.mindMapPanel.agentHealth.unhealthy", "异常"),
      stopped: t("articleReader.mindMapPanel.agentHealth.stopped", "未启动"),
    } satisfies Record<AgentWorkerStatusSnapshot["health"], string>,
  };

  useEffect(() => {
    let unlistenTask: UnlistenFn | undefined;
    let unlistenStatus: UnlistenFn | undefined;

    const refreshWorkerStatus = async () => {
      try {
        const snapshot = await invoke<AgentWorkerStatusSnapshot>("get_agent_worker_status_cmd");
        setWorkerStatus(snapshot);
      } catch (err) {
        console.error("Failed to load agent worker status", err);
      }
    };

    const setup = async () => {
      unlistenTask = await listen<AgentTask>("agent-task-updated", (event) => {
        applyTaskUpdate(event.payload);
      });

      unlistenStatus = await listen<AgentWorkerStatusSnapshot>("agent-worker-status", (event) => {
        setWorkerStatus(event.payload);
      });

      try {
        const response = await invoke<{ items?: AgentTask[] }>("assistant_task_list_cmd", {
          query: {
            article_id: article.id,
            limit: 100,
            offset: 0,
          },
        });
        const activeTask = response?.items?.find(
          (candidate) =>
            candidate.task_type === "mind_map_generate"
            && (candidate.status === "queued" || candidate.status === "running"),
        );
        if (activeTask) {
          applyTaskUpdate(activeTask);
        }
      } catch {
        // The task-center API can be unavailable during initial Backend startup.
      }
    };

    setTask(null);
    setArtifact(null);
    setResult(null);
    setDraftResult(null);
    setSelectedNode(null);
    setError(null);

    if (article.active_mind_map_artifact_id) {
      void loadCurrentArtifact(article.active_mind_map_artifact_id);
    }

    void refreshWorkerStatus();
    void setup();

    return () => {
      if (unlistenTask) {
        void unlistenTask();
      }
      if (unlistenStatus) {
        void unlistenStatus();
      }
    };
  }, [article.active_mind_map_artifact_id, article.id]);

  useEffect(() => {
    if (!task || (task.status !== "queued" && task.status !== "running")) {
      return undefined;
    }

    let cancelled = false;
    const refreshTask = async () => {
      try {
        const nextTask = await invoke<AgentTask | null>("get_agent_task_cmd", {
          taskId: task.id,
        });
        if (!cancelled && nextTask) {
          applyTaskUpdate(nextTask);
        }
      } catch {
        // Global task events remain the fast path; the next poll can recover.
      }
    };
    void refreshTask();
    const timer = window.setInterval(() => {
      void refreshTask();
    }, 2_000);

    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [task?.id, task?.status]);

  useEffect(() => {
    setShowDetails(panelMode !== "compact");
    setShowLogs(panelMode !== "compact");
  }, [panelMode]);

  const handleGenerate = async () => {
    setIsCreatingTask(true);
    setError(null);
    setResult(null);
    setArtifact(null);
    try {
      const nextTask = await invoke<AgentTask>("create_mind_map_task_cmd", {
        articleId: article.id,
        displayLanguage: targetLanguage,
        maxDepth: 3,
      });
      setTask(nextTask);
    } catch (err) {
      setError(typeof err === "string" ? err : "Failed to start mind map generation");
    } finally {
      setIsCreatingTask(false);
    }
  };

  const handleDraftRootChange = (root: MindMapNode) => {
    setDraftResult((current) => {
      const base = current ?? result;
      if (!base?.map) {
        return current;
      }

      return {
        ...base,
        map: {
          ...base.map,
          title: root.title,
          summary: root.summary,
          root,
        },
      };
    });
  };

  const handleSaveEdits = async () => {
    if (!draftResult?.map) {
      return;
    }

    setIsSavingArtifact(true);
    setError(null);
    try {
      const savedArtifact = await invoke<Artifact>("artifact_save_cmd", {
        taskId: artifact?.task_id ?? task?.id ?? `manual-edit-${article.id}`,
        articleId: article.id,
        content: draftResult,
      });
      const savedResult = savedArtifact.content as MindMapResult;
      setArtifact(savedArtifact);
      setResult(savedResult);
      setDraftResult(savedResult);
      setSelectedNode(savedResult.map?.root ?? null);
      setIsDirty(false);
    } catch (err) {
      setError(typeof err === "string" ? err : "Failed to save mind map edits");
    } finally {
      setIsSavingArtifact(false);
    }
  };

  const activeProgress =
    task && (task.status === "queued" || task.status === "running") ? task : null;

  const panelTitle = t("articleReader.mindMapPanel.agentStatus", "Agent 状态");
  const nodeCount = useMemo(() => (result?.map ? countNodes(result.map.root) : 0), [result]);
  const showCompactChrome = panelMode === "compact";
  const showFullChrome = panelMode === "full";
  const rootClassName =
    panelMode === "compact"
      ? "h-full overflow-y-auto px-3 py-4"
      : panelMode === "full"
        ? "h-full overflow-y-auto px-6 py-5"
        : "h-full overflow-y-auto px-4 py-5";

  if (isLoadingArtifact) {
    return (
      <div className={rootClassName} data-testid="mind-map-panel-root" data-panel-mode={panelMode}>
        <div className="flex items-center justify-center gap-2 py-8 text-muted-foreground">
          <Loader2 size={16} className="animate-spin" />
          <span>{t("articleReader.mindMapPanel.loading", "正在加载思维导图...")}</span>
        </div>
        <AgentLogPanel
          status={workerStatus}
          title={panelTitle}
          labels={workerLabels}
          collapsed={!showLogs}
          onToggle={() => setShowLogs((current) => !current)}
        />
      </div>
    );
  }

  if (activeProgress) {
    return (
      <div className={rootClassName} data-testid="mind-map-panel-root" data-panel-mode={panelMode}>
        <div className="flex flex-col items-center justify-center gap-4 p-8 text-center">
          <div className="flex h-14 w-14 items-center justify-center rounded-full bg-primary/10 text-primary">
            <Loader2 size={24} className="animate-spin" />
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium text-foreground">
              {activeProgress.message || t("articleReader.mindMapPanel.generating", "正在生成思维导图")}
            </p>
            <p className="text-xs text-muted-foreground">{Math.round(activeProgress.progress * 100)}%</p>
          </div>
        </div>
        <AgentLogPanel
          status={workerStatus}
          title={panelTitle}
          labels={workerLabels}
          collapsed={!showLogs}
          onToggle={() => setShowLogs((current) => !current)}
        />
      </div>
    );
  }

  if (error) {
    return (
      <div className={rootClassName} data-testid="mind-map-panel-root" data-panel-mode={panelMode}>
        <div className="flex flex-col items-center justify-center gap-4 p-8 text-center">
          <div className="flex h-14 w-14 items-center justify-center rounded-full bg-destructive/10 text-destructive">
            <AlertCircle size={24} />
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium text-foreground">
              {t("articleReader.mindMapPanel.failed", "思维导图生成失败")}
            </p>
            <p className="text-xs text-muted-foreground">{error}</p>
          </div>
          <Button onClick={handleGenerate} variant="secondary">
            {t("articleReader.mindMapPanel.retry", "重试")}
          </Button>
        </div>
        <AgentLogPanel
          status={workerStatus}
          title={panelTitle}
          labels={workerLabels}
          collapsed={!showLogs}
          onToggle={() => setShowLogs((current) => !current)}
        />
      </div>
    );
  }

  if (result?.status === "not_applicable") {
    return (
      <div className={rootClassName} data-testid="mind-map-panel-root" data-panel-mode={panelMode}>
        <div className="flex flex-col items-center justify-center gap-4 p-8 text-center">
          <div className="flex h-14 w-14 items-center justify-center rounded-full bg-muted text-muted-foreground">
            <BookOpen size={24} />
          </div>
          <div className="space-y-1">
            <p className="text-sm font-medium text-foreground">
              {t("articleReader.mindMapPanel.notApplicable", "当前内容不适合生成思维导图")}
            </p>
            <p className="text-xs leading-5 text-muted-foreground">
              {result.diagnostics.notes[0] ||
                result.reason ||
                t("articleReader.mindMapPanel.notApplicableHint", "当前素材缺少稳定语义内容。")}
            </p>
          </div>
          <Button onClick={handleGenerate} variant="secondary">
            {t("articleReader.mindMapPanel.generateAgain", "重新生成")}
          </Button>
        </div>
        <AgentLogPanel
          status={workerStatus}
          title={panelTitle}
          labels={workerLabels}
          collapsed={!showLogs}
          onToggle={() => setShowLogs((current) => !current)}
        />
      </div>
    );
  }

  if (result?.map) {
    return (
      <div className={rootClassName} data-testid="mind-map-panel-root" data-panel-mode={panelMode}>
        <div className="rounded-[28px] border border-border bg-[linear-gradient(145deg,rgba(236,227,212,0.65),rgba(249,247,242,0.92))] p-4 shadow-sm">
          <div className="flex items-start justify-between gap-3">
            <div>
              <p className="text-sm font-semibold text-foreground">{result.map.title}</p>
              <p className="mt-1 text-xs leading-5 text-muted-foreground">{result.map.summary}</p>
            </div>
            <div className="flex flex-wrap items-center justify-end gap-2">
              {result.status === "partial" && (
                <span className="rounded-full bg-amber-500/15 px-2 py-1 text-[11px] font-medium text-amber-700">
                  {t("articleReader.mindMapPanel.partial", "部分覆盖")}
                </span>
              )}
              {isDirty && (
                <span className="rounded-full bg-primary/10 px-2 py-1 text-[11px] font-medium text-primary">
                  {t("articleReader.mindMapPanel.unsavedEdits", "本地编辑未保存")}
                </span>
              )}
              <Button
                size="sm"
                variant={isDirty ? "default" : "outline"}
                onClick={handleSaveEdits}
                disabled={!isDirty || isSavingArtifact}
              >
                {isSavingArtifact ? <Loader2 size={14} className="animate-spin" /> : <Save size={14} />}
                <span className="ml-2">
                  {isSavingArtifact
                    ? t("articleReader.mindMapPanel.saving", "保存中...")
                    : t("articleReader.mindMapPanel.save", "保存脑图")}
                </span>
              </Button>
            </div>
          </div>

          <div
            className={`mt-4 grid gap-3 text-[11px] text-muted-foreground ${
              showCompactChrome ? "grid-cols-1" : showFullChrome ? "grid-cols-3" : "grid-cols-3"
            }`}
          >
            <div className="rounded-2xl bg-white/70 px-3 py-3">
              <p>{t("articleReader.mindMapPanel.nodeCount", "节点数")}</p>
              <p className="mt-1 text-sm font-semibold text-foreground">{nodeCount}</p>
            </div>
            <div className="rounded-2xl bg-white/70 px-3 py-3">
              <p>{t("articleReader.mindMapPanel.coverage", "覆盖度")}</p>
              <p className="mt-1 text-sm font-semibold capitalize text-foreground">{result.diagnostics.coverage}</p>
            </div>
            <div className="rounded-2xl bg-white/70 px-3 py-3">
              <p>{t("articleReader.mindMapPanel.evidenceDensity", "证据密度")}</p>
              <p className="mt-1 text-sm font-semibold text-foreground">
                {Math.round(result.diagnostics.evidence_density * 100)}%
              </p>
            </div>
          </div>
        </div>

        <EditableMindMap
          result={result}
          language={targetLanguage}
          newNodeLabel={t("articleReader.mindMapPanel.newNode", "新节点")}
          panelMode={panelMode}
          onSelectionChange={setSelectedNode}
          onDirtyChange={setIsDirty}
          onDataChange={handleDraftRootChange}
        />

        <div className="mt-4 flex flex-wrap items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            className="h-8"
            onClick={() => setShowDetails((current) => !current)}
          >
            {showDetails
              ? t("articleReader.mindMapPanel.hideDetails", "隐藏节点详情")
              : t("articleReader.mindMapPanel.showDetails", "显示节点详情")}
          </Button>
        </div>

        {showDetails && <MindMapDetails node={selectedNode} t={t} />}

        {artifact && (
          <p className="mt-4 text-[11px] text-muted-foreground">
            {t("articleReader.mindMapPanel.artifactId", "产物 ID")}: {artifact.id}
          </p>
        )}

        <AgentLogPanel
          status={workerStatus}
          title={panelTitle}
          labels={workerLabels}
          collapsed={!showLogs}
          onToggle={() => setShowLogs((current) => !current)}
        />
      </div>
    );
  }

  return (
    <div className={rootClassName} data-testid="mind-map-panel-root" data-panel-mode={panelMode}>
      <div className="flex flex-col items-center justify-center gap-4 p-8 text-center">
        <div className="flex h-16 w-16 items-center justify-center rounded-full bg-primary/10 text-primary">
          {isCreatingTask ? <Loader2 size={28} className="animate-spin" /> : <Sparkles size={28} />}
        </div>
        <div className="space-y-1">
          <p className="text-sm font-medium text-foreground">
            {t("articleReader.mindMapPanel.title", "生成文章思维导图")}
          </p>
          <p className="text-xs leading-5 text-muted-foreground">
            {t(
              "articleReader.mindMapPanel.description",
              "从原始内容提取主题结构，并保留与文章的证据关联。",
            )}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <Button onClick={handleGenerate} disabled={isCreatingTask}>
            {t("articleReader.mindMapPanel.generate", "生成思维导图")}
          </Button>
          <Button
            onClick={() => invoke("get_agent_worker_status_cmd").then((snapshot) => setWorkerStatus(snapshot as AgentWorkerStatusSnapshot))}
            variant="ghost"
            size="icon"
            title={t("articleReader.mindMapPanel.refreshStatus", "刷新 Agent 状态")}
          >
            <RefreshCw size={16} />
          </Button>
        </div>
      </div>
      <AgentLogPanel
        status={workerStatus}
        title={panelTitle}
        labels={workerLabels}
        collapsed={!showLogs}
        onToggle={() => setShowLogs((current) => !current)}
      />
    </div>
  );
}
