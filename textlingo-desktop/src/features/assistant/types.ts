import type { SourceLocatorV1 } from "../reader";

export const ASSISTANT_TASK_STATUSES = ["queued", "running", "succeeded", "failed", "cancelled"] as const;
export type AssistantTaskStatus = (typeof ASSISTANT_TASK_STATUSES)[number];

export interface AssistantTask {
  id: string;
  task_type: string;
  status: AssistantTaskStatus;
  article_id: string;
  input: Record<string, unknown>;
  progress: number;
  stage?: string | null;
  message?: string | null;
  error?: string | null;
  worker_session_id?: string | null;
  artifact_ids: string[];
  created_at: string;
  updated_at: string;
  started_at?: string | null;
  finished_at?: string | null;
  retry_of_task_id?: string | null;
  retry_root_task_id?: string | null;
  retry_attempt?: number | null;
  run_summary?: string | null;
}

export interface AssistantTaskLineageEntry {
  task_id: string;
  status: AssistantTaskStatus;
  attempt?: number | null;
  created_at?: string | null;
}

export interface AssistantTaskDetail extends AssistantTask {
  retry_lineage: AssistantTaskLineageEntry[];
}

export interface AssistantTaskTimelineEvent {
  id: string;
  task_id: string;
  event_type: string;
  status?: AssistantTaskStatus | null;
  from_status?: AssistantTaskStatus | null;
  to_status?: AssistantTaskStatus | null;
  stage?: string | null;
  message?: string | null;
  level?: "debug" | "info" | "warn" | "error" | null;
  details: Record<string, unknown>;
  occurred_at: string;
}

export interface AssistantArtifact {
  id: string;
  task_id: string;
  article_id: string;
  artifact_type: string;
  version: string;
  content: unknown;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  file_available?: boolean;
  missing?: boolean;
}

export interface AssistantTaskListQuery {
  status?: AssistantTaskStatus;
  article_id?: string;
  root_task_id?: string;
  limit?: number;
  offset?: number;
}

export interface AssistantTaskListResponse {
  items: AssistantTask[];
  total: number;
  limit?: number;
  offset?: number;
}

export type AssistantSourceReference =
  | {
    target: "source";
    articleId: string;
    locator?: SourceLocatorV1;
    label: string;
  }
  | {
    target: "learning_item";
    articleId?: string;
    learningItemId: string;
    label: string;
  };

export interface AssistantTasksApi {
  list(query?: AssistantTaskListQuery): Promise<AssistantTaskListResponse>;
  detail(taskId: string): Promise<AssistantTaskDetail>;
  timeline(taskId: string): Promise<AssistantTaskTimelineEvent[]>;
  cancel(taskId: string): Promise<AssistantTaskDetail>;
  retry(taskId: string): Promise<AssistantTaskDetail>;
  artifacts(taskId: string): Promise<AssistantArtifact[]>;
}
