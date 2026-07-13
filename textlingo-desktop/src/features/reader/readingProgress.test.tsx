import { renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  READING_PROGRESS_COMPLETION_THRESHOLD,
  createEpubCfiLocator,
  createMediaTimeLocator,
  createPageLocator,
  createReadingProgressUpdate,
  createSegmentLocator,
  getEpubCfiFromLocator,
  getMediaTimeFromLocator,
  getPageNumberFromLocator,
  getPageNumberFromProgress,
  getSegmentPositionFromLocator,
  useReadingProgressReporter,
} from "./readingProgress";

describe("readingProgress", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("normalizes progress and marks completion at the configured threshold", () => {
    expect(createReadingProgressUpdate("article", createSegmentLocator(1, 10), -1)).toMatchObject({
      progress_ratio: 0,
      status: "reading",
    });
    expect(createReadingProgressUpdate("article", createSegmentLocator(10, 10), READING_PROGRESS_COMPLETION_THRESHOLD)).toMatchObject({
      progress_ratio: READING_PROGRESS_COMPLETION_THRESHOLD,
      status: "completed",
    });
    expect(createReadingProgressUpdate("article", createSegmentLocator(10, 10), 2).progress_ratio).toBe(1);
  });

  it("round-trips reader-specific locators and derives a safe page fallback", () => {
    expect(getSegmentPositionFromLocator(createSegmentLocator(0, 12))).toBe(0);
    expect(getPageNumberFromLocator(createPageLocator(4, 12))).toBe(4);
    expect(getPageNumberFromProgress(0.5, 11)).toBe(6);
    expect(getEpubCfiFromLocator(createEpubCfiLocator("epubcfi(/6/2[chapter]!/4/1:0)"))).toBe("epubcfi(/6/2[chapter]!/4/1:0)");
    expect(getMediaTimeFromLocator(createMediaTimeLocator(12.5, 100))).toBe(12.5);
  });

  it("throttles updates, flushes terminal updates, and contains callback errors", () => {
    vi.useFakeTimers();
    const onProgressChange = vi.fn();
    const { result, unmount } = renderHook(() => useReadingProgressReporter(onProgressChange, 500));

    result.current.reportProgress(createReadingProgressUpdate("txt", createPageLocator(2, 10), 0.2));
    result.current.reportProgress(createReadingProgressUpdate("txt", createPageLocator(3, 10), 0.3));
    expect(onProgressChange).not.toHaveBeenCalled();

    vi.advanceTimersByTime(500);
    expect(onProgressChange).toHaveBeenCalledTimes(1);
    expect(onProgressChange).toHaveBeenLastCalledWith(expect.objectContaining({ locator: { version: 1, kind: "page", page: 3, total_pages: 10 } }));

    result.current.reportProgress(createReadingProgressUpdate("txt", createPageLocator(10, 10), 1), true);
    expect(onProgressChange).toHaveBeenCalledTimes(2);

    const throwingCallback = vi.fn(() => {
      throw new Error("backend unavailable");
    });
    const throwingHook = renderHook(() => useReadingProgressReporter(throwingCallback, 500));
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => undefined);
    expect(() => throwingHook.result.current.reportProgress(createReadingProgressUpdate("txt", createPageLocator(1, 1), 1), true)).not.toThrow();
    expect(warnSpy).toHaveBeenCalled();
    warnSpy.mockRestore();
    throwingHook.unmount();

    result.current.reportProgress(createReadingProgressUpdate("txt", createPageLocator(4, 10), 0.4));
    unmount();
    expect(onProgressChange).toHaveBeenLastCalledWith(expect.objectContaining({ locator: { version: 1, kind: "page", page: 4, total_pages: 10 } }));
  });

  it("flushes a pending update through the callback that reported it", () => {
    vi.useFakeTimers();
    const onMaterialAProgress = vi.fn();
    const onMaterialBProgress = vi.fn();
    const { result, rerender } = renderHook(
      ({ onProgressChange }) => useReadingProgressReporter(onProgressChange, 500),
      { initialProps: { onProgressChange: onMaterialAProgress } },
    );

    result.current.reportProgress(createReadingProgressUpdate("pdf", createPageLocator(2, 10), 0.2));
    rerender({ onProgressChange: onMaterialBProgress });

    expect(onMaterialAProgress).toHaveBeenCalledWith(expect.objectContaining({
      locator: { version: 1, kind: "page", page: 2, total_pages: 10 },
    }));
    expect(onMaterialBProgress).not.toHaveBeenCalled();

    result.current.reportProgress(createReadingProgressUpdate("pdf", createPageLocator(3, 10), 0.3));
    vi.advanceTimersByTime(500);
    expect(onMaterialBProgress).toHaveBeenCalledWith(expect.objectContaining({
      locator: { version: 1, kind: "page", page: 3, total_pages: 10 },
    }));
  });
});
