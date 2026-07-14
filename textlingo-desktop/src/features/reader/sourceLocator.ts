export const SOURCE_LOCATOR_VERSION = 1 as const;

export interface TextQuoteSelector {
  exact: string;
  prefix?: string;
  suffix?: string;
}

interface SourceLocatorMetadata {
  version?: typeof SOURCE_LOCATOR_VERSION;
  reader_kind?: "article" | "txt" | "pdf" | "epub" | "media";
  material_revision?: string;
  content_sha256?: string;
  quote?: TextQuoteSelector;
}

export type SourceLocatorV1 = SourceLocatorMetadata & (
  | {
    kind: "segment";
    segment_order: number;
    total_segments: number;
    segment_id?: string;
  }
  | {
    kind: "text_range";
    segment_id?: string;
    segment_order?: number;
    page?: number;
    total_pages?: number;
    start_offset: number;
    end_offset: number;
    quote: TextQuoteSelector;
  }
  | {
    kind: "page";
    page: number;
    total_pages?: number;
  }
  | {
    kind: "epub_cfi" | "cfi";
    cfi: string;
  }
  | {
    kind: "time" | "time_range";
    current_time: number;
    end_time?: number;
    duration?: number;
    segment_id?: string;
  }
);

export function withLocatorVersion<T extends SourceLocatorV1>(locator: T): T & { version: 1 } {
  return { ...locator, version: SOURCE_LOCATOR_VERSION };
}

export function parseSourceLocator(value: unknown): SourceLocatorV1 | undefined {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return undefined;
  const record = value as Record<string, unknown>;
  const kind = record.kind;
  if (typeof kind !== "string") return undefined;
  switch (kind) {
    case "segment":
      if (typeof record.segment_order !== "number" || typeof record.total_segments !== "number") return undefined;
      break;
    case "text_range":
      if (typeof record.start_offset !== "number" || typeof record.end_offset !== "number") return undefined;
      if (typeof record.quote !== "object" || record.quote === null || typeof (record.quote as Record<string, unknown>).exact !== "string") return undefined;
      break;
    case "page":
      if (typeof record.page !== "number") return undefined;
      break;
    case "epub_cfi":
    case "cfi":
      if (typeof record.cfi !== "string") return undefined;
      break;
    case "time":
    case "time_range":
      if (typeof record.current_time !== "number") return undefined;
      break;
    default:
      return undefined;
  }
  const locator = value as SourceLocatorV1;
  try {
    assertValidSourceLocator(locator);
    return locator;
  } catch {
    return undefined;
  }
}

export function createTextRangeLocator(params: {
  segmentId?: string;
  segmentOrder?: number;
  startOffset: number;
  endOffset: number;
  exact: string;
  prefix?: string;
  suffix?: string;
  materialRevision?: string;
  contentSha256?: string;
}): SourceLocatorV1 {
  const locator: SourceLocatorV1 = {
    version: SOURCE_LOCATOR_VERSION,
    kind: "text_range",
    ...(params.segmentId ? { segment_id: params.segmentId } : {}),
    ...(params.segmentOrder !== undefined ? { segment_order: params.segmentOrder } : {}),
    start_offset: Math.max(0, Math.trunc(params.startOffset)),
    end_offset: Math.max(0, Math.trunc(params.endOffset)),
    quote: {
      exact: params.exact,
      ...(params.prefix ? { prefix: params.prefix } : {}),
      ...(params.suffix ? { suffix: params.suffix } : {}),
    },
    ...(params.materialRevision ? { material_revision: params.materialRevision } : {}),
    ...(params.contentSha256 ? { content_sha256: params.contentSha256.toLowerCase() } : {}),
  };
  assertValidSourceLocator(locator);
  return locator;
}

export function assertValidSourceLocator(locator: SourceLocatorV1): void {
  if (locator.version !== undefined && locator.version !== SOURCE_LOCATOR_VERSION) {
    throw new Error(`Unsupported source locator version: ${locator.version}`);
  }
  if (locator.content_sha256 && !/^[0-9a-f]{64}$/i.test(locator.content_sha256)) {
    throw new Error("content_sha256 must be a 64-character hexadecimal digest");
  }
  if (locator.quote && locator.quote.exact.trim().length === 0) {
    throw new Error("quote.exact must not be empty");
  }
  if (locator.kind === "segment" && (locator.segment_order < 0 || locator.total_segments < 1)) {
    throw new Error("Invalid segment locator");
  }
  if (locator.kind === "text_range" && (locator.start_offset < 0 || locator.start_offset >= locator.end_offset)) {
    throw new Error("Invalid text range locator");
  }
  if (locator.kind === "text_range" && locator.page !== undefined
    && (locator.page < 1 || (locator.total_pages !== undefined && locator.total_pages < locator.page))) {
    throw new Error("Invalid text range page");
  }
  if (locator.kind === "page" && (locator.page < 1 || (locator.total_pages !== undefined && locator.total_pages < locator.page))) {
    throw new Error("Invalid page locator");
  }
  if ((locator.kind === "epub_cfi" || locator.kind === "cfi") && locator.cfi.trim().length === 0) {
    throw new Error("EPUB CFI must not be empty");
  }
  if (locator.kind === "time" || locator.kind === "time_range") {
    if (!Number.isFinite(locator.current_time) || locator.current_time < 0) throw new Error("Invalid time locator start");
    if (locator.end_time !== undefined && (!Number.isFinite(locator.end_time) || locator.end_time < locator.current_time)) {
      throw new Error("Invalid time locator end");
    }
    if (locator.duration !== undefined && (!Number.isFinite(locator.duration) || locator.duration < 0)) {
      throw new Error("Invalid time locator duration");
    }
  }
}
