import { describe, expect, it } from "vitest";

import { featureBoundaries } from "../features";
import { cn } from ".";

describe("PR-2 directory boundaries", () => {
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

  it("re-exports shared lib helpers from the shared barrel", () => {
    expect(cn("alpha", false && "beta", "gamma")).toBe("alpha gamma");
  });
});
