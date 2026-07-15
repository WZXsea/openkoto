import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { parseSourceLocator } from "../reader";
import { assistantTasksApi } from "./api";
import type {
  AssistantArtifact,
  AssistantSourceReference,
  AssistantTask,
  AssistantTaskDetail,
  AssistantTaskStatus,
  AssistantTaskTimelineEvent,
  AssistantTasksApi,
} from "./types";

function asRecord(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  const source = asRecord(error);
  if (typeof source.message === "string") return source.message;
  if (typeof source.code === "string") return source.code;
  return String(error);
}

function referencesFrom(value: unknown, fallbackArticleId: string, fallbackLabel: string): AssistantSourceReference[] {
  const source = asRecord(value);
  const articleId = typeof source.article_id === "string"
    ? source.article_id
    : typeof source.material_id === "string" ? source.material_id : fallbackArticleId;
  const locator = parseSourceLocator(source.source_locator ?? source.locator);
  const learningItemId = typeof source.learning_item_id === "string" ? source.learning_item_id : undefined;
  const references: AssistantSourceReference[] = [];
  if (articleId && (locator || source.article_id || source.material_id)) {
    references.push({ target: "source", articleId, locator, label: `${fallbackLabel} · 原文` });
  }
  if (learningItemId) {
    references.push({
      target: "learning_item",
      ...(articleId ? { articleId } : {}),
      learningItemId,
      label: `${fallbackLabel} · 学习项`,
    });
  }
  return references;
}

export function collectAssistantSourceReferences(task: AssistantTaskDetail, artifacts: AssistantArtifact[]): AssistantSourceReference[] {
  const candidates: AssistantSourceReference[] = [];
  candidates.push(...referencesFrom(task.input, task.article_id, "任务输入来源"));
  artifacts.forEach((artifact, index) => {
    candidates.push(...referencesFrom(artifact.metadata, artifact.article_id || task.article_id, `产物 ${index + 1} 来源`));
    candidates.push(...referencesFrom(artifact.content, artifact.article_id || task.article_id, `产物 ${index + 1} 证据`));
  });
  const seen = new Set<string>();
  return candidates.filter((reference) => {
    const key = reference.target === "source"
      ? `source:${reference.articleId}:${JSON.stringify(reference.locator || null)}`
      : `learning_item:${reference.learningItemId}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

interface UseAssistantTaskCenterOptions {
  api?: AssistantTasksApi;
  initialArticleId?: string;
  refreshIntervalMs?: number;
}

export function useAssistantTaskCenter({
  api = assistantTasksApi,
  initialArticleId,
  refreshIntervalMs = 4_000,
}: UseAssistantTaskCenterOptions = {}) {
  const [status, setStatus] = useState<AssistantTaskStatus | "all">("all");
  const [tasks, setTasks] = useState<AssistantTask[]>([]);
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [detail, setDetail] = useState<AssistantTaskDetail | null>(null);
  const [timeline, setTimeline] = useState<AssistantTaskTimelineEvent[]>([]);
  const [artifacts, setArtifacts] = useState<AssistantArtifact[]>([]);
  const [isListLoading, setIsListLoading] = useState(true);
  const [isDetailLoading, setIsDetailLoading] = useState(false);
  const [isTimelineLoading, setIsTimelineLoading] = useState(false);
  const [isArtifactsLoading, setIsArtifactsLoading] = useState(false);
  const [pendingAction, setPendingAction] = useState<"cancel" | "retry" | null>(null);
  const [listError, setListError] = useState<string | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [timelineError, setTimelineError] = useState<string | null>(null);
  const [artifactsError, setArtifactsError] = useState<string | null>(null);
  const listRequest = useRef(0);
  const detailRequest = useRef(0);
  const timelineRequest = useRef(0);
  const artifactsRequest = useRef(0);

  const loadList = useCallback(async (quiet = false) => {
    const requestId = ++listRequest.current;
    if (!quiet) setIsListLoading(true);
    setListError(null);
    try {
      const response = await api.list({
        status: status === "all" ? undefined : status,
        article_id: initialArticleId,
        limit: 100,
        offset: 0,
      });
      if (requestId !== listRequest.current) return;
      setTasks(response.items);
      setSelectedTaskId((current) => current && response.items.some((task) => task.id === current)
        ? current
        : response.items[0]?.id ?? null);
    } catch (error) {
      if (requestId === listRequest.current) setListError(errorMessage(error));
    } finally {
      if (!quiet && requestId === listRequest.current) setIsListLoading(false);
    }
  }, [api, initialArticleId, status]);

  const loadDetail = useCallback(async (taskId: string, quiet = false) => {
    const requestId = ++detailRequest.current;
    // A task switch supersedes any section-only retry from the previous task.
    ++timelineRequest.current;
    ++artifactsRequest.current;
    setIsTimelineLoading(false);
    setIsArtifactsLoading(false);
    if (!quiet) setIsDetailLoading(true);
    setDetailError(null);
    setTimelineError(null);
    setArtifactsError(null);
    try {
      const [detailOutcome, timelineOutcome, artifactsOutcome] = await Promise.allSettled([
        api.detail(taskId),
        api.timeline(taskId),
        api.artifacts(taskId),
      ]);
      if (requestId !== detailRequest.current) return;
      if (detailOutcome.status === "rejected") throw detailOutcome.reason;
      setDetail(detailOutcome.value);
      if (timelineOutcome.status === "fulfilled") setTimeline(timelineOutcome.value);
      else {
        setTimeline([]);
        setTimelineError(errorMessage(timelineOutcome.reason));
      }
      if (artifactsOutcome.status === "fulfilled") setArtifacts(artifactsOutcome.value);
      else {
        setArtifacts([]);
        setArtifactsError(errorMessage(artifactsOutcome.reason));
      }
    } catch (error) {
      if (requestId === detailRequest.current) {
        setDetail(null);
        setTimeline([]);
        setArtifacts([]);
        setDetailError(errorMessage(error));
      }
    } finally {
      if (!quiet && requestId === detailRequest.current) setIsDetailLoading(false);
    }
  }, [api]);

  const loadTimeline = useCallback(async (taskId: string) => {
    const requestId = ++timelineRequest.current;
    setIsTimelineLoading(true);
    setTimelineError(null);
    try {
      const nextTimeline = await api.timeline(taskId);
      if (requestId === timelineRequest.current) setTimeline(nextTimeline);
    } catch (error) {
      if (requestId === timelineRequest.current) setTimelineError(errorMessage(error));
    } finally {
      if (requestId === timelineRequest.current) setIsTimelineLoading(false);
    }
  }, [api]);

  const loadArtifacts = useCallback(async (taskId: string) => {
    const requestId = ++artifactsRequest.current;
    setIsArtifactsLoading(true);
    setArtifactsError(null);
    try {
      const nextArtifacts = await api.artifacts(taskId);
      if (requestId === artifactsRequest.current) setArtifacts(nextArtifacts);
    } catch (error) {
      if (requestId === artifactsRequest.current) setArtifactsError(errorMessage(error));
    } finally {
      if (requestId === artifactsRequest.current) setIsArtifactsLoading(false);
    }
  }, [api]);

  useEffect(() => { void loadList(); }, [loadList]);
  useEffect(() => {
    if (selectedTaskId) void loadDetail(selectedTaskId);
    else {
      ++timelineRequest.current;
      ++artifactsRequest.current;
      setIsTimelineLoading(false);
      setIsArtifactsLoading(false);
      setDetail(null);
      setTimeline([]);
      setArtifacts([]);
      setTimelineError(null);
      setArtifactsError(null);
    }
  }, [loadDetail, selectedTaskId]);

  const hasActiveTasks = tasks.some((task) => task.status === "queued" || task.status === "running");
  useEffect(() => {
    if (!hasActiveTasks || refreshIntervalMs <= 0) return undefined;
    const timer = window.setInterval(() => {
      void loadList(true);
      if (selectedTaskId) void loadDetail(selectedTaskId, true);
    }, refreshIntervalMs);
    return () => window.clearInterval(timer);
  }, [hasActiveTasks, loadDetail, loadList, refreshIntervalMs, selectedTaskId]);

  const cancelSelected = useCallback(async () => {
    if (!detail || !["queued", "running"].includes(detail.status)) return;
    setPendingAction("cancel");
    setDetailError(null);
    try {
      const cancelled = await api.cancel(detail.id);
      setDetail(cancelled);
      await loadList(true);
      await loadDetail(cancelled.id, true);
    } catch (error) {
      setDetailError(errorMessage(error));
    } finally {
      setPendingAction(null);
    }
  }, [api, detail, loadDetail, loadList]);

  const retrySelected = useCallback(async () => {
    if (!detail || !["failed", "cancelled"].includes(detail.status)) return;
    setPendingAction("retry");
    setDetailError(null);
    try {
      const retried = await api.retry(detail.id);
      setSelectedTaskId(retried.id);
      setDetail(retried);
      setTimeline([]);
      setArtifacts([]);
      if (status === "all") await loadList(true);
      else setStatus("all");
    } catch (error) {
      setDetailError(errorMessage(error));
    } finally {
      setPendingAction(null);
    }
  }, [api, detail, loadList, status]);

  const sourceReferences = useMemo(
    () => detail ? collectAssistantSourceReferences(detail, artifacts) : [],
    [artifacts, detail],
  );

  return {
    status,
    setStatus,
    tasks,
    selectedTaskId,
    setSelectedTaskId,
    detail,
    timeline,
    artifacts,
    sourceReferences,
    isListLoading,
    isDetailLoading,
    isTimelineLoading,
    isArtifactsLoading,
    pendingAction,
    listError,
    detailError,
    timelineError,
    artifactsError,
    refresh: () => Promise.all([loadList(), selectedTaskId ? loadDetail(selectedTaskId) : Promise.resolve()]),
    refreshTimeline: () => selectedTaskId ? loadTimeline(selectedTaskId) : Promise.resolve(),
    refreshArtifacts: () => selectedTaskId ? loadArtifacts(selectedTaskId) : Promise.resolve(),
    cancelSelected,
    retrySelected,
  };
}
