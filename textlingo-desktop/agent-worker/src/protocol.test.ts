import { describe, expect, it } from "vitest";

import {
  createTaskErrorEvent,
  createTaskLogEvent,
  createTaskStartedEvent,
  createWorkerReadyEvent,
  parseAgentCancelRequest,
  parseAgentRunRequest,
} from "./protocol.js";

describe("protocol", () => {
  it("parses an agent.run request with provider config", () => {
    const request = parseAgentRunRequest(
      JSON.stringify({
        id: "req-1",
        type: "request",
        method: "agent.run",
        params: {
          task_id: "task-1",
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
    expect(request.params.provider_config.kind).toBe("native_google");
    expect(request.params.task_type).toBe("mind_map.generate");
    if (request.params.task_type !== "mind_map.generate") {
      throw new Error("Expected a mind map request");
    }
    expect(request.params.input.article_id).toBe("article-1");
    expect(request.params.input.article_snapshot.title).toBe("Sample Article");
  });

  it("creates worker and task lifecycle events", () => {
    const ready = createWorkerReadyEvent("worker-1", "opencode", "0.1.0");
    const started = createTaskStartedEvent("task-1", "mind_map.generate");
    const log = createTaskLogEvent("task-1", "info", "provider", "Gemini request started");
    const error = createTaskErrorEvent("task-1", "provider_auth_error", "Authentication failed", "bad key");

    expect(ready.event).toBe("worker.ready");
    expect(started.event).toBe("task.started");
    expect(log.payload.source).toBe("provider");
    expect(error.payload.code).toBe("provider_auth_error");
  });

  it("parses an assistant.agent_turn request", () => {
    const request = parseAgentRunRequest(
      JSON.stringify({
        id: "req-agent-1",
        type: "request",
        method: "agent.run",
        params: {
          task_id: "task-agent-1",
          task_type: "assistant.agent_turn",
          provider_config: {
            kind: "native_google",
            provider: "google",
            model: "gemini-2.0-flash-exp",
            api_key: "secret",
          },
          input: {
            user_message: "打开标题带 N1 的 PDF",
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
            available_materials: [
              {
                id: "article-2",
                title: "N1 PDF",
                material_type: "pdf",
                created_at: "2026-03-08T00:00:00Z",
                translated: true,
              },
            ],
          },
        },
      }),
    );

    expect(request.params.task_type).toBe("assistant.agent_turn");
    if (request.params.task_type !== "assistant.agent_turn") {
      throw new Error("Expected an assistant request");
    }
    expect(request.params.input.user_message).toBe("打开标题带 N1 的 PDF");
    expect(request.params.input.available_materials).toHaveLength(1);
  });

  it("parses a task-scoped agent.cancel request", () => {
    const request = parseAgentCancelRequest(
      JSON.stringify({
        id: "cancel-1",
        type: "request",
        method: "agent.cancel",
        params: { task_id: "task-agent-1" },
      }),
    );

    expect(request.method).toBe("agent.cancel");
    expect(request.params.task_id).toBe("task-agent-1");
  });
});
