import { AlertCircle, Ban, Bot, Clock3, FileInput, Loader2, RefreshCw, RotateCcw, Route, XCircle } from "lucide-react";
import { useMemo } from "react";

import {
  ASSISTANT_TASK_STATUSES,
  useAssistantTaskCenter,
  type AssistantSourceReference,
  type AssistantTask,
  type AssistantTaskStatus,
  type AssistantTasksApi,
} from "../../features/assistant";
import type { Article } from "../../types";
import { formatDate } from "../../lib/utils";
import { Button } from "../ui/button";
import { Select } from "../ui/select";
import { AssistantArtifactViewer } from "./AssistantArtifactViewer";

const STATUS_LABELS: Record<AssistantTaskStatus, string> = {
  queued: "排队中",
  running: "运行中",
  succeeded: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

const STATUS_STYLES: Record<AssistantTaskStatus, string> = {
  queued: "bg-sky-500/10 text-sky-700 dark:text-sky-300",
  running: "bg-primary/10 text-primary",
  succeeded: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300",
  failed: "bg-destructive/10 text-destructive",
  cancelled: "bg-muted text-muted-foreground",
};

const TASK_TYPE_LABELS: Record<string, string> = {
  mind_map_generate: "生成思维导图",
  assistant_agent_turn: "Assistant 任务",
  article_ask: "文章问答",
  structured_report: "结构化报告",
  ppt_outline: "幻灯片大纲",
  ppt_slides: "幻灯片生成",
};

interface AssistantTaskCenterProps {
  api?: AssistantTasksApi;
  articles: Article[];
  initialArticleId?: string;
  mode?: "page" | "reader";
  onNavigateSource: (reference: AssistantSourceReference) => void;
}

function taskTitle(task: AssistantTask): string {
  return TASK_TYPE_LABELS[task.task_type] || task.task_type || "Assistant 任务";
}

function isActiveTask(status: AssistantTaskStatus): status is "queued" | "running" {
  return status === "queued" || status === "running";
}

function progressPercentage(progress: number): number {
  return Math.round(Math.min(1, Math.max(0, progress)) * 100);
}

function TaskStatusBadge({ status, announce = false }: { status: AssistantTaskStatus; announce?: boolean }) {
  const isActive = isActiveTask(status);
  return (
    <span
      role={announce ? "status" : undefined}
      aria-live={announce ? "polite" : undefined}
      aria-label={announce ? STATUS_LABELS[status] : undefined}
      className={`inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-0.5 text-[11px] ${STATUS_STYLES[status]}`}
    >
      {isActive && (
        <Loader2
          aria-hidden="true"
          size={11}
          className="motion-safe:animate-spin motion-reduce:animate-none"
          data-testid="assistant-active-status-indicator"
        />
      )}
      <span>{STATUS_LABELS[status]}</span>
    </span>
  );
}

function TaskProgress({
  progress,
  status,
  label,
}: {
  progress: number;
  status: AssistantTaskStatus;
  label: string;
}) {
  const percentage = progressPercentage(progress);
  const isActive = isActiveTask(status);
  return (
    <span
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={percentage}
      aria-valuetext={`${STATUS_LABELS[status]}，已完成 ${percentage}%`}
      className="relative block h-1 overflow-hidden rounded-full bg-muted"
    >
      <span
        className="block h-full rounded-full bg-primary transition-[width] duration-300 motion-reduce:transition-none"
        style={{ width: `${percentage}%` }}
      />
      {isActive && (
        <span
          aria-hidden="true"
          className="absolute inset-0 bg-gradient-to-r from-transparent via-primary/25 to-transparent motion-safe:animate-pulse motion-reduce:hidden"
          data-testid="assistant-active-progress-indicator"
        />
      )}
    </span>
  );
}

function durationLabel(start?: string | null, end?: string | null): string | null {
  if (!start) return null;
  const startMs = Date.parse(start);
  const endMs = end ? Date.parse(end) : Date.now();
  if (!Number.isFinite(startMs) || !Number.isFinite(endMs) || endMs < startMs) return null;
  const seconds = Math.round((endMs - startMs) / 1000);
  return seconds < 60 ? `${seconds} 秒` : `${Math.floor(seconds / 60)} 分 ${seconds % 60} 秒`;
}

export function AssistantTaskCenter({
  api,
  articles,
  initialArticleId,
  mode = "page",
  onNavigateSource,
}: AssistantTaskCenterProps) {
  const state = useAssistantTaskCenter({ api, initialArticleId });
  const articleTitles = useMemo(() => new Map(articles.map((article) => [article.id, article.title])), [articles]);
  const detail = state.detail;
  const layoutClass = mode === "reader"
    ? "flex h-full min-h-0 flex-col"
    : "grid h-full min-h-0 grid-rows-[minmax(220px,40%)_minmax(0,1fr)] lg:grid-cols-[360px_minmax(0,1fr)] lg:grid-rows-1";

  return (
    <section className={layoutClass} aria-label="Assistant 任务中心" data-testid="assistant-task-center" data-mode={mode}>
      <div className={`flex min-h-0 flex-col border-border bg-card/35 ${mode === "reader" ? "h-[42%] shrink-0 border-b" : "h-full border-r"}`}>
        <header className="shrink-0 border-b border-border p-4">
          <div className="flex items-center justify-between gap-3">
            <div><h1 className={mode === "reader" ? "text-sm font-semibold" : "text-xl font-semibold"}>Assistant 任务</h1><p className="mt-1 text-xs text-muted-foreground">{initialArticleId ? "当前素材的工作流" : "查看运行状态、失败原因和产物"}</p></div>
            <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => void state.refresh()} aria-label="刷新 Assistant 任务"><RefreshCw size={15} /></Button>
          </div>
          <label className="mt-3 block text-xs text-muted-foreground">
            <span className="sr-only">筛选任务状态</span>
            <Select aria-label="筛选任务状态" value={state.status} onChange={(event) => state.setStatus(event.target.value as AssistantTaskStatus | "all")}>
              <option value="all">全部状态</option>
              {ASSISTANT_TASK_STATUSES.map((status) => <option key={status} value={status}>{STATUS_LABELS[status]}</option>)}
            </Select>
          </label>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto p-2" role="list" aria-label="Assistant 任务列表">
          {state.isListLoading ? <div className="flex items-center justify-center gap-2 py-10 text-sm text-muted-foreground" role="status"><Loader2 size={16} className="animate-spin" />正在加载任务</div>
            : state.listError ? <div className="m-2 rounded-xl border border-destructive/25 bg-destructive/10 p-3 text-sm text-destructive" role="alert"><p>无法连接任务服务</p><p className="mt-1 break-words text-xs opacity-80">{state.listError}</p><Button variant="outline" size="sm" className="mt-3" onClick={() => void state.refresh()}>重试</Button></div>
              : state.tasks.length === 0 ? <div className="flex flex-col items-center justify-center px-4 py-10 text-center text-sm text-muted-foreground"><Bot size={24} className="mb-3 opacity-60" /><p>当前没有任务</p><p className="mt-1 text-xs">在阅读器中发起 Assistant 任务后会显示在这里。</p></div>
                : state.tasks.map((task) => {
                  const selected = state.selectedTaskId === task.id;
                  return (
                    <div key={task.id} role="listitem" className="mb-1.5">
                    <button type="button" onClick={() => state.setSelectedTaskId(task.id)} className={`w-full rounded-xl border p-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring ${selected ? "border-primary/35 bg-primary/5" : "border-transparent hover:border-border hover:bg-muted/35"}`}>
                      <span className="flex items-start justify-between gap-2"><span className="min-w-0"><span className="block truncate text-sm font-medium">{taskTitle(task)}</span><span className="mt-0.5 block truncate text-xs text-muted-foreground">{articleTitles.get(task.article_id) || task.article_id || "未关联素材"}</span></span><TaskStatusBadge status={task.status} /></span>
                      {isActiveTask(task.status) && <span className="mt-3 block"><TaskProgress progress={task.progress} status={task.status} label={`${taskTitle(task)}列表进度`} /></span>}
                      <span className="mt-2 block text-[11px] text-muted-foreground">{formatDate(task.updated_at)}</span>
                    </button>
                    </div>
                  );
                })}
        </div>
      </div>

      <div className="min-h-0 overflow-y-auto bg-background p-4 sm:p-5">
        {state.isDetailLoading ? <div className="flex h-full min-h-48 items-center justify-center gap-2 text-sm text-muted-foreground" role="status"><Loader2 size={17} className="animate-spin" />正在加载任务详情</div>
          : state.detailError ? <div className="rounded-2xl border border-destructive/25 bg-destructive/10 p-4 text-sm text-destructive" role="alert"><p className="font-medium">任务详情加载失败</p><p className="mt-1 break-words text-xs">{state.detailError}</p>{state.selectedTaskId && <Button variant="outline" size="sm" className="mt-3" onClick={() => void state.refresh()}>重试</Button>}</div>
            : !detail ? <div className="flex h-full min-h-48 flex-col items-center justify-center text-center text-muted-foreground"><Route size={26} className="mb-3 opacity-60" /><p className="text-sm">选择一个任务查看详情</p></div>
              : (
                <div className="mx-auto max-w-4xl space-y-5" data-testid={`assistant-task-detail-${detail.id}`}>
                  <header className="flex flex-wrap items-start justify-between gap-4">
                    <div className="min-w-0"><div className="mb-2 flex flex-wrap items-center gap-2"><TaskStatusBadge status={detail.status} announce />{detail.retry_attempt != null && <span className="rounded-full border border-border px-2.5 py-1 text-xs text-muted-foreground">尝试 {detail.retry_attempt}</span>}</div><h2 className="break-words text-xl font-semibold">{taskTitle(detail)}</h2><p className="mt-1 break-words text-sm text-muted-foreground">{articleTitles.get(detail.article_id) || detail.article_id}</p></div>
                    <div className="flex gap-2">
                      {["queued", "running"].includes(detail.status) && <Button variant="outline" size="sm" onClick={() => void state.cancelSelected()} disabled={state.pendingAction !== null} className="gap-1.5">{state.pendingAction === "cancel" ? <Loader2 aria-hidden="true" size={15} className="motion-safe:animate-spin motion-reduce:animate-none" /> : <XCircle size={15} />}{state.pendingAction === "cancel" ? "取消中" : "取消任务"}</Button>}
                      {["failed", "cancelled"].includes(detail.status) && <Button size="sm" onClick={() => void state.retrySelected()} disabled={state.pendingAction !== null} className="gap-1.5">{state.pendingAction === "retry" ? <Loader2 aria-hidden="true" size={15} className="motion-safe:animate-spin motion-reduce:animate-none" /> : <RotateCcw size={15} />}{state.pendingAction === "retry" ? "重试中" : "重试"}</Button>}
                    </div>
                  </header>

                  <section className="grid gap-3 sm:grid-cols-3" aria-label="运行摘要">
                    <div className="rounded-xl border border-border bg-card p-3"><p className="text-xs text-muted-foreground">进度</p><p className="mt-1 text-lg font-semibold tabular-nums">{progressPercentage(detail.progress)}%</p><span className="mt-2 block"><TaskProgress progress={detail.progress} status={detail.status} label={`${taskTitle(detail)}详情进度`} /></span></div>
                    <div className="rounded-xl border border-border bg-card p-3"><p className="text-xs text-muted-foreground">运行时长</p><p className="mt-1 text-lg font-semibold">{durationLabel(detail.started_at, detail.finished_at) || "—"}</p></div>
                    <div className="rounded-xl border border-border bg-card p-3"><p className="text-xs text-muted-foreground">产物</p><p className="mt-1 text-lg font-semibold tabular-nums">{state.artifacts.length}</p></div>
                  </section>

                  {(detail.run_summary || detail.message || detail.stage) && <section className="rounded-2xl border border-border bg-card p-4"><h3 className="text-sm font-semibold">运行摘要</h3><p className="mt-2 whitespace-pre-wrap break-words text-sm leading-6 text-muted-foreground">{detail.run_summary || detail.message || detail.stage}</p></section>}
                  {detail.error && <section className="flex gap-3 rounded-2xl border border-destructive/25 bg-destructive/10 p-4 text-destructive" role="alert"><AlertCircle size={18} className="mt-0.5 shrink-0" /><div><h3 className="text-sm font-semibold">失败原因</h3><p className="mt-1 whitespace-pre-wrap break-words text-sm">{detail.error}</p></div></section>}

                  {state.sourceReferences.length > 0 && <section className="rounded-2xl border border-border bg-card p-4"><h3 className="text-sm font-semibold">来源证据</h3><div className="mt-3 flex flex-wrap gap-2">{state.sourceReferences.map((reference, index) => <Button key={`${reference.target}-${reference.target === "source" ? reference.articleId : reference.learningItemId}-${index}`} variant="outline" size="sm" className="gap-1.5" onClick={() => onNavigateSource(reference)}><Route size={14} />{reference.label}</Button>)}</div></section>}

                  <details className="rounded-2xl border border-border bg-card p-4"><summary className="cursor-pointer text-sm font-semibold focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"><FileInput size={15} className="mr-2 inline" />输入快照</summary><pre className="mt-3 max-h-80 overflow-auto whitespace-pre-wrap break-words rounded-xl bg-muted/45 p-3 text-xs">{JSON.stringify(detail.input, null, 2)}</pre></details>

                  {detail.retry_lineage.length > 0 && <section className="rounded-2xl border border-border bg-card p-4"><h3 className="text-sm font-semibold">重试链路</h3><div className="mt-3 flex flex-wrap items-center gap-2">{detail.retry_lineage.map((entry, index) => <span key={entry.task_id} className="flex items-center gap-2"><button type="button" onClick={() => state.setSelectedTaskId(entry.task_id)} className="rounded-lg border border-border px-2.5 py-1.5 text-xs hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">尝试 {entry.attempt != null ? entry.attempt : index + 1} · {STATUS_LABELS[entry.status]}</button>{index < detail.retry_lineage.length - 1 && <span className="text-muted-foreground">→</span>}</span>)}</div></section>}

                  <section className="rounded-2xl border border-border bg-card p-4">
                    <h3 className="text-sm font-semibold">运行时间线</h3>
                    {state.isTimelineLoading ? (
                      <p className="mt-3 flex items-center gap-2 text-sm text-muted-foreground" role="status">
                        <Loader2 size={14} className="animate-spin" />正在重新加载时间线
                      </p>
                    ) : state.timelineError ? (
                      <div className="mt-3 text-sm text-destructive" role="alert">
                        <p>时间线暂不可用：{state.timelineError}</p>
                        <Button variant="outline" size="sm" className="mt-3" onClick={() => void state.refreshTimeline()}>
                          重试时间线
                        </Button>
                      </div>
                    ) : state.timeline.length === 0 ? (
                      <p className="mt-3 text-sm text-muted-foreground">暂无时间线记录</p>
                    ) : (
                      <ol className="mt-4 space-y-4">
                        {state.timeline.map((event) => (
                          <li key={event.id} className="relative border-l border-border pl-4">
                            <span className={`absolute -left-1.5 top-1 h-3 w-3 rounded-full border-2 border-background ${event.level === "error" ? "bg-destructive" : "bg-primary"}`} />
                            <div className="flex flex-wrap items-center gap-2">
                              <span className="text-sm font-medium">{event.stage || event.event_type}</span>
                              {event.from_status && event.to_status && <span className="text-xs text-muted-foreground">{STATUS_LABELS[event.from_status]} → {STATUS_LABELS[event.to_status]}</span>}
                              <time className="ml-auto text-[11px] text-muted-foreground"><Clock3 size={11} className="mr-1 inline" />{formatDate(event.occurred_at)}</time>
                            </div>
                            {event.message && <p className="mt-1 whitespace-pre-wrap break-words text-sm text-muted-foreground">{event.message}</p>}
                          </li>
                        ))}
                      </ol>
                    )}
                  </section>

                  <section>
                    <h3 className="mb-3 text-sm font-semibold">任务产物</h3>
                    {state.isArtifactsLoading ? (
                      <div className="flex items-center gap-2 rounded-2xl border border-border p-4 text-sm text-muted-foreground" role="status">
                        <Loader2 size={14} className="animate-spin" />正在重新加载任务产物
                      </div>
                    ) : state.artifactsError ? (
                      <div className="rounded-2xl border border-amber-500/25 bg-amber-500/10 p-4 text-sm text-amber-700 dark:text-amber-300" role="alert">
                        <div className="flex items-start gap-2"><AlertCircle size={16} className="mt-0.5 shrink-0" /><span>任务产物不可用：{state.artifactsError}</span></div>
                        <Button variant="outline" size="sm" className="mt-3" onClick={() => void state.refreshArtifacts()}>
                          重试产物
                        </Button>
                      </div>
                    ) : state.artifacts.length === 0 ? (
                      <div className="flex items-center gap-2 rounded-2xl border border-dashed border-border p-4 text-sm text-muted-foreground"><Ban size={16} />当前任务没有可查看的产物</div>
                    ) : (
                      <div className="space-y-3">{state.artifacts.map((artifact) => <AssistantArtifactViewer key={artifact.id} artifact={artifact} />)}</div>
                    )}
                  </section>
                </div>
              )}
      </div>
    </section>
  );
}
