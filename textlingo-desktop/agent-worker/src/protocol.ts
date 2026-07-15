import { z } from "zod";

export const runtimeProviderSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("openai_compatible"),
    provider: z.string().min(1),
    model: z.string().min(1),
    api_key: z.string().optional(),
    baseUrl: z.string().min(1),
  }),
  z.object({
    kind: z.literal("native_google"),
    provider: z.string().min(1),
    model: z.string().min(1),
    api_key: z.string().min(1),
  }),
  z.object({
    kind: z.literal("native_anthropic"),
    provider: z.string().min(1),
    model: z.string().min(1),
    api_key: z.string().min(1),
  }),
  z.object({
    kind: z.literal("unsupported"),
    provider: z.string().min(1),
    reason: z.string().min(1),
  }),
]);

export type RuntimeProvider = z.infer<typeof runtimeProviderSchema>;

export const materialSummarySchema = z.object({
  id: z.string().min(1),
  title: z.string().min(1),
  material_type: z.string().min(1),
  created_at: z.string().min(1),
  translated: z.boolean(),
});

export type MaterialSummary = z.infer<typeof materialSummarySchema>;

export const articleSnapshotSchema = z.object({
  title: z.string().min(1),
  content: z.string(),
  source_type: z.string().min(1).nullable().optional(),
});

export type ArticleSnapshot = z.infer<typeof articleSnapshotSchema>;

export const agentRunInputSchema = z.object({
  article_id: z.string().min(1),
  display_language: z.string().min(1).default("zh-CN"),
  max_depth: z.number().int().positive().default(3),
  mode: z.enum(["fast", "balanced", "deep"]).default("balanced"),
  article_snapshot: articleSnapshotSchema,
});

export type AgentRunInput = z.infer<typeof agentRunInputSchema>;

export const assistantConversationMessageSchema = z.object({
  role: z.enum(["user", "assistant"]),
  content: z.string().min(1),
});

export const assistantRunInputSchema = z.object({
  user_message: z.string().min(1),
  conversation: z.array(assistantConversationMessageSchema).default([]),
  ui_context: z.object({
    current_article_id: z.string().min(1).optional(),
    display_language: z.string().min(1).default("zh-CN"),
  }),
  current_material: materialSummarySchema.nullable().optional(),
  available_materials: z.array(materialSummarySchema).default([]),
});

export type AssistantRunInput = z.infer<typeof assistantRunInputSchema>;

export const agentRunRequestSchema = z.object({
  id: z.string().min(1),
  type: z.literal("request"),
  method: z.literal("agent.run"),
  params: z.discriminatedUnion("task_type", [
    z.object({
      task_id: z.string().min(1),
      task_type: z.literal("mind_map.generate"),
      provider_config: runtimeProviderSchema,
      input: agentRunInputSchema,
    }),
    z.object({
      task_id: z.string().min(1),
      task_type: z.literal("assistant.agent_turn"),
      provider_config: runtimeProviderSchema,
      input: assistantRunInputSchema,
    }),
  ]),
});

export type AgentRunRequest = z.infer<typeof agentRunRequestSchema>;

export function parseAgentRunRequest(raw: string): AgentRunRequest {
  return agentRunRequestSchema.parse(JSON.parse(raw));
}

export const agentCancelRequestSchema = z.object({
  id: z.string().min(1),
  type: z.literal("request"),
  method: z.literal("agent.cancel"),
  params: z.object({
    task_id: z.string().min(1),
  }),
});

export type AgentCancelRequest = z.infer<typeof agentCancelRequestSchema>;

export function parseAgentCancelRequest(raw: string): AgentCancelRequest {
  return agentCancelRequestSchema.parse(JSON.parse(raw));
}

const timestampSchema = z.string().datetime({ offset: true }).or(z.string().min(1));

const workerReadyEventSchema = z.object({
  type: z.literal("event"),
  event: z.literal("worker.ready"),
  payload: z.object({
    worker_session_id: z.string().min(1),
    timestamp: timestampSchema,
    runtime: z.string().min(1),
    version: z.string().min(1),
  }),
});

const workerHeartbeatEventSchema = z.object({
  type: z.literal("event"),
  event: z.literal("worker.heartbeat"),
  payload: z.object({
    worker_session_id: z.string().min(1),
    timestamp: timestampSchema,
  }),
});

const taskStartedEventSchema = z.object({
  type: z.literal("event"),
  event: z.literal("task.started"),
  payload: z.object({
    task_id: z.string().min(1),
    task_type: z.string().min(1),
    timestamp: timestampSchema,
  }),
});

const taskProgressEventSchema = z.object({
  type: z.literal("event"),
  event: z.literal("task.progress"),
  payload: z.object({
    task_id: z.string().min(1),
    stage: z.string().min(1),
    progress: z.number(),
    message: z.string().optional(),
    timestamp: timestampSchema,
  }),
});

const taskLogEventSchema = z.object({
  type: z.literal("event"),
  event: z.literal("task.log"),
  payload: z.object({
    task_id: z.string().min(1),
    level: z.enum(["debug", "info", "warn", "error"]),
    source: z.enum(["runtime", "provider", "tool", "recipe"]),
    message: z.string().min(1),
    timestamp: timestampSchema,
  }),
});

const taskResultEventSchema = z.object({
  type: z.literal("event"),
  event: z.literal("task.result"),
  payload: z.object({
    task_id: z.string().min(1),
    artifact_type: z.string().min(1).default("mind_map"),
    content: z.unknown(),
    timestamp: timestampSchema,
  }),
});

const taskErrorEventSchema = z.object({
  type: z.literal("event"),
  event: z.literal("task.error"),
  payload: z.object({
    task_id: z.string().min(1),
    code: z.string().min(1),
    message: z.string().min(1),
    details: z.string().optional(),
    timestamp: timestampSchema,
  }),
});

export const workerEventSchema = z.discriminatedUnion("event", [
  workerReadyEventSchema,
  workerHeartbeatEventSchema,
  taskStartedEventSchema,
  taskProgressEventSchema,
  taskLogEventSchema,
  taskResultEventSchema,
  taskErrorEventSchema,
]);

export type WorkerReadyEvent = z.infer<typeof workerReadyEventSchema>;
export type WorkerHeartbeatEvent = z.infer<typeof workerHeartbeatEventSchema>;
export type TaskStartedEvent = z.infer<typeof taskStartedEventSchema>;
export type TaskProgressEvent = z.infer<typeof taskProgressEventSchema>;
export type TaskLogEvent = z.infer<typeof taskLogEventSchema>;
export type TaskResultEvent = z.infer<typeof taskResultEventSchema>;
export type TaskErrorEvent = z.infer<typeof taskErrorEventSchema>;
export type RuntimeWorkerEvent = z.infer<typeof workerEventSchema>;

function isoNow() {
  return new Date().toISOString();
}

export function createWorkerReadyEvent(
  workerSessionId: string,
  runtime: string,
  version: string,
): WorkerReadyEvent {
  return {
    type: "event",
    event: "worker.ready",
    payload: {
      worker_session_id: workerSessionId,
      timestamp: isoNow(),
      runtime,
      version,
    },
  };
}

export function createWorkerHeartbeatEvent(workerSessionId: string): WorkerHeartbeatEvent {
  return {
    type: "event",
    event: "worker.heartbeat",
    payload: {
      worker_session_id: workerSessionId,
      timestamp: isoNow(),
    },
  };
}

export function createTaskStartedEvent(taskId: string, taskType: string): TaskStartedEvent {
  return {
    type: "event",
    event: "task.started",
    payload: {
      task_id: taskId,
      task_type: taskType,
      timestamp: isoNow(),
    },
  };
}

export function createTaskProgressEvent(
  taskId: string,
  stage: string,
  progress: number,
  message?: string,
): TaskProgressEvent {
  return {
    type: "event",
    event: "task.progress",
    payload: {
      task_id: taskId,
      stage,
      progress,
      message,
      timestamp: isoNow(),
    },
  };
}

export function createTaskLogEvent(
  taskId: string,
  level: "debug" | "info" | "warn" | "error",
  source: "runtime" | "provider" | "tool" | "recipe",
  message: string,
): TaskLogEvent {
  return {
    type: "event",
    event: "task.log",
    payload: {
      task_id: taskId,
      level,
      source,
      message,
      timestamp: isoNow(),
    },
  };
}

export function createTaskResultEvent(
  taskId: string,
  content: unknown,
  artifactType = "mind_map",
): TaskResultEvent {
  return {
    type: "event",
    event: "task.result",
    payload: {
      task_id: taskId,
      artifact_type: artifactType,
      content,
      timestamp: isoNow(),
    },
  };
}

export function createTaskErrorEvent(
  taskId: string,
  code: string,
  message: string,
  details?: string,
): TaskErrorEvent {
  return {
    type: "event",
    event: "task.error",
    payload: {
      task_id: taskId,
      code,
      message,
      details,
      timestamp: isoNow(),
    },
  };
}

// Legacy aliases kept temporarily so the current worker can compile while the runtime is migrated.
export interface WorkerTaskPayload {
  article_id: string;
  [key: string]: unknown;
}

export interface WorkerTaskStartParams {
  task_id: string;
  task_type: string;
  payload: WorkerTaskPayload;
}

export interface WorkerRequest {
  id: string;
  type: "request";
  method: string;
  params: WorkerTaskStartParams;
}

export type WorkerProgressEvent = TaskProgressEvent;
export type WorkerHeartbeatEventLegacy = WorkerHeartbeatEvent;
export type WorkerResultEvent = {
  type: "event";
  event: "task.result";
  payload: {
    task_id: string;
    content: unknown;
    timestamp: string;
  };
};
export type WorkerErrorEvent = {
  type: "event";
  event: "task.error";
  payload: {
    task_id: string;
    message: string;
    timestamp: string;
  };
};
