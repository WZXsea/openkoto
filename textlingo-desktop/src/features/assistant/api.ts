import { invoke } from "@tauri-apps/api/core";

import {
  ASSISTANT_TASK_STATUSES,
  type AssistantArtifact,
  type AssistantTask,
  type AssistantTaskDetail,
  type AssistantTaskLineageEntry,
  type AssistantTaskListQuery,
  type AssistantTaskListResponse,
  type AssistantTaskStatus,
  type AssistantTaskTimelineEvent,
  type AssistantTasksApi,
} from "./types";

function record(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {};
}

function stringValue(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value : null;
}

function scalarString(value: unknown, fallback: string): string {
  return typeof value === "string" || typeof value === "number" ? String(value) : fallback;
}

function normalizeStatus(value: unknown): AssistantTaskStatus {
  if (ASSISTANT_TASK_STATUSES.includes(value as AssistantTaskStatus)) return value as AssistantTaskStatus;
  return value === "interrupted" ? "failed" : "failed";
}

function normalizeTask(value: unknown): AssistantTask {
  const source = record(value);
  const rawProgress = typeof source.progress === "number" && Number.isFinite(source.progress) ? source.progress : 0;
  return {
    id: stringValue(source.id),
    task_type: stringValue(source.task_type, "unknown"),
    status: normalizeStatus(source.status),
    article_id: stringValue(source.article_id ?? source.material_id),
    input: record(source.input_snapshot ?? source.input),
    progress: Math.min(1, Math.max(0, rawProgress > 1 ? rawProgress / 100 : rawProgress)),
    stage: optionalString(source.stage),
    message: optionalString(source.message),
    error: optionalString(source.error),
    worker_session_id: optionalString(source.worker_session_id),
    artifact_ids: Array.isArray(source.artifact_ids) ? source.artifact_ids.filter((item): item is string => typeof item === "string") : [],
    created_at: stringValue(source.created_at),
    updated_at: stringValue(source.updated_at),
    started_at: optionalString(source.started_at),
    finished_at: optionalString(source.finished_at),
    retry_of_task_id: optionalString(source.retry_of_task_id ?? source.retry_parent_task_id),
    retry_root_task_id: optionalString(source.root_task_id ?? source.retry_root_task_id),
    retry_attempt: typeof source.attempt === "number" ? source.attempt : typeof source.retry_attempt === "number" ? source.retry_attempt : null,
    run_summary: optionalString(source.run_summary ?? source.summary),
  };
}

function normalizeLineage(value: unknown): AssistantTaskLineageEntry[] {
  if (!Array.isArray(value)) return [];
  return value.map((item) => {
    const source = record(item);
    return {
      task_id: stringValue(source.task_id ?? source.id),
      status: normalizeStatus(source.status),
      attempt: typeof source.attempt === "number" ? source.attempt : null,
      created_at: optionalString(source.created_at),
    };
  }).filter((item) => item.task_id);
}

function normalizeDetail(value: unknown): AssistantTaskDetail {
  const source = record(value);
  const taskSource = source.task ?? source;
  return {
    ...normalizeTask(taskSource),
    retry_lineage: normalizeLineage(source.retry_lineage ?? record(taskSource).retry_lineage),
  };
}

function normalizeTimeline(value: unknown): AssistantTaskTimelineEvent[] {
  const values = Array.isArray(value) ? value : Array.isArray(record(value).events) ? record(value).events as unknown[] : [];
  return values.map((item, index) => {
    const source = record(item);
    const metadata = record(source.details ?? source.metadata);
    const rawLevel = source.level ?? metadata.level;
    return {
      id: stringValue(source.id, `event-${index}`),
      task_id: stringValue(source.task_id),
      event_type: stringValue(source.event_type ?? source.type, "update"),
      status: source.to_status || source.status ? normalizeStatus(source.to_status ?? source.status) : null,
      from_status: source.from_status ? normalizeStatus(source.from_status) : null,
      to_status: source.to_status ? normalizeStatus(source.to_status) : null,
      stage: optionalString(source.stage),
      message: optionalString(source.message ?? source.error),
      level: optionalString(source.error)
        ? "error"
        : ["debug", "info", "warn", "error"].includes(String(rawLevel)) ? rawLevel as AssistantTaskTimelineEvent["level"] : null,
      details: metadata,
      occurred_at: stringValue(source.occurred_at ?? source.timestamp ?? source.created_at),
    };
  });
}

function normalizeArtifacts(value: unknown): AssistantArtifact[] {
  const values = Array.isArray(value) ? value : Array.isArray(record(value).items) ? record(value).items as unknown[] : [];
  return values.map((item) => {
    const source = record(item);
    const metadata = record(source.metadata);
    return {
      id: stringValue(source.id),
      task_id: stringValue(source.task_id),
      article_id: stringValue(source.article_id ?? source.material_id),
      artifact_type: stringValue(source.artifact_type, "unknown"),
      version: scalarString(source.output_version ?? source.version, "1"),
      content: source.content ?? null,
      metadata,
      created_at: stringValue(source.created_at),
      updated_at: stringValue(source.updated_at),
      file_available: typeof source.file_available === "boolean"
        ? source.file_available
        : typeof metadata.file_available === "boolean" ? metadata.file_available : undefined,
      missing: typeof source.missing === "boolean"
        ? source.missing
        : typeof metadata.missing === "boolean" ? metadata.missing : undefined,
    };
  }).filter((artifact) => artifact.id);
}

async function listTasks(query: AssistantTaskListQuery = {}): Promise<AssistantTaskListResponse> {
  const response = await invoke<unknown>("assistant_task_list_cmd", { query });
  const source = record(response);
  const values = Array.isArray(response) ? response : Array.isArray(source.items) ? source.items : Array.isArray(source.tasks) ? source.tasks : [];
  const items = values.map(normalizeTask).filter((task) => task.id);
  return {
    items,
    total: typeof source.total === "number" ? source.total : items.length,
    limit: typeof source.limit === "number" ? source.limit : undefined,
    offset: typeof source.offset === "number" ? source.offset : undefined,
  };
}

export function createAssistantTasksApi(): AssistantTasksApi {
  return {
    list: listTasks,
    detail: async (taskId) => {
      const detail = normalizeDetail(await invoke<unknown>("assistant_task_detail_cmd", { taskId }));
      if (detail.retry_lineage.length > 0) return detail;
      const rootTaskId = detail.retry_root_task_id || detail.id;
      try {
        const lineageResponse = await listTasks({ root_task_id: rootTaskId, limit: 200, offset: 0 });
        const retryLineage = lineageResponse.items
          .filter((task) => task.id === rootTaskId || task.retry_root_task_id === rootTaskId)
          .sort((left, right) => (left.retry_attempt ?? 0) - (right.retry_attempt ?? 0))
          .map((task) => ({
            task_id: task.id,
            status: task.status,
            attempt: task.retry_attempt,
            created_at: task.created_at,
          }));
        return { ...detail, retry_lineage: retryLineage };
      } catch {
        return detail;
      }
    },
    timeline: async (taskId) => normalizeTimeline(await invoke<unknown>("assistant_task_timeline_cmd", { taskId })),
    cancel: async (taskId) => normalizeDetail(await invoke<unknown>("assistant_task_cancel_cmd", { taskId })),
    retry: async (taskId) => normalizeDetail(await invoke<unknown>("assistant_task_retry_cmd", { taskId })),
    artifacts: async (taskId) => normalizeArtifacts(await invoke<unknown>("assistant_task_artifacts_cmd", { taskId })),
  };
}

export const assistantTasksApi = createAssistantTasksApi();
