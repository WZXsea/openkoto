import { existsSync, mkdtempSync, readFileSync, readdirSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it, vi } from "vitest";

import {
  buildMindMapWorkspaceFiles,
  normalizeMindMapResult,
  runMindMapTask,
} from "./mindMapTask.js";

describe("mindMapTask", () => {
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

  it("runs the Pi prompt runner in a temporary workspace and saves the result", async () => {
    const saveArtifact = vi.fn(async () => ({ artifact_id: "artifact-1" }));
    const reportProgress = vi.fn(async () => undefined);
    const log = vi.fn();
    const workspaceRoot = mkdtempSync(join(tmpdir(), "mind-map-task-test-"));
    const promptRunner = vi.fn(async ({ cwd, providerConfig }: {
      cwd?: string;
      providerConfig: { kind: string };
    }) => {
      expect(providerConfig.kind).toBe("native_google");
      expect(cwd).toBeTruthy();
      expect(existsSync(cwd!)).toBe(true);
      expect(readFileSync(join(cwd!, "article-source.json"), "utf8")).toContain(
        "Alpha beta gamma.",
      );
      expect(readFileSync(join(cwd!, "TASK.md"), "utf8")).toContain("article-source.json");
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

  it("does not save a partial artifact after user cancellation", async () => {
    const controller = new AbortController();
    const saveArtifact = vi.fn(async () => ({ artifact_id: "should-not-save" }));
    const workspaceRoot = mkdtempSync(join(tmpdir(), "mind-map-cancel-test-"));

    await expect(
      runMindMapTask(
        {
          taskId: "task-cancel",
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
          promptRunner: vi.fn(async () => {
            controller.abort();
            return JSON.stringify({
              status: "not_applicable",
              map: null,
              diagnostics: {
                content_type: "unknown",
                coverage: "none",
                notes: [],
                window_count: 1,
                evidence_density: 0,
                low_confidence_node_ids: [],
              },
            });
          }),
          saveArtifact,
          reportProgress: vi.fn(async () => undefined),
          workspaceRoot,
          providerConfig: {
            kind: "native_google",
            provider: "google",
            model: "gemini-2.0-flash-exp",
            api_key: "secret",
          },
          signal: controller.signal,
        },
      ),
    ).rejects.toMatchObject({ name: "AbortError" });

    expect(saveArtifact).not.toHaveBeenCalled();
    expect(readdirSync(workspaceRoot)).toEqual([]);
  });
});
