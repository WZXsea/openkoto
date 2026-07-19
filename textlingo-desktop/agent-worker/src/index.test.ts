import { describe, expect, it, vi } from "vitest";

import { createWorkerHost } from "./index.js";

describe("worker host", () => {
  it("emits worker.ready on startup", () => {
    const events: unknown[] = [];

    createWorkerHost({
      workerSessionId: "worker-1",
      version: "0.1.0",
      writeEvent: (event) => events.push(event),
      runAgentTask: async () => undefined,
    });

    expect(events[0]).toMatchObject({
      type: "event",
      event: "worker.ready",
      payload: {
        worker_session_id: "worker-1",
        runtime: "pi-agent-core",
        version: "0.1.0",
      },
    });
  });

  it("handles agent.run lines through the new runtime entry", async () => {
    const events: unknown[] = [];
    const runAgentTask = vi.fn(async () => undefined);

    const host = createWorkerHost({
      workerSessionId: "worker-1",
      version: "0.1.0",
      writeEvent: (event) => events.push(event),
      runAgentTask,
    });

    await host.handleLine(
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

    expect(runAgentTask).toHaveBeenCalledTimes(1);
    expect(events.some((event: any) => event.event === "task.started")).toBe(true);
  });

  it("accepts assistant.agent_turn lines through the runtime entry", async () => {
    const events: unknown[] = [];
    const runAgentTask = vi.fn(async () => undefined);

    const host = createWorkerHost({
      workerSessionId: "worker-1",
      version: "0.1.0",
      writeEvent: (event) => events.push(event),
      runAgentTask,
    });

    await host.handleLine(
      JSON.stringify({
        id: "req-2",
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
            user_message: "查看当前素材",
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
    );

    expect(runAgentTask).toHaveBeenCalledTimes(1);
    expect(events.some((event: any) => event.event === "task.started")).toBe(true);
  });

  it("cancels only the requested running task", async () => {
    const events: any[] = [];
    let releaseOtherTask: (() => void) | undefined;
    const runAgentTask = vi.fn(async (request: any, signal?: AbortSignal) => {
      if (request.params.task_id === "task-cancel") {
        await new Promise<void>((resolve) => {
          signal?.addEventListener("abort", () => resolve(), { once: true });
        });
        if (signal?.aborted) {
          const error = new Error("cancelled");
          error.name = "AbortError";
          throw error;
        }
        return;
      }
      await new Promise<void>((resolve) => {
        releaseOtherTask = resolve;
      });
    });
    const host = createWorkerHost({
      workerSessionId: "worker-1",
      version: "0.1.0",
      writeEvent: (event) => events.push(event),
      runAgentTask,
    });

    const makeRun = (taskId: string) => JSON.stringify({
      id: `req-${taskId}`,
      type: "request",
      method: "agent.run",
      params: {
        task_id: taskId,
        task_type: "assistant.agent_turn",
        provider_config: {
          kind: "native_google",
          provider: "google",
          model: "gemini-2.0-flash-exp",
          api_key: "secret",
        },
        input: {
          user_message: "查看当前素材",
          conversation: [],
          ui_context: { display_language: "zh-CN" },
          current_material: null,
          available_materials: [],
        },
      },
    });

    const cancelledRun = host.handleLine(makeRun("task-cancel"));
    const otherRun = host.handleLine(makeRun("task-other"));
    await Promise.resolve();
    await host.handleLine(JSON.stringify({
      id: "cancel-1",
      type: "request",
      method: "agent.cancel",
      params: { task_id: "task-cancel" },
    }));
    await cancelledRun;
    expect(events).toContainEqual(expect.objectContaining({
      event: "task.error",
      payload: expect.objectContaining({
        task_id: "task-cancel",
        code: "task_cancelled",
      }),
    }));
    expect(events.some((event) =>
      event.event === "task.error" && event.payload?.task_id === "task-other"
    )).toBe(false);

    releaseOtherTask?.();
    await otherRun;
  });
});
