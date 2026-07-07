import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { featureBoundaries } from "../features";
import * as shared from ".";
import { cn } from ".";

const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const runtimeBoundaryRoots = ["app", "features", "shared"].map((dir) => path.join(srcRoot, dir));

function collectRuntimeSourceFiles(directory: string): string[] {
  return readdirSync(directory).flatMap((entry) => {
    const entryPath = path.join(directory, entry);
    const stat = statSync(entryPath);

    if (stat.isDirectory()) return collectRuntimeSourceFiles(entryPath);
    if (entry.endsWith(".test.ts") || entry.endsWith(".test.tsx") || entry.endsWith(".md")) return [];
    if (!/\.(ts|tsx)$/.test(entry)) return [];
    return [entryPath];
  });
}

describe("PR-3 directory boundaries", () => {
  it("keeps stable feature entries for Phase 1", () => {
    expect(featureBoundaries.map((feature) => feature.id)).toEqual([
      "materials",
      "reader",
      "notes",
      "local-learning",
      "assistant",
      "settings",
      "media",
      "books",
    ]);
  });

  it("keeps feature ids unique and legacy component paths real", () => {
    const ids = featureBoundaries.map((feature) => feature.id);

    expect(new Set(ids).size).toBe(ids.length);

    for (const feature of featureBoundaries) {
      for (const legacyPath of feature.legacyComponents) {
        expect(existsSync(path.join(srcRoot, legacyPath.replace(/^src\//, "")))).toBe(true);
      }
    }
  });

  it("re-exports shared lib helpers from the shared barrel", () => {
    expect(cn("alpha", false && "beta", "gamma")).toBe("alpha gamma");
  });

  it("does not expose app or assistant event hooks from shared", () => {
    expect("useConfig" in shared).toBe(true);
    expect("useAgentOpenMaterialListener" in shared).toBe(false);
  });

  it("keeps app, feature, and shared runtime files free of external connector entrypoints", () => {
    const runtimeSource = runtimeBoundaryRoots
      .flatMap(collectRuntimeSourceFiles)
      .map((filePath) => readFileSync(filePath, "utf-8"))
      .join("\n");

    expect(runtimeSource).not.toMatch(/from\s+["']@tauri-apps\/plugin-shell["']/);
    expect(runtimeSource).not.toMatch(/shell:allow-execute|shell:default/);
    expect(runtimeSource).not.toMatch(/invoke(?:<[^>]+>)?\(\s*["'](?:anki|zotero|mineru|language_tool|mcp)/i);
    expect(runtimeSource).not.toMatch(/\bAnkiConnect\b|\bLanguageTool\b/);
  });
});
