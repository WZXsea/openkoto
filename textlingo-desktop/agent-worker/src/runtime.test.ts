import { describe, expect, it, vi } from "vitest";

import {
  createTaskErrorEvent,
  createTaskStartedEvent,
  createErrorEvent,
  createHeartbeatEvent,
  createProgressEvent,
  createResultEvent,
  executeAgentRunRequest,
  handleAgentRunRequest,
  parseWorkerRequest,
  parseAgentRunRequest,
  validateMindMapResult,
} from "./runtime.js";

describe("runtime", () => {
  it("parses a task request payload", () => {
    const request = parseWorkerRequest(
      JSON.stringify({
        id: "req-1",
        type: "request",
        method: "task.start",
        params: {
          task_id: "task-1",
          task_type: "mind_map.generate",
          payload: {
            article_id: "article-1",
          },
        },
      }),
    );

    expect(request.method).toBe("task.start");
    expect(request.params.task_id).toBe("task-1");
    expect(request.params.payload.article_id).toBe("article-1");
  });

  it("creates progress events with normalized payload", () => {
    const event = createProgressEvent("task-1", "reading", 0.4, "Reading source");

    expect(event.type).toBe("event");
    expect(event.event).toBe("task.progress");
    expect(event.payload.task_id).toBe("task-1");
    expect(event.payload.progress).toBe(0.4);
  });

  it("creates heartbeat events", () => {
    const event = createHeartbeatEvent("worker-1");

    expect(event.type).toBe("event");
    expect(event.event).toBe("worker.heartbeat");
    expect(event.payload.worker_session_id).toBe("worker-1");
  });

  it("creates result events", () => {
    const event = createResultEvent("task-1", { status: "applicable" });

    expect(event.event).toBe("task.result");
    expect(event.payload.task_id).toBe("task-1");
  });

  it("creates error events", () => {
    const event = createErrorEvent("task-1", "boom");

    expect(event.event).toBe("task.error");
    expect(event.payload.message).toBe("boom");
  });

  it("rejects invalid mind map results", () => {
    expect(() =>
      validateMindMapResult({
        status: "applicable",
        map: null,
      }),
    ).toThrow(/diagnostics/i);
  });

  it("parses an agent.run request", () => {
    const request = parseAgentRunRequest(
      JSON.stringify({
        id: "req-2",
        type: "request",
        method: "agent.run",
        params: {
          task_id: "task-2",
          task_type: "mind_map.generate",
          provider_config: {
            kind: "native_google",
            provider: "google",
            model: "gemini-2.0-flash-exp",
            api_key: "secret",
          },
          input: {
            article_id: "article-1",
            display_language: "zh-CN",
            max_depth: 3,
            mode: "balanced",
            article_snapshot: {
              title: "Sample Article",
              content: "Alpha beta gamma.",
              source_type: "article",
            },
          },
        },
      }),
    );

    expect(request.method).toBe("agent.run");
    expect(request.params.task_type).toBe("mind_map.generate");
    if (request.params.task_type !== "mind_map.generate") {
      throw new Error("Expected a mind map request");
    }
    expect(request.params.input.article_id).toBe("article-1");
  });

  it("emits task.started before executing an agent run request", async () => {
    const writes: unknown[] = [];

    await handleAgentRunRequest(
      parseAgentRunRequest(
        JSON.stringify({
          id: "req-2",
          type: "request",
          method: "agent.run",
          params: {
            task_id: "task-2",
            task_type: "mind_map.generate",
            provider_config: {
              kind: "native_google",
              provider: "google",
              model: "gemini-2.0-flash-exp",
              api_key: "secret",
            },
            input: {
              article_id: "article-1",
              display_language: "zh-CN",
              max_depth: 3,
              mode: "balanced",
              article_snapshot: {
                title: "Sample Article",
                content: "Alpha beta gamma.",
                source_type: "article",
              },
            },
          },
        }),
      ),
      {
        writeEvent: (event) => writes.push(event),
        runTask: async () => undefined,
      },
    );

    expect(writes[0]).toMatchObject({
      type: "event",
      event: "task.started",
      payload: {
        task_id: "task-2",
        task_type: "mind_map.generate",
      },
    });
  });

  it("emits a normalized task.error for unsupported providers", async () => {
    const writes: unknown[] = [];

    await handleAgentRunRequest(
      parseAgentRunRequest(
        JSON.stringify({
          id: "req-3",
          type: "request",
          method: "agent.run",
          params: {
            task_id: "task-3",
            task_type: "mind_map.generate",
            provider_config: {
              kind: "unsupported",
              provider: "weird-provider",
              reason: "Provider weird-provider is not supported for the agent runtime",
            },
            input: {
              article_id: "article-1",
              display_language: "zh-CN",
              max_depth: 3,
              mode: "balanced",
              article_snapshot: {
                title: "Sample Article",
                content: "Alpha beta gamma.",
                source_type: "article",
              },
            },
          },
        }),
      ),
      {
        writeEvent: (event) => writes.push(event),
        runTask: async () => undefined,
      },
    );

    expect(writes[0]).toMatchObject({
      type: "event",
      event: "task.started",
      payload: {
        task_id: "task-3",
        task_type: "mind_map.generate",
      },
    });
    expect(writes[1]).toMatchObject({
      type: "event",
      event: "task.error",
      payload: {
        task_id: "task-3",
        code: "provider_unsupported",
        message: "Provider is not supported for the agent runtime",
        details: "Provider weird-provider is not supported for the agent runtime",
      },
    });
    expect((writes[1] as { payload: { timestamp: string } }).payload.timestamp).toEqual(
      expect.any(String),
    );
  });

  it("dispatches assistant.agent_turn requests to the assistant runner", async () => {
    const runAssistantTask = vi.fn(async () => ({
      reply: "done",
      action: null,
    }));

    await executeAgentRunRequest(
      parseAgentRunRequest(
        JSON.stringify({
          id: "req-4",
          type: "request",
          method: "agent.run",
          params: {
            task_id: "task-4",
            task_type: "assistant.agent_turn",
            provider_config: {
              kind: "native_google",
              provider: "google",
              model: "gemini-2.0-flash-exp",
              api_key: "secret",
            },
            input: {
              user_message: "列出 PDF",
              conversation: [],
              ui_context: {
                current_article_id: "article-1",
                display_language: "zh-CN",
              },
              current_material: {
                id: "article-1",
                title: "Current PDF",
                material_type: "pdf",
                created_at: "2026-03-08T00:00:00Z",
                translated: false,
              },
              available_materials: [],
            },
          },
        }),
      ),
      {
        runAssistantTask,
        reportProgress: async () => undefined,
        saveArtifact: async () => ({ artifact_id: "unused" }),
      },
    );

    expect(runAssistantTask).toHaveBeenCalledTimes(1);
  });
});
