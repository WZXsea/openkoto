import { createHash } from "node:crypto";
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { createOpencode } from "@opencode-ai/sdk";
import type { Config, Part, SessionPromptResponse } from "@opencode-ai/sdk";

import { parseMindMapResult } from "./mindMapSchema.js";
import type { ArticleSnapshot, RuntimeProvider } from "./protocol.js";

export const OPENAI_COMPATIBLE_REQUEST_TIMEOUT_MS = 120_000;

const MIND_MAP_OUTPUT_SCHEMA = {
  type: "object",
  required: ["status", "diagnostics"],
  properties: {
    status: {
      type: "string",
      enum: ["applicable", "partial", "not_applicable"],
    },
    reason: {
      type: ["string", "null"],
    },
    map: {
      type: ["object", "null"],
      required: [
        "version",
        "article_id",
        "title",
        "display_language",
        "generation_mode",
        "source_hash",
        "summary",
        "root",
      ],
      properties: {
        version: { type: "string" },
        article_id: { type: "string" },
        title: { type: "string" },
        display_language: { type: "string" },
        generation_mode: { type: "string" },
        source_hash: { type: "string" },
        summary: { type: "string" },
        root: { $ref: "#/$defs/node" },
      },
    },
    diagnostics: {
      type: "object",
      required: ["content_type", "coverage", "window_count", "evidence_density"],
      properties: {
        content_type: {
          type: "string",
          enum: [
            "narrative",
            "lecture",
            "dialogue",
            "article",
            "lyrics",
            "music_only",
            "mixed",
            "unknown",
          ],
        },
        coverage: {
          type: "string",
          enum: ["full", "partial", "none"],
        },
        notes: {
          type: "array",
          items: { type: "string" },
        },
        window_count: { type: "integer", minimum: 0 },
        evidence_density: { type: "number", minimum: 0, maximum: 1 },
        low_confidence_node_ids: {
          type: "array",
          items: { type: "string" },
        },
      },
    },
  },
  $defs: {
    node: {
      type: "object",
      required: ["id", "title", "node_type", "summary", "confidence", "children"],
      properties: {
        id: { type: "string" },
        title: { type: "string" },
        node_type: {
          type: "string",
          enum: ["root", "theme", "topic", "event", "entity", "relation", "evidence"],
        },
        summary: { type: "string" },
        confidence: { type: "number", minimum: 0, maximum: 1 },
        source_segment_ids: {
          type: "array",
          items: { type: "string" },
        },
        source_offsets: {
          type: "array",
          items: {
            type: "object",
            required: ["start", "end"],
            properties: {
              start: { type: "integer", minimum: 0 },
              end: { type: "integer", minimum: 0 },
            },
          },
        },
        time_range: {
          type: "object",
          required: ["start", "end"],
          properties: {
            start: { type: "number" },
            end: { type: "number" },
          },
        },
        children: {
          type: "array",
          items: { $ref: "#/$defs/node" },
        },
      },
    },
  },
} as const;

export interface MindMapTaskInput {
  taskId: string;
  articleId: string;
  displayLanguage: string;
  maxDepth: number;
  mode: "fast" | "balanced" | "deep";
  articleSnapshot: {
    title: string;
    content: string;
    sourceType?: string | null;
  };
}

export interface OpenCodePromptRequest {
  cwd: string;
  model: string;
  prompt: string;
  system: string;
  config: Config;
  providerConfig: RuntimeProvider;
  signal?: AbortSignal;
}

export interface MindMapTaskDeps {
  promptRunner?: (request: OpenCodePromptRequest) => Promise<unknown>;
  saveArtifact(taskId: string, artifactType: "mind_map", content: unknown): Promise<{ artifact_id: string }>;
  reportProgress(taskId: string, stage: string, progress: number, message?: string): Promise<void>;
  log?: (
    level: "debug" | "info" | "warn" | "error",
    message: string,
    source?: "runtime" | "provider" | "tool" | "recipe",
  ) => void;
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

export function buildMindMapWorkspaceFiles(input: MindMapTaskInput): Record<string, string> {
  return {
    "article-source.json": JSON.stringify(
      {
        article_id: input.articleId,
        title: input.articleSnapshot.title,
        source_type: input.articleSnapshot.sourceType ?? null,
        content: input.articleSnapshot.content,
      },
      null,
      2,
    ),
    "mind-map-schema.json": JSON.stringify(MIND_MAP_OUTPUT_SCHEMA, null, 2),
    "TASK.md": [
      "# Mind Map Task",
      "",
      `Task ID: ${input.taskId}`,
      `Article ID: ${input.articleId}`,
      `Display language: ${input.displayLanguage}`,
      `Max depth: ${input.maxDepth}`,
      `Mode: ${input.mode}`,
      "",
      "Read `article-source.json` as the only source document.",
      "Return only valid JSON that matches `mind-map-schema.json`.",
      "Do not edit files or use shell commands.",
    ].join("\n"),
  };
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

function inferContentType(sourceType?: string | null) {
  const normalized = sourceType?.toLowerCase() ?? "";
  if (normalized.includes("audio")) {
    return "dialogue";
  }
  if (normalized.includes("video") || normalized.includes("youtube")) {
    return "dialogue";
  }
  if (normalized.includes("book") || normalized.includes("article") || normalized.includes("web")) {
    return "article";
  }
  return "unknown";
}

function buildSourceHash(content: string) {
  return `sha256:${createHash("sha256").update(content).digest("hex")}`;
}

function normalizeNode(node: unknown, fallbackTitle: string, depth = 0): Record<string, unknown> {
  const next = typeof node === "object" && node !== null ? { ...(node as Record<string, unknown>) } : {};
  const title =
    typeof next.title === "string" && next.title.trim().length > 0
      ? next.title.trim()
      : fallbackTitle;
  const summary =
    typeof next.summary === "string"
      ? next.summary
      : depth === 0
        ? `Overview of ${title}.`
        : `${title} is a key branch in the source.`;
  const nodeType =
    typeof next.node_type === "string" && next.node_type.length > 0
      ? next.node_type
      : depth === 0
        ? "root"
        : "topic";
  const childrenInput = Array.isArray(next.children) ? next.children : [];

  return {
    id: typeof next.id === "string" && next.id.length > 0 ? next.id : depth === 0 ? "root" : `node-${depth}-${title}`,
    title,
    node_type: nodeType,
    summary,
    confidence:
      typeof next.confidence === "number" && Number.isFinite(next.confidence)
        ? Math.max(0, Math.min(1, next.confidence))
        : depth === 0
          ? 0.8
          : 0.6,
    source_segment_ids: Array.isArray(next.source_segment_ids) ? next.source_segment_ids : [],
    source_offsets: Array.isArray(next.source_offsets) ? next.source_offsets : [],
    ...(typeof next.time_range === "object" && next.time_range !== null ? { time_range: next.time_range } : {}),
    children: childrenInput.map((child, index) => normalizeNode(child, `${title} ${index + 1}`, depth + 1)),
  };
}

export function normalizeMindMapResult(raw: unknown, input: MindMapTaskInput) {
  const next = typeof raw === "object" && raw !== null ? { ...(raw as Record<string, unknown>) } : {};
  const status =
    next.status === "applicable" || next.status === "partial" || next.status === "not_applicable"
      ? next.status
      : "partial";
  const reason = typeof next.reason === "string" || next.reason === null ? next.reason : null;
  const diagnostics =
    typeof next.diagnostics === "object" && next.diagnostics !== null
      ? { ...(next.diagnostics as Record<string, unknown>) }
      : {};
  const notes = Array.isArray(diagnostics.notes) ? diagnostics.notes : [];
  const lowConfidenceNodeIds = Array.isArray(diagnostics.low_confidence_node_ids)
    ? diagnostics.low_confidence_node_ids
    : [];

  if (status === "not_applicable") {
    return {
      status,
      reason,
      map: null,
      diagnostics: {
        content_type:
          typeof diagnostics.content_type === "string"
            ? diagnostics.content_type
            : inferContentType(input.articleSnapshot.sourceType),
        coverage: "none",
        notes,
        window_count:
          typeof diagnostics.window_count === "number" && Number.isFinite(diagnostics.window_count)
            ? diagnostics.window_count
            : 1,
        evidence_density:
          typeof diagnostics.evidence_density === "number" && Number.isFinite(diagnostics.evidence_density)
            ? diagnostics.evidence_density
            : 0,
        low_confidence_node_ids: lowConfidenceNodeIds,
      },
    };
  }

  const map = typeof next.map === "object" && next.map !== null ? { ...(next.map as Record<string, unknown>) } : {};
  const fallbackTitle = input.articleSnapshot.title || "Mind Map";
  const root = normalizeNode(map.root, fallbackTitle);
  const summary =
    typeof map.summary === "string" && map.summary.trim().length > 0
      ? map.summary
      : root.summary;

  return {
    status,
    reason,
    map: {
      version: typeof map.version === "string" && map.version.trim().length > 0 ? map.version : "1",
      article_id:
        typeof map.article_id === "string" && map.article_id.trim().length > 0
          ? map.article_id
          : input.articleId,
      title:
        typeof map.title === "string" && map.title.trim().length > 0
          ? map.title
          : input.articleSnapshot.title,
      display_language:
        typeof map.display_language === "string" && map.display_language.trim().length > 0
          ? map.display_language
          : input.displayLanguage,
      generation_mode:
        typeof map.generation_mode === "string" && map.generation_mode.trim().length > 0
          ? map.generation_mode
          : "evidence_first",
      source_hash:
        typeof map.source_hash === "string" && map.source_hash.trim().length > 0
          ? map.source_hash
          : buildSourceHash(input.articleSnapshot.content),
      summary,
      root,
    },
    diagnostics: {
      content_type:
        typeof diagnostics.content_type === "string"
          ? diagnostics.content_type
          : inferContentType(input.articleSnapshot.sourceType),
      coverage:
        typeof diagnostics.coverage === "string"
          ? diagnostics.coverage
          : status === "partial"
            ? "partial"
            : "full",
      notes,
      window_count:
        typeof diagnostics.window_count === "number" && Number.isFinite(diagnostics.window_count)
          ? diagnostics.window_count
          : 1,
      evidence_density:
        typeof diagnostics.evidence_density === "number" && Number.isFinite(diagnostics.evidence_density)
          ? Math.max(0, Math.min(1, diagnostics.evidence_density))
          : 0,
      low_confidence_node_ids: lowConfidenceNodeIds,
    },
  };
}

function createNamedModelDefinition(model: string) {
  return {
    id: model,
    name: model,
    tool_call: false,
    reasoning: false,
  };
}

export function resolveProviderModel(providerConfig: RuntimeProvider): {
  model: string;
  config: Config;
} {
  const baseConfig: Config = {
    plugin: [],
    autoupdate: false,
    permission: {
      edit: "deny",
      bash: "deny",
      webfetch: "deny",
      external_directory: "deny",
    },
  };

  if (providerConfig.kind === "native_google") {
    const model = `google/${providerConfig.model}`;
    return {
      model,
      config: {
        ...baseConfig,
        model,
        enabled_providers: ["google"],
        provider: {
          google: {
            options: {
              apiKey: providerConfig.api_key,
            },
            models: {
              [providerConfig.model]: createNamedModelDefinition(providerConfig.model),
            },
          },
        },
      },
    };
  }

  if (providerConfig.kind === "native_anthropic") {
    const model = `anthropic/${providerConfig.model}`;
    return {
      model,
      config: {
        ...baseConfig,
        model,
        enabled_providers: ["anthropic"],
        provider: {
          anthropic: {
            options: {
              apiKey: providerConfig.api_key,
            },
            models: {
              [providerConfig.model]: createNamedModelDefinition(providerConfig.model),
            },
          },
        },
      },
    };
  }

  if (providerConfig.kind === "openai_compatible") {
    const normalizedBaseUrl = providerConfig.baseUrl.replace(/\/$/, "");
    if (providerConfig.provider === "openai" || providerConfig.provider === "openrouter") {
      const model = `${providerConfig.provider}/${providerConfig.model}`;
      return {
        model,
        config: {
          ...baseConfig,
          model,
          enabled_providers: [providerConfig.provider],
          provider: {
            [providerConfig.provider]: {
              options: {
                apiKey: providerConfig.api_key,
                baseURL: normalizedBaseUrl,
              },
              models: {
                [providerConfig.model]: createNamedModelDefinition(providerConfig.model),
              },
            },
          },
        },
      };
    }

    const providerId = "textlingo_openai_compatible";
    const endpoint = normalizedBaseUrl.endsWith("/chat/completions")
      ? normalizedBaseUrl
      : `${normalizedBaseUrl}/chat/completions`;
    const model = `${providerId}/${providerConfig.model}`;
    return {
      model,
      config: {
        ...baseConfig,
        model,
        enabled_providers: [providerId],
        provider: {
          [providerId]: {
            api: endpoint,
            options: {
              apiKey: providerConfig.api_key,
              baseURL: normalizedBaseUrl,
            },
            models: {
              [providerConfig.model]: createNamedModelDefinition(providerConfig.model),
            },
          },
        },
      },
    };
  }

  throw new Error(`Unsupported provider kind: ${providerConfig.kind}`);
}

async function createTaskWorkspace(input: MindMapTaskInput, workspaceRoot?: string) {
  const root = workspaceRoot ?? tmpdir();
  await mkdir(root, { recursive: true });
  const cwd = await mkdtemp(join(root, "mind-map-task-"));
  const files = buildMindMapWorkspaceFiles(input);
  await Promise.all(
    Object.entries(files).map(([name, content]) => writeFile(join(cwd, name), content, "utf8")),
  );
  return cwd;
}

function buildPrompt(input: MindMapTaskInput) {
  return JSON.stringify(
    {
      task: "Generate an evidence-grounded mind map for this article.",
      instructions: [
        "Use article_source as the only source document.",
        "Return only valid JSON, with no markdown fences.",
        "Include every required field in map and diagnostics.",
        "If the source is unsuitable, return status not_applicable, map null, and complete diagnostics.",
        `The display language must be ${input.displayLanguage}.`,
      ],
      article_source: {
        article_id: input.articleId,
        title: input.articleSnapshot.title,
        source_type: input.articleSnapshot.sourceType ?? null,
        content: input.articleSnapshot.content,
      },
      output_schema: MIND_MAP_OUTPUT_SCHEMA,
    },
    null,
    2,
  );
}

export async function findAvailablePort() {
  return new Promise<number>((resolve, reject) => {
    const server = createServer();
    server.unref();
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        server.close(() => reject(new Error("Failed to resolve an available port")));
        return;
      }
      const { port } = address;
      server.close((error) => {
        if (error) {
          reject(error);
          return;
        }
        resolve(port);
      });
    });
  });
}

function buildSystemPrompt(input: MindMapTaskInput) {
  return [
    "You generate article-level mind maps from source text.",
    "Use the article snapshot in the workspace as the source of truth.",
    `Keep the tree depth at or below ${input.maxDepth}.`,
    "Do not invent details that are not supported by the source.",
  ].join(" ");
}

function extractTextParts(response: SessionPromptResponse) {
  return response.parts
    .filter((part: Part): part is Extract<Part, { type: "text" }> => part.type === "text")
    .map((part) => part.text)
    .join("\n")
    .trim();
}

export async function runOpenCodePrompt(request: OpenCodePromptRequest) {
  throwIfCancelled(request.signal);
  const port = await findAvailablePort();
  const { client, server } = await createOpencode({
    hostname: "127.0.0.1",
    port,
    config: request.config,
  });

  let sessionId: string | null = null;
  const abortSession = () => {
    if (!sessionId) {
      return;
    }
    void client.session.abort({
      path: { id: sessionId },
      query: { directory: request.cwd },
    });
  };
  request.signal?.addEventListener("abort", abortSession, { once: true });

  try {
    throwIfCancelled(request.signal);
    const session = await client.session.create({
      query: {
        directory: request.cwd,
      },
      body: {
        title: "Mind Map Generation",
      },
    });
    if (!session.data || session.error) {
      throw new Error(session.error ? JSON.stringify(session.error) : "Failed to create OpenCode session");
    }
    sessionId = session.data.id;
    if (request.signal?.aborted) {
      await client.session.abort({
        path: { id: sessionId },
        query: { directory: request.cwd },
      });
      throwIfCancelled(request.signal);
    }

    const response = await client.session.prompt({
      path: {
        id: session.data.id,
      },
      query: {
        directory: request.cwd,
      },
      body: {
        system: request.system,
        model: {
          providerID: request.model.split("/")[0],
          modelID: request.model.slice(request.model.indexOf("/") + 1),
        },
        parts: [
          {
            type: "text",
            text: request.prompt,
          },
        ],
      },
    });
    if (!response.data || response.error) {
      throw new Error(response.error ? JSON.stringify(response.error) : "OpenCode prompt failed");
    }

    throwIfCancelled(request.signal);

    return parsePromptResult(extractTextParts(response.data));
  } finally {
    request.signal?.removeEventListener("abort", abortSession);
    server.close();
  }
}

function openAICompatibleEndpoint(baseUrl: string) {
  const normalized = baseUrl.replace(/\/+$/, "");
  return normalized.endsWith("/chat/completions")
    ? normalized
    : `${normalized}/chat/completions`;
}

function redactProviderError(message: string, apiKey?: string) {
  let sanitized = message;
  if (apiKey) {
    sanitized = sanitized.split(apiKey).join("[redacted]");
  }
  return sanitized
    .replace(/Bearer\s+[^\s"',}]+/gi, "Bearer [redacted]")
    .slice(0, 500);
}

function responseErrorMessage(payload: unknown) {
  if (typeof payload !== "object" || payload === null) {
    return null;
  }
  const error = (payload as { error?: unknown }).error;
  if (typeof error === "string") {
    return error;
  }
  if (typeof error === "object" && error !== null) {
    const message = (error as { message?: unknown }).message;
    return typeof message === "string" ? message : null;
  }
  return null;
}

function openAICompatibleResponseText(payload: unknown) {
  if (typeof payload !== "object" || payload === null) {
    throw new Error("OpenAI-compatible response is not a JSON object");
  }
  const choices = (payload as { choices?: unknown }).choices;
  if (!Array.isArray(choices) || choices.length === 0) {
    throw new Error("OpenAI-compatible response does not contain choices");
  }
  const message = (choices[0] as { message?: unknown } | undefined)?.message;
  if (typeof message !== "object" || message === null) {
    throw new Error("OpenAI-compatible response does not contain a message");
  }
  const content = (message as { content?: unknown }).content;
  if (typeof content === "string" && content.trim()) {
    return content;
  }
  if (Array.isArray(content)) {
    const text = content
      .map((part) => {
        if (typeof part !== "object" || part === null) {
          return "";
        }
        const value = (part as { text?: unknown }).text;
        return typeof value === "string" ? value : "";
      })
      .filter(Boolean)
      .join("\n")
      .trim();
    if (text) {
      return text;
    }
  }
  throw new Error("OpenAI-compatible response message is empty");
}

export async function runOpenAICompatiblePrompt(request: OpenCodePromptRequest) {
  if (request.providerConfig.kind !== "openai_compatible") {
    throw new Error("OpenAI-compatible prompt runner received an incompatible provider");
  }
  throwIfCancelled(request.signal);

  const providerConfig = request.providerConfig;
  const headers: Record<string, string> = {
    "content-type": "application/json",
  };
  if (providerConfig.api_key) {
    headers.authorization = `Bearer ${providerConfig.api_key}`;
  }

  const requestController = new AbortController();
  let timedOut = false;
  const cancelRequest = () => requestController.abort(request.signal?.reason);
  request.signal?.addEventListener("abort", cancelRequest, { once: true });
  const timeout = setTimeout(() => {
    timedOut = true;
    requestController.abort();
  }, OPENAI_COMPATIBLE_REQUEST_TIMEOUT_MS);
  timeout.unref();

  try {
    const response = await fetch(openAICompatibleEndpoint(providerConfig.baseUrl), {
      method: "POST",
      headers,
      body: JSON.stringify({
        model: providerConfig.model,
        messages: [
          {
            role: "system",
            content: request.system,
          },
          {
            role: "user",
            content: request.prompt,
          },
        ],
      }),
      signal: requestController.signal,
    });
    throwIfCancelled(request.signal);

    let payload: unknown;
    try {
      payload = await response.json();
    } catch {
      throw new Error(`OpenAI-compatible request returned invalid JSON (HTTP ${response.status})`);
    }
    throwIfCancelled(request.signal);

    if (!response.ok) {
      const providerMessage = responseErrorMessage(payload);
      const suffix = providerMessage
        ? `: ${redactProviderError(providerMessage, providerConfig.api_key)}`
        : "";
      throw new Error(`OpenAI-compatible request failed (HTTP ${response.status})${suffix}`);
    }

    return parsePromptResult(openAICompatibleResponseText(payload));
  } catch (error) {
    if (timedOut) {
      throw new Error(
        `OpenAI-compatible request timed out after ${OPENAI_COMPATIBLE_REQUEST_TIMEOUT_MS / 1000} seconds`,
      );
    }
    throwIfCancelled(request.signal);
    throw error;
  } finally {
    clearTimeout(timeout);
    request.signal?.removeEventListener("abort", cancelRequest);
  }
}

export async function runAgentPrompt(request: OpenCodePromptRequest) {
  if (request.providerConfig.kind === "openai_compatible") {
    return runOpenAICompatiblePrompt(request);
  }
  throw new Error(
    `Built-in direct runtime does not support provider kind ${request.providerConfig.kind}; configure an OpenAI-compatible provider`,
  );
}

export async function runMindMapTask(input: MindMapTaskInput, deps: MindMapTaskDeps) {
  throwIfCancelled(deps.signal);
  await deps.reportProgress(input.taskId, "planning", 0.1, "Preparing mind map task");
  const log =
    deps.log ??
    ((level: "debug" | "info" | "warn" | "error", message: string) => {
      process.stderr.write(`[${level}] ${message}\n`);
    });

  const cwd = await createTaskWorkspace(input, deps.workspaceRoot);
  log("info", `Prepared task workspace: ${cwd}`, "recipe");

  try {
    const resolvedProvider = resolveProviderModel(deps.providerConfig);
    const promptRunner = deps.promptRunner ?? runAgentPrompt;
    throwIfCancelled(deps.signal);

    await deps.reportProgress(input.taskId, "starting_agent", 0.2, "Starting agent runtime");
    await deps.reportProgress(
      input.taskId,
      "analyzing",
      0.35,
      "Agent runtime is analyzing the source",
    );
    log("info", "Starting mind map model request", "provider");

    const result = await promptRunner({
      cwd,
      model: resolvedProvider.model,
      prompt: buildPrompt(input),
      system: buildSystemPrompt(input),
      config: resolvedProvider.config,
      providerConfig: deps.providerConfig,
      signal: deps.signal,
    });

    throwIfCancelled(deps.signal);
    log("info", "Mind map model returned a final result", "provider");
    await deps.reportProgress(input.taskId, "validating", 0.75, "Validating mind map output");
    const normalized = normalizeMindMapResult(parsePromptResult(result), input);
    const parsed = parseMindMapResult(normalized);
    log("info", "Mind map result validated", "recipe");
    await deps.reportProgress(input.taskId, "saving", 0.9, "Saving mind map artifact");
    throwIfCancelled(deps.signal);
    const artifact = await deps.saveArtifact(input.taskId, "mind_map", parsed);
    log("info", `Mind map artifact saved: ${artifact.artifact_id}`, "runtime");

    return artifact;
  } finally {
    await rm(cwd, { recursive: true, force: true });
  }
}
