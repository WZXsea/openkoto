import type { Annotation } from "../../types";

export type AnnotationLocatorStatus = "precise" | "fallback" | "invalid";

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

export function getAnnotationLocatorStatus(annotation: Pick<Annotation, "locator">): AnnotationLocatorStatus {
  const locator = asRecord(annotation.locator);
  if (!locator || locator.version !== undefined && locator.version !== 1) return "invalid";

  switch (locator.kind) {
    case "text_range": {
      const quote = asRecord(locator.quote);
      return typeof locator.start_offset === "number"
        && typeof locator.end_offset === "number"
        && locator.start_offset < locator.end_offset
        && typeof quote?.exact === "string"
        && quote.exact.trim().length > 0
        ? "precise"
        : "invalid";
    }
    case "epub_cfi":
    case "cfi":
      return typeof locator.cfi === "string" && locator.cfi.trim().length > 0 ? "precise" : "invalid";
    case "segment":
      return typeof locator.segment_order === "number" && typeof locator.total_segments === "number"
        ? "fallback"
        : "invalid";
    case "page":
      return typeof locator.page === "number" && locator.page >= 1 ? "fallback" : "invalid";
    case "time":
    case "time_range":
      return typeof locator.current_time === "number" && locator.current_time >= 0 ? "fallback" : "invalid";
    default:
      return "invalid";
  }
}

export function formatAnnotationLocator(annotation: Pick<Annotation, "locator">): string {
  const locator = asRecord(annotation.locator);
  if (!locator) return "定位数据不可用";
  switch (locator.kind) {
    case "text_range":
      return `文本 ${locator.start_offset}-${locator.end_offset}`;
    case "segment":
      return `第 ${Number(locator.segment_order) + 1}/${locator.total_segments} 段`;
    case "page":
      return `第 ${locator.page} 页`;
    case "epub_cfi":
    case "cfi":
      return "EPUB CFI";
    case "time":
    case "time_range":
      return `时间 ${formatSeconds(locator.current_time)}${typeof locator.end_time === "number" ? `-${formatSeconds(locator.end_time)}` : ""}`;
    default:
      return "定位数据不可用";
  }
}

function formatSeconds(value: unknown): string {
  if (typeof value !== "number" || !Number.isFinite(value)) return "?";
  const seconds = Math.max(0, Math.floor(value));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}
