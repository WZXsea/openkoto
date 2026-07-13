import { useCallback, useEffect, useRef } from "react";

import { SOURCE_LOCATOR_VERSION, type SourceLocatorV1 } from "./sourceLocator";

export const READING_PROGRESS_COMPLETION_THRESHOLD = 0.98;

export type ReaderKind = "article" | "pdf" | "epub" | "txt" | "media";
export type ReadingProgressStatus = "reading" | "completed";

export type ReadingProgressLocator = SourceLocatorV1;

export interface ReadingProgressUpdate {
  reader_kind: ReaderKind;
  locator: ReadingProgressLocator;
  progress_ratio: number;
  status: ReadingProgressStatus;
}

export type ReadingProgressChangeHandler = (update: ReadingProgressUpdate) => void;

interface PendingReadingProgress {
  update: ReadingProgressUpdate;
  callback: ReadingProgressChangeHandler;
}

export function clampProgressRatio(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

export function getReadingProgressStatus(progressRatio: number): ReadingProgressStatus {
  return clampProgressRatio(progressRatio) >= READING_PROGRESS_COMPLETION_THRESHOLD
    ? "completed"
    : "reading";
}

export function createReadingProgressUpdate(
  readerKind: ReaderKind,
  locator: ReadingProgressLocator,
  progressRatio: number,
): ReadingProgressUpdate {
  const normalizedRatio = clampProgressRatio(progressRatio);
  return {
    reader_kind: readerKind,
    locator,
    progress_ratio: normalizedRatio,
    status: getReadingProgressStatus(normalizedRatio),
  };
}

export function getInitialProgressForReader(
  initialProgress: ReadingProgressUpdate | undefined,
  readerKind: ReaderKind,
): ReadingProgressUpdate | undefined {
  return initialProgress?.reader_kind === readerKind ? initialProgress : undefined;
}

export function createSegmentLocator(position: number, total: number, segmentId?: string): ReadingProgressLocator {
  return {
    version: SOURCE_LOCATOR_VERSION,
    kind: "segment",
    segment_order: Math.max(0, Math.trunc(position)),
    total_segments: Math.max(1, Math.trunc(total)),
    ...(segmentId ? { segment_id: segmentId } : {}),
  };
}

export function getSegmentPositionFromLocator(locator: ReadingProgressLocator | undefined): number | undefined {
  return locator?.kind === "segment" ? locator.segment_order : undefined;
}

export function createPageLocator(pageNumber: number, totalPages: number): ReadingProgressLocator {
  return {
    version: SOURCE_LOCATOR_VERSION,
    kind: "page",
    page: Math.max(1, Math.trunc(pageNumber)),
    total_pages: Math.max(1, Math.trunc(totalPages)),
  };
}

export function getPageNumberFromLocator(locator: ReadingProgressLocator | undefined): number | undefined {
  return locator?.kind === "page" ? locator.page : undefined;
}

export function getPageNumberFromProgress(progressRatio: number | undefined, totalPages: number): number | undefined {
  if (progressRatio === undefined || !Number.isFinite(progressRatio) || totalPages < 1) return undefined;
  return Math.min(totalPages, Math.max(1, Math.round(clampProgressRatio(progressRatio) * Math.max(1, totalPages - 1)) + 1));
}

export function createEpubCfiLocator(cfi: string): ReadingProgressLocator {
  return { version: SOURCE_LOCATOR_VERSION, kind: "epub_cfi", cfi };
}

export function getEpubCfiFromLocator(locator: ReadingProgressLocator | undefined): string | undefined {
  return locator?.kind === "epub_cfi" || locator?.kind === "cfi" ? locator.cfi : undefined;
}

export function createMediaTimeLocator(currentTime: number, duration: number): ReadingProgressLocator {
  return {
    version: SOURCE_LOCATOR_VERSION,
    kind: "time",
    current_time: Math.max(0, currentTime),
    duration: Math.max(0, duration),
  };
}

export function getMediaTimeFromLocator(locator: ReadingProgressLocator | undefined): number | undefined {
  return locator?.kind === "time" || locator?.kind === "time_range" ? locator.current_time : undefined;
}

export function useReadingProgressReporter(
  onProgressChange: ReadingProgressChangeHandler | undefined,
  delayMs = 750,
) {
  const callbackRef = useRef(onProgressChange);
  const pendingUpdateRef = useRef<PendingReadingProgress | undefined>(undefined);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const flushProgress = useCallback(() => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = undefined;
    }

    const pendingUpdate = pendingUpdateRef.current;
    pendingUpdateRef.current = undefined;
    if (!pendingUpdate) return;

    try {
      pendingUpdate.callback(pendingUpdate.update);
    } catch (error) {
      console.warn("[ReadingProgress] Progress callback failed:", error);
    }
  }, []);

  useEffect(() => {
    // Flush before switching callbacks so a pending update remains bound to its material.
    if (callbackRef.current !== onProgressChange) {
      flushProgress();
    }
    callbackRef.current = onProgressChange;
  }, [flushProgress, onProgressChange]);

  const reportProgress = useCallback((update: ReadingProgressUpdate, flushImmediately = false) => {
    const callback = callbackRef.current;
    if (!callback) return;

    pendingUpdateRef.current = { update, callback };
    if (flushImmediately) {
      flushProgress();
      return;
    }

    if (!timeoutRef.current) {
      timeoutRef.current = setTimeout(flushProgress, delayMs);
    }
  }, [delayMs, flushProgress]);

  useEffect(() => flushProgress, [flushProgress]);

  return { reportProgress, flushProgress };
}
