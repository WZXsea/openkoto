import { describe, expect, it } from "vitest";

import {
  SOURCE_LOCATOR_VERSION,
  assertValidSourceLocator,
  createTextRangeLocator,
  parseSourceLocator,
  withLocatorVersion,
  type SourceLocatorV1,
} from "./sourceLocator";

describe("SourceLocator v1", () => {
  it("upgrades a legacy reading locator without changing its anchor", () => {
    const locator = withLocatorVersion({
      kind: "segment",
      segment_order: 2,
      total_segments: 5,
      segment_id: "segment-2",
    });

    expect(locator).toEqual({
      version: SOURCE_LOCATOR_VERSION,
      kind: "segment",
      segment_order: 2,
      total_segments: 5,
      segment_id: "segment-2",
    });
    expect(() => assertValidSourceLocator(locator)).not.toThrow();
  });

  it("creates a stable text range with quote and material identity", () => {
    const locator = createTextRangeLocator({
      segmentId: "segment-2",
      startOffset: 4,
      endOffset: 12,
      exact: "language",
      prefix: "learn ",
      suffix: " well",
      materialRevision: "revision-1",
      contentSha256: "A".repeat(64),
    });

    expect(locator).toMatchObject({
      version: 1,
      kind: "text_range",
      start_offset: 4,
      end_offset: 12,
      material_revision: "revision-1",
      content_sha256: "a".repeat(64),
      quote: { exact: "language", prefix: "learn ", suffix: " well" },
    });
  });

  it("accepts legacy cfi/time discriminator aliases", () => {
    const aliases: SourceLocatorV1[] = [
      { version: 1, kind: "cfi", cfi: "epubcfi(/6/2!/4/1:0)" },
      { version: 1, kind: "time_range", current_time: 4, end_time: 8, duration: 30 },
    ];
    aliases.forEach((locator) => expect(() => assertValidSourceLocator(locator)).not.toThrow());
  });

  it("parses legacy progress and rejects unknown locator shapes", () => {
    expect(parseSourceLocator({ kind: "page", page: 3, total_pages: 10 })).toEqual({
      kind: "page",
      page: 3,
      total_pages: 10,
    });
    expect(parseSourceLocator({ kind: "unknown", page: 3 })).toBeUndefined();
    expect(parseSourceLocator({ kind: "segment", segment_order: 0 })).toBeUndefined();
  });

  it("rejects invalid ranges and hashes", () => {
    expect(() => createTextRangeLocator({ startOffset: 8, endOffset: 4, exact: "invalid" })).toThrow();
    expect(() => assertValidSourceLocator({
      version: 1,
      kind: "page",
      page: 1,
      content_sha256: "invalid",
    })).toThrow("content_sha256");
  });
});
