import { mkdir, rm } from "node:fs/promises";
import { join } from "node:path";

import { z } from "zod";

import type { AssistantRunInput, RuntimeProvider } from "./protocol.js";
import { runAgentPrompt, type OpenCodePromptRequest, resolveProviderModel } from "./mindMapTask.js";

const assistantActionSchema = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("open_material"),
    material_id: z.string().min(1),
  }),
  z.object({
    kind: z.literal("list_materials"),
  }),
  z.object({
    kind: z.literal("get_current_material"),
  }),
]);

const assistantTaskResultSchema = z.object({
  reply: z.string().min(1),
  action: assistantActionSchema.nullable().optional(),
});

export type AssistantTaskResult = z.infer<typeof assistantTaskResultSchema>;

export interface AssistantTaskInput {
  taskId: string;
  userMessage: string;
  conversation: AssistantRunInput["conversation"];
  uiContext: AssistantRunInput["ui_context"];
  currentMaterial: AssistantRunInput["current_material"];
  availableMaterials: AssistantRunInput["available_materials"];
}

export interface AssistantTaskDeps {
  reportProgress(taskId: string, stage: string, progress: number, message?: string): Promise<void>;
  log?: (
    level: "debug" | "info" | "warn" | "error",
    message: string,
    source?: "runtime" | "provider" | "tool" | "recipe",
  ) => void;
  promptRunner?: (request: OpenCodePromptRequest) => Promise<unknown>;
  workspaceRoot?: string;
  providerConfig: RuntimeProvider;
  signal?: AbortSignal;
}

function throwIfCancelled(signal?: AbortSignal) {
  if (!signal?.aborted) {
    return;
  }
  const error = new Error("Agent task cancelled");
  error.name = "AbortError";
  throw error;
}

function extractJsonText(text: string) {
  const trimmed = text.trim();
  const fenced = trimmed.match(/```(?:json)?\s*([\s\S]*?)```/i);
  return fenced ? fenced[1].trim() : trimmed;
}

function parsePromptResult(result: unknown) {
  if (typeof result === "string") {
    return JSON.parse(extractJsonText(result));
  }
  return result;
}

function normalizeAssistantTaskResult(raw: unknown): AssistantTaskResult {
  const next = typeof raw === "object" && raw !== null ? { ...(raw as Record<string, unknown>) } : {};
  const reply =
    typeof next.reply === "string" && next.reply.trim().length > 0
      ? next.reply.trim()
      : "已完成请求。";

  const rawAction = next.action;
  if (rawAction == null) {
    return {
      reply,
      action: null,
    };
  }

  if (typeof rawAction !== "object") {
    return {
      reply,
      action: null,
    };
  }

  const action = rawAction as Record<string, unknown>;
  const kind = typeof action.kind === "string" ? action.kind.trim() : "";

  if (kind === "open_material" && typeof action.material_id === "string" && action.material_id.trim().length > 0) {
    return {
      reply,
      action: {
        kind: "open_material",
        material_id: action.material_id.trim(),
      },
    };
  }

  if (kind === "list_materials") {
    return {
      reply,
      action: {
        kind: "list_materials",
      },
    };
  }

  if (kind === "get_current_material") {
    return {
      reply,
      action: {
        kind: "get_current_material",
      },
    };
  }

  return {
    reply,
    action: null,
  };
}

function buildSystemPrompt() {
  return [
    "You are an in-app agent for a language-learning desktop application.",
    "Use only the provided current material and available material summaries.",
    "Return only valid JSON.",
    "Do not mention hidden reasoning.",
  ].join(" ");
}

function buildPrompt(input: AssistantTaskInput) {
  return JSON.stringify(
    {
      task: "assistant_agent_turn",
      user_message: input.userMessage,
      ui_context: input.uiContext,
      current_material: input.currentMaterial,
      available_materials: input.availableMaterials,
      conversation: input.conversation,
      instructions: {
        supported_actions: [
          "get_current_material",
          "list_materials",
          "open_material",
        ],
        response_schema: {
          reply: "string",
          action: {
            kind: "open_material | list_materials | get_current_material | null",
            material_id: "required when kind is open_material",
          },
        },
      },
    },
    null,
    2,
  );
}

function createToolLog(
  action: AssistantTaskResult["action"],
): string | null {
  if (!action) {
    return null;
  }

  switch (action.kind) {
    case "open_material":
      return `calling open_material(material_id="${action.material_id}")`;
    case "list_materials":
      return "calling list_materials()";
    case "get_current_material":
      return "calling get_current_material()";
  }
}

export async function runAssistantTask(input: AssistantTaskInput, deps: AssistantTaskDeps) {
  throwIfCancelled(deps.signal);
  const log =
    deps.log ??
    ((level: "debug" | "info" | "warn" | "error", message: string) => {
      process.stderr.write(`[${level}] ${message}\n`);
    });

  await deps.reportProgress(input.taskId, "planning", 0.1, "Preparing assistant turn");

  const resolvedProvider = resolveProviderModel(deps.providerConfig);
  const promptRunner = deps.promptRunner ?? runAgentPrompt;
  const cwd = deps.workspaceRoot
    ? join(deps.workspaceRoot, input.taskId)
    : process.cwd();

  await mkdir(cwd, { recursive: true });
  log("info", "Starting assistant agent turn", "provider");

  try {
    throwIfCancelled(deps.signal);
    await deps.reportProgress(input.taskId, "thinking", 0.4, "Understanding the request");
    const result = await promptRunner({
      cwd,
      model: resolvedProvider.model,
      prompt: buildPrompt(input),
      system: buildSystemPrompt(),
      config: resolvedProvider.config,
      providerConfig: deps.providerConfig,
      signal: deps.signal,
    });

    throwIfCancelled(deps.signal);
    await deps.reportProgress(input.taskId, "finalizing", 0.8, "Preparing the final response");
    const normalized = normalizeAssistantTaskResult(parsePromptResult(result));
    const parsed = assistantTaskResultSchema.parse(normalized);
    if (normalized.action === null) {
      const raw = parsePromptResult(result);
      const rawKind =
        typeof raw === "object" && raw !== null && typeof (raw as { action?: { kind?: unknown } }).action?.kind === "string"
          ? (raw as { action: { kind: string } }).action.kind
          : null;
      if (rawKind) {
        log("warn", `Unknown assistant action kind: ${rawKind}`, "recipe");
      }
    }
    const toolLog = createToolLog(parsed.action ?? null);
    if (toolLog) {
      log("info", toolLog, "tool");
    }
    return parsed;
  } finally {
    if (deps.workspaceRoot) {
      await rm(cwd, { recursive: true, force: true });
    }
  }
}
