import type {
  AgentRunRequest,
  TaskErrorEvent,
  TaskProgressEvent,
  TaskResultEvent,
  TaskStartedEvent,
  WorkerErrorEvent,
  WorkerRequest,
  WorkerResultEvent,
} from "./protocol.js";
import {
  createTaskErrorEvent,
  createTaskResultEvent,
  createTaskProgressEvent,
  createTaskStartedEvent,
  createWorkerHeartbeatEvent,
  parseAgentRunRequest,
} from "./protocol.js";
import { runMindMapTask, type MindMapTaskDeps } from "./mindMapTask.js";
import { runAssistantTask, type AssistantTaskDeps } from "./assistantTask.js";
import { parseMindMapResult } from "./mindMapSchema.js";
import { z } from "zod";

const workerRequestSchema = z.object({
  id: z.string().min(1),
  type: z.literal("request"),
  method: z.string().min(1),
  params: z.object({
    task_id: z.string().min(1),
    task_type: z.string().min(1),
    payload: z
      .object({
        article_id: z.string().min(1),
      })
      .passthrough(),
  }),
});

export function parseWorkerRequest(raw: string): WorkerRequest {
  return workerRequestSchema.parse(JSON.parse(raw)) as WorkerRequest;
}

export function createProgressEvent(
  taskId: string,
  stage: string,
  progress: number,
  message?: string,
): TaskProgressEvent {
  return createTaskProgressEvent(taskId, stage, progress, message);
}

export function createHeartbeatEvent(workerSessionId: string) {
  return createWorkerHeartbeatEvent(workerSessionId);
}

export function createResultEvent(taskId: string, content: unknown): WorkerResultEvent {
  const next = createTaskResultEvent(taskId, content);
  return {
    type: next.type,
    event: next.event,
    payload: {
      task_id: next.payload.task_id,
      content: next.payload.content,
      timestamp: next.payload.timestamp,
    },
  };
}

export function createErrorEvent(taskId: string, message: string): WorkerErrorEvent {
  const next = createTaskErrorEvent(taskId, "generation_error", message);
  return {
    type: next.type,
    event: next.event,
    payload: {
      task_id: next.payload.task_id,
      message: next.payload.message,
      timestamp: next.payload.timestamp,
    },
  };
}

export function validateMindMapResult(value: unknown) {
  return parseMindMapResult(value);
}

export { parseAgentRunRequest };
export { createTaskStartedEvent, createTaskErrorEvent };

export async function executeAgentRunRequest(
  request: AgentRunRequest,
  deps: {
    runMindMapTask?: typeof runMindMapTask;
    runAssistantTask?: typeof runAssistantTask;
    saveArtifact: MindMapTaskDeps["saveArtifact"];
    reportProgress: MindMapTaskDeps["reportProgress"];
    log?: MindMapTaskDeps["log"];
    promptRunner?: MindMapTaskDeps["promptRunner"];
    workspaceRoot?: string;
    writeEvent?: (event: TaskResultEvent) => void;
    signal?: AbortSignal;
  },
) {
  if (request.params.task_type === "mind_map.generate") {
    const runner = deps.runMindMapTask ?? runMindMapTask;
    return runner(
      {
        taskId: request.params.task_id,
        articleId: request.params.input.article_id,
        displayLanguage: request.params.input.display_language,
        maxDepth: request.params.input.max_depth,
        mode: request.params.input.mode,
        articleSnapshot: {
          title: request.params.input.article_snapshot.title,
          content: request.params.input.article_snapshot.content,
          sourceType: request.params.input.article_snapshot.source_type ?? null,
        },
      },
      {
        saveArtifact: deps.saveArtifact,
        reportProgress: deps.reportProgress,
        log: deps.log,
        promptRunner: deps.promptRunner,
        workspaceRoot: deps.workspaceRoot,
        providerConfig: request.params.provider_config,
        signal: deps.signal,
      },
    );
  }

  if (request.params.task_type === "assistant.agent_turn") {
    const runner = deps.runAssistantTask ?? runAssistantTask;
    const result = await runner(
      {
        taskId: request.params.task_id,
        userMessage: request.params.input.user_message,
        conversation: request.params.input.conversation,
        uiContext: request.params.input.ui_context,
        currentMaterial: request.params.input.current_material ?? null,
        availableMaterials: request.params.input.available_materials,
      },
      {
        reportProgress: deps.reportProgress,
        log: deps.log as AssistantTaskDeps["log"],
        promptRunner: deps.promptRunner,
        workspaceRoot: deps.workspaceRoot,
        providerConfig: request.params.provider_config,
        signal: deps.signal,
      },
    );
    if (deps.signal?.aborted) {
      const error = new Error("Agent task cancelled");
      error.name = "AbortError";
      throw error;
    }
    deps.writeEvent?.(createTaskResultEvent(request.params.task_id, result, "article_answer"));
    return result;
  }

  throw new Error("Unsupported task type");
}

export async function handleAgentRunRequest(
  request: AgentRunRequest,
  deps: {
    writeEvent: (event: TaskStartedEvent | TaskProgressEvent | TaskResultEvent | TaskErrorEvent) => void;
    runTask: (request: AgentRunRequest, signal?: AbortSignal) => Promise<void>;
    signal?: AbortSignal;
  },
) {
  deps.writeEvent(createTaskStartedEvent(request.params.task_id, request.params.task_type));

  if (request.params.provider_config.kind === "unsupported") {
    deps.writeEvent(
      createTaskErrorEvent(
        request.params.task_id,
        "provider_unsupported",
        "Provider is not supported for the agent runtime",
        request.params.provider_config.reason,
      ),
    );
    return;
  }

  try {
    await deps.runTask(request, deps.signal);
  } catch (error) {
    const cancelled =
      deps.signal?.aborted || (error instanceof Error && error.name === "AbortError");
    deps.writeEvent(
      createTaskErrorEvent(
        request.params.task_id,
        cancelled ? "task_cancelled" : "internal_error",
        cancelled ? "Agent task cancelled" : "Agent runtime execution failed",
        error instanceof Error ? error.message : String(error),
      ),
    );
  }
}
