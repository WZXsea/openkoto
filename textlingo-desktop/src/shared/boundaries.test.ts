import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

import { featureBoundaries } from "../features";
import * as shared from ".";
import { cn } from ".";

const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const phase1FrontendRuntimeRoots = ["app", "features", "shared"].map((dir) => path.join(srcRoot, dir));
const phase1DirectExternalConnectorPatterns = [
  /from\s+["']@tauri-apps\/plugin-shell["']/,
  /import\(\s*["']@tauri-apps\/plugin-shell["']\s*\)/,
  /require\(\s*["']@tauri-apps\/plugin-shell["']\s*\)/,
  /shell:allow-execute|shell:default/,
  /invoke(?:<[^>]+>)?\(\s*["'][^"']*(?:anki|zotero|mineru|language[_-]?tool|mcp)[^"']*["']/i,
  /\b(?:fetch|EventSource|WebSocket)\(\s*["'][^"']*(?:127\.0\.0\.1:8765|localhost:8765|anki|zotero|mineru|languagetool|language-tool|mcp)[^"']*["']/i,
  /\bAnkiConnect\b|\bLanguageTool\b|\bFastMCP\b/,
];

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

  it("keeps Phase 1 frontend runtime files free of direct external connector entrypoints", () => {
    const runtimeSource = phase1FrontendRuntimeRoots
      .flatMap(collectRuntimeSourceFiles)
      .map((filePath) => readFileSync(filePath, "utf-8"))
      .join("\n");

    for (const pattern of phase1DirectExternalConnectorPatterns) {
      expect(runtimeSource).not.toMatch(pattern);
    }
  });
});
