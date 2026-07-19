import { existsSync, readFileSync } from "node:fs";
import { mkdtempSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it, vi } from "vitest";

import {
  OPENAI_COMPATIBLE_REQUEST_TIMEOUT_MS,
  buildMindMapWorkspaceFiles,
  findAvailablePort,
  normalizeMindMapResult,
  resolveProviderModel,
  runAgentPrompt,
  runMindMapTask,
  runOpenAICompatiblePrompt,
} from "./mindMapTask.js";

describe("mindMapTask", () => {
  it("registers native google models explicitly for OpenCode", () => {
    const resolved = resolveProviderModel({
      kind: "native_google",
      provider: "google-ai-studio",
      model: "models/gemini-3-flash-preview",
      api_key: "secret",
    });

    expect(resolved.model).toBe("google/models/gemini-3-flash-preview");
    expect(resolved.config.enabled_providers).toEqual(["google"]);
    expect(resolved.config.plugin).toEqual([]);
    expect(resolved.config.autoupdate).toBe(false);
    expect(resolved.config.provider?.google?.models?.["models/gemini-3-flash-preview"]).toMatchObject({
      id: "models/gemini-3-flash-preview",
      name: "models/gemini-3-flash-preview",
    });
  });

  it("builds workspace files from the article snapshot", () => {
    const files = buildMindMapWorkspaceFiles({
      taskId: "task-1",
      articleId: "article-1",
      displayLanguage: "zh-CN",
      maxDepth: 3,
      mode: "balanced",
      articleSnapshot: {
        title: "Sample",
        content: "Alpha beta gamma.",
        sourceType: "article",
      },
    });

    expect(files["article-source.json"]).toContain("\"title\": \"Sample\"");
    expect(files["article-source.json"]).toContain("\"content\": \"Alpha beta gamma.\"");
    expect(files["TASK.md"]).toContain("article-source.json");
    expect(files["TASK.md"]).toContain("zh-CN");
  });

  it("allocates an ephemeral port for OpenCode server startup", async () => {
    const port = await findAvailablePort();
    expect(port).toBeGreaterThan(0);
  });

  it("calls OpenAI-compatible chat completions without starting OpenCode", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          choices: [
            {
              message: {
                content: "```json\n{\"status\":\"not_applicable\",\"map\":null,\"diagnostics\":{\"content_type\":\"unknown\",\"coverage\":\"none\",\"notes\":[],\"window_count\":1,\"evidence_density\":0,\"low_confidence_node_ids\":[]}}\n```",
              },
            },
          ],
        }),
        {
          status: 200,
          headers: { "content-type": "application/json" },
        },
      ),
    );

    const providerConfig = {
      kind: "openai_compatible" as const,
      provider: "openai-compatible",
      model: "custom-model",
      api_key: "test-secret",
      baseUrl: "https://models.example.test/v1/",
    };
    const result = await runAgentPrompt({
      cwd: "/tmp/unused",
      model: "textlingo_openai_compatible/custom-model",
      prompt: "Return JSON",
      system: "System",
      config: {},
      providerConfig,
    });

    expect(result).toMatchObject({ status: "not_applicable" });
    expect(fetchMock).toHaveBeenCalledWith(
      "https://models.example.test/v1/chat/completions",
      expect.objectContaining({
        method: "POST",
        headers: expect.objectContaining({
          authorization: "Bearer test-secret",
        }),
      }),
    );
    const request = fetchMock.mock.calls[0][1];
    expect(JSON.parse(String(request?.body))).toMatchObject({
      model: "custom-model",
      messages: [
        { role: "system", content: "System" },
        { role: "user", content: "Return JSON" },
      ],
    });
    fetchMock.mockRestore();
  });

  it("redacts the API key from OpenAI-compatible provider errors", async () => {
    const fetchMock = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          error: {
            message: "Rejected credential test-secret",
          },
        }),
        {
          status: 401,
          headers: { "content-type": "application/json" },
        },
      ),
    );

    await expect(
      runOpenAICompatiblePrompt({
        cwd: "/tmp/unused",
        model: "textlingo_openai_compatible/custom-model",
        prompt: "Return JSON",
        system: "System",
        config: {},
        providerConfig: {
          kind: "openai_compatible",
          provider: "openai-compatible",
          model: "custom-model",
          api_key: "test-secret",
          baseUrl: "https://models.example.test/v1",
        },
      }),
    ).rejects.toThrow("Rejected credential [redacted]");
    fetchMock.mockRestore();
  });

  it("aborts an OpenAI-compatible request after the runtime timeout", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.spyOn(globalThis, "fetch").mockImplementation(
      (_input, init) =>
        new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener(
            "abort",
            () => reject(new DOMException("Aborted", "AbortError")),
            { once: true },
          );
        }),
    );

    const pending = runOpenAICompatiblePrompt({
      cwd: "/tmp/unused",
      model: "textlingo_openai_compatible/custom-model",
      prompt: "Return JSON",
      system: "System",
      config: {},
      providerConfig: {
        kind: "openai_compatible",
        provider: "openai-compatible",
        model: "custom-model",
        api_key: "test-secret",
        baseUrl: "https://models.example.test/v1",
      },
    });
    const assertion = expect(pending).rejects.toThrow(
      "OpenAI-compatible request timed out after 120 seconds",
    );

    await vi.advanceTimersByTimeAsync(OPENAI_COMPATIBLE_REQUEST_TIMEOUT_MS);
    await assertion;
    fetchMock.mockRestore();
    vi.useRealTimers();
  });

  it("fails fast for providers without a built-in direct runtime", async () => {
    await expect(
      runAgentPrompt({
        cwd: "/tmp/unused",
        model: "google/gemini-2.0-flash-exp",
        prompt: "Return JSON",
        system: "System",
        config: {},
        providerConfig: {
          kind: "native_google",
          provider: "google",
          model: "gemini-2.0-flash-exp",
          api_key: "test-secret",
        },
      }),
    ).rejects.toThrow(
      "Built-in direct runtime does not support provider kind native_google; configure an OpenAI-compatible provider",
    );
  });

  it("normalizes partial model output into a schema-valid result", () => {
    const normalized = normalizeMindMapResult(
      {
        status: "applicable",
        map: {
          root: {
            title: "Main thread",
            children: [],
          },
        },
        diagnostics: {},
      },
      {
        taskId: "task-1",
        articleId: "article-1",
        displayLanguage: "zh-CN",
        maxDepth: 3,
        mode: "balanced",
        articleSnapshot: {
          title: "Sample",
          content: "Alpha beta gamma.",
          sourceType: "article",
        },
      },
    );

    expect(normalized).toMatchObject({
      status: "applicable",
      map: {
        version: "1",
        article_id: "article-1",
        title: "Sample",
        display_language: "zh-CN",
        generation_mode: "evidence_first",
        summary: expect.any(String),
        root: {
          id: "root",
          title: "Main thread",
          node_type: "root",
          children: [],
        },
      },
      diagnostics: {
        content_type: "article",
        coverage: "full",
        window_count: 1,
        evidence_density: 0,
      },
    });
  });

  it("runs the OpenCode prompt runner in a temporary workspace and saves the result", async () => {
    const saveArtifact = vi.fn(async () => ({ artifact_id: "artifact-1" }));
    const reportProgress = vi.fn(async () => undefined);
    const log = vi.fn();
    const workspaceRoot = mkdtempSync(join(tmpdir(), "mind-map-task-test-"));
    const promptRunner = vi.fn(async ({ cwd, model }: { cwd: string; model: string }) => {
      expect(model).toBe("google/gemini-2.0-flash-exp");
      expect(existsSync(cwd)).toBe(true);
      expect(readFileSync(join(cwd, "article-source.json"), "utf8")).toContain("Alpha beta gamma.");
      expect(readFileSync(join(cwd, "TASK.md"), "utf8")).toContain("article-source.json");
      return {
        status: "applicable",
        reason: null,
        map: {
          version: "1",
          article_id: "article-1",
          title: "Sample",
          display_language: "zh-CN",
          generation_mode: "evidence_first",
          source_hash: "sha256:abc",
          summary: "Overview",
          root: {
            id: "root",
            title: "Root",
            node_type: "root",
            summary: "Summary",
            confidence: 0.9,
            source_segment_ids: ["seg-1"],
            source_offsets: [],
            children: [],
          },
        },
        diagnostics: {
          content_type: "narrative",
          coverage: "full",
          notes: [],
          window_count: 1,
          evidence_density: 1,
          low_confidence_node_ids: [],
        },
      };
    });

    const result = await runMindMapTask(
      {
        taskId: "task-1",
        articleId: "article-1",
        displayLanguage: "zh-CN",
        maxDepth: 3,
        mode: "balanced",
        articleSnapshot: {
          title: "Sample",
          content: "Alpha beta gamma.",
          sourceType: "article",
        },
      },
      {
        promptRunner,
        saveArtifact,
        reportProgress,
        log,
        workspaceRoot,
        providerConfig: {
          kind: "native_google",
          provider: "google",
          model: "gemini-2.0-flash-exp",
          api_key: "secret",
        },
      },
    );

    expect(promptRunner).toHaveBeenCalledTimes(1);
    expect(saveArtifact).toHaveBeenCalledWith(
      "task-1",
      "mind_map",
      expect.objectContaining({ status: "applicable" }),
    );
    expect(reportProgress.mock.calls).toEqual([
      ["task-1", "planning", 0.1, "Preparing mind map task"],
      ["task-1", "starting_agent", 0.2, "Starting agent runtime"],
      ["task-1", "analyzing", 0.35, "Agent runtime is analyzing the source"],
      ["task-1", "validating", 0.75, "Validating mind map output"],
      ["task-1", "saving", 0.9, "Saving mind map artifact"],
    ]);
    expect(log.mock.calls).toEqual([
      ["info", expect.stringContaining("Prepared task workspace:"), "recipe"],
      ["info", "Starting mind map model request", "provider"],
      ["info", "Mind map model returned a final result", "provider"],
      ["info", "Mind map result validated", "recipe"],
      ["info", "Mind map artifact saved: artifact-1", "runtime"],
    ]);
    expect(readdirSync(workspaceRoot)).toEqual([]);
    expect(result.artifact_id).toBe("artifact-1");
  });
});
