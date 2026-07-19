import { describe, expect, it } from "vitest";

import {
  createAnnotationDraft,
  resolveAnnotation,
  toSourceLocator,
  type ReaderAnnotationReference,
} from "./annotationLocator";

describe("reader annotation locator", () => {
  it("resolves article segment text ranges exactly", () => {
    const annotation: ReaderAnnotationReference = {
      reader_kind: "article",
      locator: {
        reader_kind: "article",
        kind: "text_range",
        segment_id: "s-1",
        segment_order: 0,
        start_offset: 6,
        end_offset: 14,
        quote: { exact: "language" },
      },
    };

    expect(resolveAnnotation(annotation, {
      reader_kind: "article",
      segments: [{ id: "s-1", order: 0, text: "Learn language safely." }],
    })).toMatchObject({ status: "exact", strategy: "identity-anchor" });
  });

  it("resolves TXT ranges through page/segment order", () => {
    const result = resolveAnnotation({
      reader_kind: "txt",
      locator: { reader_kind: "txt", kind: "text_range", segment_order: 1, page: 2, start_offset: 0, end_offset: 6, quote: { exact: "second" } },
    }, {
      reader_kind: "txt",
      segments: [{ order: 0, text: "first" }, { order: 1, text: "second page" }],
    });

    expect(result).toMatchObject({ status: "exact", locator: { segment_order: 1 } });
  });

  it("resolves a PDF range on its page", () => {
    const result = resolveAnnotation({
      reader_kind: "pdf",
      locator: { reader_kind: "pdf", kind: "text_range", page: 4, start_offset: 0, end_offset: 5, quote: { exact: "Paper" } },
    }, {
      reader_kind: "pdf",
      pages: [{ page: 4, text: "Paper methods" }],
    });

    expect(result).toMatchObject({ status: "exact", locator: { page: 4 } });
  });

  it("resolves an EPUB CFI with its quote contract", () => {
    const result = resolveAnnotation({
      reader_kind: "epub",
      locator: { reader_kind: "epub", kind: "epub_cfi", cfi: "epubcfi(/6/4!/4/1:0)", quote: { exact: "chapter" } },
    }, { reader_kind: "epub", epub_cfi_available: true });

    expect(result).toMatchObject({ status: "exact", strategy: "identity-anchor", locator: { kind: "epub_cfi" } });
  });

  it("resolves media ranges and keeps subtitle identity", () => {
    const result = resolveAnnotation({
      reader_kind: "media",
      locator: { reader_kind: "media", kind: "time_range", segment_id: "sub-2", current_time: 12, end_time: 15, quote: { exact: "hello" } },
    }, {
      reader_kind: "media",
      segments: [{ id: "sub-2", text: "hello", start_time: 12, end_time: 15 }],
      duration: 60,
    });

    expect(result).toMatchObject({ status: "exact", locator: { current_time: 12, segment_id: "sub-2" } });
  });

  it("degrades to quote when the source text moves or its revision changes", () => {
    const result = resolveAnnotation({
      material_revision: "old",
      reader_kind: "article",
      locator: {
        reader_kind: "article",
        kind: "text_range",
        segment_id: "s-1",
        start_offset: 0,
        end_offset: 5,
        material_revision: "old",
        quote: { exact: "target" },
      },
    }, {
      reader_kind: "article",
      material_revision: "new",
      segments: [{ id: "s-1", order: 0, text: "prefix target suffix" }],
    });

    expect(result).toMatchObject({ status: "degraded", strategy: "quote", locator: { start_offset: 7, end_offset: 13 } });
  });

  it("creates a durable draft with source text, quote, revision, and hash", () => {
    const draft = createAnnotationDraft({
      materialId: "material-1",
      readerKind: "article",
      sourceText: "Keep the original source sentence.",
      selectedText: "original source",
      segmentId: "segment-1",
      segmentOrder: 3,
      materialRevision: "rev-3",
      contentSha256: "A".repeat(64),
    });

    expect(draft).toMatchObject({
      material_id: "material-1",
      source_text: "Keep the original source sentence.",
      quote: { exact: "original source" },
      material_revision: "rev-3",
      content_sha256: "a".repeat(64),
      locator: {
        reader_kind: "article",
        segment_id: "segment-1",
        segment_order: 3,
        start_offset: 9,
        end_offset: 24,
      },
    });
    expect(toSourceLocator(draft.locator)).toMatchObject({
      version: 1,
      kind: "text_range",
      start_offset: 9,
      end_offset: 24,
      quote: { exact: "original source" },
    });
  });
});
