import { createInterface } from "node:readline";
import { randomUUID } from "node:crypto";
import { join } from "node:path";
import { tmpdir } from "node:os";

import {
  createTaskErrorEvent,
  createTaskLogEvent,
  createTaskProgressEvent,
  createTaskResultEvent,
  createWorkerHeartbeatEvent,
  createWorkerReadyEvent,
  parseAgentCancelRequest,
  parseAgentRunRequest,
} from "./protocol.js";
import { runPiAgentPrompt } from "./piRuntime.js";
import { executeAgentRunRequest, handleAgentRunRequest, parseWorkerRequest } from "./runtime.js";

function writeEvent(event: unknown) {
  process.stdout.write(`${JSON.stringify(event)}\n`);
}

export function createWorkerHost(deps: {
  workerSessionId: string;
  version: string;
  writeEvent: (event: unknown) => void;
  runAgentTask: (
    request: ReturnType<typeof parseAgentRunRequest>,
    signal?: AbortSignal,
  ) => Promise<void>;
}) {
  const activeTasks = new Map<string, AbortController>();
  deps.writeEvent(createWorkerReadyEvent(deps.workerSessionId, "pi-agent-core", deps.version));

  return {
    emitHeartbeat() {
      deps.writeEvent(createWorkerHeartbeatEvent(deps.workerSessionId));
    },
    async handleLine(rawLine: string) {
      if (!rawLine.trim()) {
        return;
      }

      try {
        const cancel = parseAgentCancelRequest(rawLine);
        activeTasks.get(cancel.params.task_id)?.abort();
        return;
      } catch {
        // Continue with task request parsing.
      }

      try {
        const request = parseAgentRunRequest(rawLine);
        const taskId = request.params.task_id;
        if (activeTasks.has(taskId)) {
          deps.writeEvent(
            createTaskErrorEvent(
              taskId,
              "duplicate_task",
              "Agent task is already running",
            ),
          );
          return;
        }
        const controller = new AbortController();
        activeTasks.set(taskId, controller);
        try {
          await handleAgentRunRequest(request, {
            writeEvent: deps.writeEvent,
            runTask: deps.runAgentTask,
            signal: controller.signal,
          });
        } finally {
          if (activeTasks.get(taskId) === controller) {
            activeTasks.delete(taskId);
          }
        }
        return;
      } catch {
        // Fall through to legacy request parsing while the old runtime is still being removed.
      }

      try {
        parseWorkerRequest(rawLine);
      } catch (error) {
        deps.writeEvent(
          createTaskErrorEvent(
            "unknown-task",
            "internal_error",
            "Failed to parse worker request",
            error instanceof Error ? error.message : String(error),
          ),
        );
        return;
      }

      deps.writeEvent(
        createTaskErrorEvent(
          "unknown-task",
          "internal_error",
          "Legacy task.start runtime has not been migrated in this host",
        ),
      );
    },
  };
}

async function main() {
  const workerSessionId = randomUUID();
  const host = createWorkerHost({
    workerSessionId,
    version: "0.1.0",
    writeEvent,
    async runAgentTask(request, signal) {
      await executeAgentRunRequest(request, {
        promptRunner: runPiAgentPrompt,
        workspaceRoot: join(tmpdir(), "textlingo-agent-worker"),
        async reportProgress(taskId, stage, progress, message) {
          writeEvent(createTaskProgressEvent(taskId, stage, progress, message));
        },
        async saveArtifact(taskId, artifactType, content) {
          writeEvent(createTaskResultEvent(taskId, content, artifactType));
          return {
            artifact_id: `${taskId}:${artifactType}`,
          };
        },
        log(level, message, source = "runtime") {
          writeEvent(createTaskLogEvent(request.params.task_id, level, source, message));
        },
        writeEvent(event) {
          writeEvent(event);
        },
        signal,
      });
    },
  });

  host.emitHeartbeat();
  const heartbeatTimer = setInterval(() => {
    host.emitHeartbeat();
  }, 15_000);

  const rl = createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  });

  rl.on("line", (line) => {
    void host.handleLine(line);
  });

  rl.on("close", () => {
    clearInterval(heartbeatTimer);
    process.exit(0);
  });
}

if (!process.env.VITEST) {
  void main();
}
