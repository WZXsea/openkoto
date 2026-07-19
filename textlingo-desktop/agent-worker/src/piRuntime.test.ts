import { createServer } from "node:http";
import type { AddressInfo } from "node:net";

import {
  createModels,
  fauxAssistantMessage,
  fauxProvider,
  type FauxProviderHandle,
} from "@earendil-works/pi-ai";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  PI_AGENT_TIMEOUT_ENV,
  PiAgentProviderError,
  PiAgentStopReasonError,
  PiAgentTimeoutError,
  createPiRuntime,
  resolvePiAgentTimeoutMs,
  runPiAgentPrompt,
  type PiRuntimeBinding,
} from "./piRuntime.js";

const OPENAI_PROVIDER_CONFIG = {
  kind: "openai_compatible" as const,
  provider: "openai-compatible",
  model: "custom-model",
  api_key: "test-secret",
  baseUrl: "https://models.example.test/v1/",
};

function fauxBinding(
  faux: FauxProviderHandle,
  apiKey = "test-secret",
): PiRuntimeBinding {
  const models = createModels();
  models.setProvider(faux.provider);
  return {
    models,
    model: faux.getModel(),
    apiKey,
  };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("piRuntime", () => {
  it("constructs isolated runtime bindings for every supported provider kind", () => {
    const openAI = createPiRuntime(OPENAI_PROVIDER_CONFIG);
    expect(openAI.model).toMatchObject({
      provider: "textlingo-openai-compatible",
      api: "openai-completions",
      id: "custom-model",
      baseUrl: "https://models.example.test/v1",
    });
    expect(openAI.apiKey).toBe("test-secret");
    expect(openAI.models.getProviders()).toHaveLength(1);

    const google = createPiRuntime({
      kind: "native_google",
      provider: "google-ai-studio",
      model: "models/gemini-3-flash-preview",
      api_key: "google-secret",
    });
    expect(google.model.provider).toBe("google");
    expect(google.model.api).toBe("google-generative-ai");
    expect(google.model.id).toBe("models/gemini-3-flash-preview");
    expect(google.apiKey).toBe("google-secret");

    const anthropic = createPiRuntime({
      kind: "native_anthropic",
      provider: "anthropic",
      model: "claude-sonnet-4-5",
      api_key: "anthropic-secret",
    });
    expect(anthropic.model.provider).toBe("anthropic");
    expect(anthropic.model.api).toBe("anthropic-messages");
    expect(anthropic.apiKey).toBe("anthropic-secret");
  });

  it("uses request-scoped auth, tools:[], and forwards Pi streaming events", async () => {
    const faux = fauxProvider({
      provider: "faux-streaming",
      tokenSize: { min: 1, max: 1 },
    });
    let observedApiKey: string | undefined;
    let observedTools: unknown;
    faux.setResponses([
      (context, options) => {
        observedApiKey = options?.apiKey;
        observedTools = context.tools;
        return fauxAssistantMessage('{"status":"ok"}');
      },
    ]);
    const events: string[] = [];

    const result = await runPiAgentPrompt(
      {
        prompt: "Return JSON",
        system: "System",
        providerConfig: OPENAI_PROVIDER_CONFIG,
        onEvent(event) {
          events.push(
            event.type === "message_update"
              ? `${event.type}:${event.assistantMessageEvent.type}`
              : event.type,
          );
        },
      },
      {
        createRuntime: () => fauxBinding(faux),
      },
    );

    expect(result).toBe('{"status":"ok"}');
    expect(observedApiKey).toBe("test-secret");
    expect(observedTools).toEqual([]);
    expect(events).toContain("agent_start");
    expect(events).toContain("message_update:text_delta");
    expect(events.at(-1)).toBe("agent_end");
  });

  it("runs the production OpenAI-compatible adapter against a streaming endpoint", async () => {
    let observedPath = "";
    let observedAuthorization = "";
    let observedBody: Record<string, unknown> = {};
    const server = createServer((request, response) => {
      const chunks: Buffer[] = [];
      request.on("data", (chunk) => chunks.push(Buffer.from(chunk)));
      request.on("end", () => {
        observedPath = request.url ?? "";
        observedAuthorization = request.headers.authorization ?? "";
        observedBody = JSON.parse(Buffer.concat(chunks).toString("utf8"));
        response.writeHead(200, {
          "content-type": "text/event-stream",
          connection: "keep-alive",
        });
        response.write(
          `data: ${JSON.stringify({
            id: "chatcmpl-test",
            object: "chat.completion.chunk",
            created: 1,
            model: "custom-model",
            choices: [
              {
                index: 0,
                delta: { role: "assistant", content: "{\"status\":" },
                finish_reason: null,
              },
            ],
          })}\n\n`,
        );
        response.write(
          `data: ${JSON.stringify({
            id: "chatcmpl-test",
            object: "chat.completion.chunk",
            created: 1,
            model: "custom-model",
            choices: [
              {
                index: 0,
                delta: { content: "\"ok\"}" },
                finish_reason: "stop",
              },
            ],
          })}\n\n`,
        );
        response.end("data: [DONE]\n\n");
      });
    });
    await new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(0, "127.0.0.1", resolve);
    });
    const address = server.address() as AddressInfo;

    try {
      const result = await runPiAgentPrompt({
        prompt: "Return JSON",
        system: "System",
        providerConfig: {
          ...OPENAI_PROVIDER_CONFIG,
          baseUrl: `http://127.0.0.1:${address.port}/v1`,
        },
        timeoutMs: 5_000,
      });

      expect(result).toBe('{"status":"ok"}');
      expect(observedPath).toBe("/v1/chat/completions");
      expect(observedAuthorization).toBe("Bearer test-secret");
      expect(observedBody).toMatchObject({
        model: "custom-model",
        stream: true,
        messages: [
          { role: "system", content: "System" },
          {
            role: "user",
            content: [{ type: "text", text: "Return JSON" }],
          },
        ],
      });
      expect(observedBody).not.toHaveProperty("tools");
    } finally {
      await new Promise<void>((resolve, reject) => {
        server.close((error) => (error ? reject(error) : resolve()));
      });
    }
  });

  it("rejects a resolved Pi stream whose stopReason is error and redacts credentials", async () => {
    const faux = fauxProvider({ provider: "faux-error" });
    faux.setResponses([
      fauxAssistantMessage([], {
        stopReason: "error",
        errorMessage: "Rejected credential test-secret",
      }),
    ]);

    await expect(
      runPiAgentPrompt(
        {
          prompt: "Return JSON",
          system: "System",
          providerConfig: OPENAI_PROVIDER_CONFIG,
        },
        {
          createRuntime: () => fauxBinding(faux),
        },
      ),
    ).rejects.toEqual(
      expect.objectContaining({
        name: "PiAgentProviderError",
        message: "Rejected credential [redacted]",
      }),
    );
  });

  it("redacts credentials from unexpected runtime exceptions", async () => {
    const faux = fauxProvider({ provider: "faux-listener-error" });
    faux.setResponses([fauxAssistantMessage("unused")]);

    await expect(
      runPiAgentPrompt(
        {
          prompt: "Return JSON",
          system: "System",
          providerConfig: OPENAI_PROVIDER_CONFIG,
          onEvent(event) {
            if (event.type === "agent_start") {
              throw new Error("Unexpected runtime failure for test-secret");
            }
          },
        },
        {
          createRuntime: () => fauxBinding(faux),
        },
      ),
    ).rejects.toEqual(
      expect.objectContaining({
        name: "PiAgentProviderError",
        message: "Unexpected runtime failure for [redacted]",
      }),
    );
  });

  it.each([
    ["aborted", PiAgentProviderError],
    ["length", PiAgentStopReasonError],
  ] as const)("rejects a resolved Pi stream whose stopReason is %s", async (stopReason, errorType) => {
    const faux = fauxProvider({ provider: `faux-${stopReason}` });
    faux.setResponses([
      fauxAssistantMessage("partial", {
        stopReason,
        errorMessage: stopReason === "aborted" ? "Provider aborted" : undefined,
      }),
    ]);

    await expect(
      runPiAgentPrompt(
        {
          prompt: "Return JSON",
          system: "System",
          providerConfig: OPENAI_PROVIDER_CONFIG,
        },
        {
          createRuntime: () => fauxBinding(faux),
        },
      ),
    ).rejects.toBeInstanceOf(errorType);
  });

  it("maps user cancellation to AbortError", async () => {
    const faux = fauxProvider({
      provider: "faux-cancel",
      tokenSize: { min: 1, max: 1 },
    });
    faux.setResponses([fauxAssistantMessage("a response that streams")]);
    const controller = new AbortController();

    await expect(
      runPiAgentPrompt(
        {
          prompt: "Return JSON",
          system: "System",
          providerConfig: OPENAI_PROVIDER_CONFIG,
          signal: controller.signal,
          onEvent(event) {
            if (
              event.type === "message_update" &&
              event.assistantMessageEvent.type === "text_start"
            ) {
              controller.abort();
            }
          },
        },
        {
          createRuntime: () => fauxBinding(faux),
        },
      ),
    ).rejects.toMatchObject({
      name: "AbortError",
      message: "Agent task cancelled",
    });
  });

  it("enforces a configurable timeout without treating partial output as success", async () => {
    vi.useFakeTimers();
    const faux = fauxProvider({
      provider: "faux-timeout",
      tokensPerSecond: 1,
      tokenSize: { min: 1, max: 1 },
    });
    faux.setResponses([fauxAssistantMessage("this response cannot finish before the timeout")]);

    const pending = runPiAgentPrompt(
      {
        prompt: "Return JSON",
        system: "System",
        providerConfig: OPENAI_PROVIDER_CONFIG,
        timeoutMs: 25,
      },
      {
        createRuntime: () => fauxBinding(faux),
      },
    );
    const assertion = expect(pending).rejects.toBeInstanceOf(PiAgentTimeoutError);

    await vi.advanceTimersByTimeAsync(5_000);
    await assertion;
  });

  it("reads the default timeout from the runtime environment", () => {
    expect(
      resolvePiAgentTimeoutMs(undefined, {
        [PI_AGENT_TIMEOUT_ENV]: "4321",
      }),
    ).toBe(4_321);
    expect(() => resolvePiAgentTimeoutMs(0)).toThrow(/positive integer/i);
  });

  it("does not leak credentials or model state across concurrent tasks", async () => {
    const first = fauxProvider({ provider: "faux-first" });
    const second = fauxProvider({ provider: "faux-second" });
    const observations: Array<{ key?: string; model: string }> = [];
    first.setResponses([
      (_context, options, _state, model) => {
        observations.push({ key: options?.apiKey, model: model.id });
        return fauxAssistantMessage("first");
      },
    ]);
    second.setResponses([
      (_context, options, _state, model) => {
        observations.push({ key: options?.apiKey, model: model.id });
        return fauxAssistantMessage("second");
      },
    ]);

    const [firstResult, secondResult] = await Promise.all([
      runPiAgentPrompt(
        {
          prompt: "first",
          system: "System",
          providerConfig: OPENAI_PROVIDER_CONFIG,
        },
        {
          createRuntime: () => fauxBinding(first, "first-key"),
        },
      ),
      runPiAgentPrompt(
        {
          prompt: "second",
          system: "System",
          providerConfig: OPENAI_PROVIDER_CONFIG,
        },
        {
          createRuntime: () => fauxBinding(second, "second-key"),
        },
      ),
    ]);

    expect([firstResult, secondResult]).toEqual(["first", "second"]);
    expect(observations).toEqual(
      expect.arrayContaining([
        { key: "first-key", model: first.getModel().id },
        { key: "second-key", model: second.getModel().id },
      ]),
    );
  });
});
