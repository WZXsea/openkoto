import { Agent, type AgentEvent } from "@earendil-works/pi-agent-core";
import {
  createModels,
  createProvider,
  type Model,
  type Models,
} from "@earendil-works/pi-ai";
import { openAICompletionsApi } from "@earendil-works/pi-ai/api/openai-completions.lazy";
import { anthropicProvider } from "@earendil-works/pi-ai/providers/anthropic";
import { googleProvider } from "@earendil-works/pi-ai/providers/google";

import type { RuntimeProvider } from "./protocol.js";

export const DEFAULT_PI_AGENT_TIMEOUT_MS = 300_000;
export const PI_AGENT_TIMEOUT_ENV = "TEXTLINGO_PI_AGENT_TIMEOUT_MS";

const CUSTOM_OPENAI_PROVIDER_ID = "textlingo-openai-compatible";
const FALLBACK_CONTEXT_WINDOW = 128_000;
const FALLBACK_MAX_TOKENS = 8_192;
const ZERO_COST = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
};

export type PiRuntimeEvent = AgentEvent;

export interface PiAgentPromptRequest {
  prompt: string;
  system: string;
  providerConfig: RuntimeProvider;
  cwd?: string;
  signal?: AbortSignal;
  timeoutMs?: number;
  onEvent?: (event: PiRuntimeEvent) => Promise<void> | void;
}

export interface PiRuntimeBinding {
  models: Models;
  model: Model<string>;
  apiKey?: string;
}

export interface PiAgentRuntimeDeps {
  createRuntime?: (providerConfig: RuntimeProvider) => PiRuntimeBinding;
}

export class PiAgentTimeoutError extends Error {
  constructor(timeoutMs: number) {
    super(`Pi agent request timed out after ${timeoutMs} ms`);
    this.name = "TimeoutError";
  }
}

export class PiAgentProviderError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PiAgentProviderError";
  }
}

export class PiAgentStopReasonError extends Error {
  constructor(stopReason: string) {
    super(`Pi agent stopped without a complete response: ${stopReason}`);
    this.name = "PiAgentStopReasonError";
  }
}

function createAbortError() {
  const error = new Error("Agent task cancelled");
  error.name = "AbortError";
  return error;
}

function normalizeBaseUrl(baseUrl: string) {
  return baseUrl.replace(/\/+$/, "").replace(/\/chat\/completions$/, "");
}

function fallbackModel(
  provider: string,
  modelId: string,
  api: "google-generative-ai" | "anthropic-messages",
  template?: Model<string>,
): Model<string> {
  return {
    ...(template ?? {
      baseUrl: "",
      input: ["text"] as const,
      cost: ZERO_COST,
      contextWindow: FALLBACK_CONTEXT_WINDOW,
      maxTokens: FALLBACK_MAX_TOKENS,
    }),
    id: modelId,
    name: modelId,
    provider,
    api,
    reasoning: false,
  };
}

function resolveCatalogModel(models: Models, provider: string, requestedModelId: string) {
  const exact = models.getModel(provider, requestedModelId);
  if (exact) {
    return exact;
  }
  const normalizedModelId = requestedModelId.replace(/^models\//, "");
  const normalized = models.getModel(provider, normalizedModelId);
  if (normalized) {
    return {
      ...normalized,
      id: requestedModelId,
      name: requestedModelId,
    };
  }
  return undefined;
}

function createNativeGoogleRuntime(providerConfig: Extract<RuntimeProvider, { kind: "native_google" }>) {
  const models = createModels();
  models.setProvider(googleProvider());
  const model =
    resolveCatalogModel(models, "google", providerConfig.model) ??
    fallbackModel(
      "google",
      providerConfig.model,
      "google-generative-ai",
      models.getModels("google")[0],
    );
  return {
    models,
    model,
    apiKey: providerConfig.api_key,
  };
}

function createNativeAnthropicRuntime(
  providerConfig: Extract<RuntimeProvider, { kind: "native_anthropic" }>,
) {
  const models = createModels();
  models.setProvider(anthropicProvider());
  const model =
    resolveCatalogModel(models, "anthropic", providerConfig.model) ??
    fallbackModel(
      "anthropic",
      providerConfig.model,
      "anthropic-messages",
      models.getModels("anthropic")[0],
    );
  return {
    models,
    model,
    apiKey: providerConfig.api_key,
  };
}

function createOpenAICompatibleRuntime(
  providerConfig: Extract<RuntimeProvider, { kind: "openai_compatible" }>,
) {
  const baseUrl = normalizeBaseUrl(providerConfig.baseUrl);
  const model: Model<"openai-completions"> = {
    id: providerConfig.model,
    name: providerConfig.model,
    api: "openai-completions",
    provider: CUSTOM_OPENAI_PROVIDER_ID,
    baseUrl,
    reasoning: false,
    input: ["text"],
    cost: ZERO_COST,
    contextWindow: FALLBACK_CONTEXT_WINDOW,
    maxTokens: FALLBACK_MAX_TOKENS,
  };
  const provider = createProvider({
    id: CUSTOM_OPENAI_PROVIDER_ID,
    name: providerConfig.provider,
    baseUrl,
    auth: {
      apiKey: {
        name: `${providerConfig.provider} API key`,
        resolve: async () => ({ auth: {} }),
      },
    },
    models: [model],
    api: openAICompletionsApi(),
  });
  const models = createModels();
  models.setProvider(provider);
  return {
    models,
    model,
    apiKey: providerConfig.api_key,
  };
}

export function createPiRuntime(providerConfig: RuntimeProvider): PiRuntimeBinding {
  switch (providerConfig.kind) {
    case "native_google":
      return createNativeGoogleRuntime(providerConfig);
    case "native_anthropic":
      return createNativeAnthropicRuntime(providerConfig);
    case "openai_compatible":
      return createOpenAICompatibleRuntime(providerConfig);
    case "unsupported":
      throw new PiAgentProviderError(providerConfig.reason);
  }
}

export function resolvePiAgentTimeoutMs(
  explicitTimeoutMs?: number,
  env: NodeJS.ProcessEnv = process.env,
) {
  if (explicitTimeoutMs !== undefined) {
    if (!Number.isSafeInteger(explicitTimeoutMs) || explicitTimeoutMs <= 0) {
      throw new RangeError("Pi agent timeout must be a positive integer");
    }
    return explicitTimeoutMs;
  }

  const configured = env[PI_AGENT_TIMEOUT_ENV]?.trim();
  if (!configured) {
    return DEFAULT_PI_AGENT_TIMEOUT_MS;
  }
  const parsed = Number(configured);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new RangeError(`${PI_AGENT_TIMEOUT_ENV} must be a positive integer`);
  }
  return parsed;
}

function redactProviderMessage(message: string, apiKey?: string) {
  let sanitized = message;
  if (apiKey) {
    sanitized = sanitized.split(apiKey).join("[redacted]");
  }
  return sanitized
    .replace(/Bearer\s+[^\s"',}]+/gi, "Bearer [redacted]")
    .slice(0, 500);
}

function lastAssistantMessage(agent: Agent) {
  return [...agent.state.messages]
    .reverse()
    .find((message) => message.role === "assistant");
}

function extractAssistantText(message: NonNullable<ReturnType<typeof lastAssistantMessage>>) {
  return message.content
    .filter((part) => part.type === "text")
    .map((part) => part.text)
    .join("\n")
    .trim();
}

export async function runPiAgentPrompt(
  request: PiAgentPromptRequest,
  deps: PiAgentRuntimeDeps = {},
) {
  if (request.signal?.aborted) {
    throw createAbortError();
  }

  const timeoutMs = resolvePiAgentTimeoutMs(request.timeoutMs);
  const runtime = (deps.createRuntime ?? createPiRuntime)(request.providerConfig);
  const streamFn = runtime.models.streamSimple.bind(runtime.models);
  const agent = new Agent({
    initialState: {
      systemPrompt: request.system,
      model: runtime.model,
      tools: [],
      messages: [],
      thinkingLevel: "off",
    },
    streamFn,
    getApiKey: () => runtime.apiKey,
  });

  let timedOut = false;
  const cancelAgent = () => agent.abort();
  request.signal?.addEventListener("abort", cancelAgent, { once: true });
  const timeout = setTimeout(() => {
    timedOut = true;
    agent.abort();
  }, timeoutMs);
  timeout.unref();
  const unsubscribe = request.onEvent
    ? agent.subscribe((event) => request.onEvent?.(event))
    : () => undefined;

  try {
    const pending = agent.prompt(request.prompt);
    if (request.signal?.aborted) {
      agent.abort();
    }
    await pending;
    await agent.waitForIdle();

    const message = lastAssistantMessage(agent);
    if (timedOut) {
      throw new PiAgentTimeoutError(timeoutMs);
    }
    if (request.signal?.aborted) {
      throw createAbortError();
    }
    if (!message) {
      throw new PiAgentProviderError("Pi agent did not return an assistant message");
    }
    if (message.stopReason === "error") {
      throw new PiAgentProviderError(
        redactProviderMessage(
          message.errorMessage || "Pi provider request failed",
          runtime.apiKey,
        ),
      );
    }
    if (message.stopReason === "aborted") {
      throw new PiAgentProviderError(
        redactProviderMessage(
          message.errorMessage || "Pi provider request was aborted",
          runtime.apiKey,
        ),
      );
    }
    if (message.stopReason !== "stop") {
      throw new PiAgentStopReasonError(message.stopReason);
    }

    const text = extractAssistantText(message);
    if (!text) {
      throw new PiAgentProviderError("Pi agent returned an empty response");
    }
    return text;
  } catch (error) {
    if (timedOut) {
      throw new PiAgentTimeoutError(timeoutMs);
    }
    if (request.signal?.aborted) {
      throw createAbortError();
    }
    if (
      error instanceof PiAgentProviderError ||
      error instanceof PiAgentStopReasonError ||
      error instanceof PiAgentTimeoutError
    ) {
      throw error;
    }
    throw new PiAgentProviderError(
      redactProviderMessage(
        error instanceof Error ? error.message : String(error),
        runtime.apiKey,
      ),
    );
  } finally {
    clearTimeout(timeout);
    request.signal?.removeEventListener("abort", cancelAgent);
    unsubscribe();
  }
}
