import { useMemo } from "react";
import type { SourceLocatorV1 } from "./sourceLocator";

export type AnnotationReaderKind = "article" | "txt" | "pdf" | "epub" | "media";

export interface AnnotationQuote {
  exact: string;
  prefix?: string;
  suffix?: string;
}

interface LocatorIdentity {
  material_revision?: string | null;
  content_sha256?: string | null;
  quote?: AnnotationQuote;
}

export type ReaderAnnotationLocator = LocatorIdentity & (
  | {
      reader_kind: "article" | "txt";
      kind: "text_range";
      segment_id?: string;
      segment_order?: number;
      page?: number;
      total_pages?: number;
      start_offset: number;
      end_offset: number;
      quote: AnnotationQuote;
    }
  | {
      reader_kind: "article" | "txt";
      kind: "segment";
      segment_order: number;
      total_segments?: number;
      segment_id?: string;
    }
  | {
      reader_kind: "pdf";
      kind: "text_range";
      page: number;
      total_pages?: number;
      start_offset: number;
      end_offset: number;
      quote: AnnotationQuote;
    }
  | {
      reader_kind: "pdf";
      kind: "page";
      page: number;
      total_pages?: number;
    }
  | {
      reader_kind: "epub";
      kind: "epub_cfi";
      cfi: string;
      quote?: AnnotationQuote;
    }
  | {
      reader_kind: "media";
      kind: "time_range";
      current_time: number;
      end_time?: number;
      duration?: number;
      segment_id?: string;
      quote?: AnnotationQuote;
    }
);

export interface ReaderAnnotationReference {
  id?: string;
  material_id?: string;
  reader_kind?: AnnotationReaderKind;
  source_text?: string;
  quote?: AnnotationQuote;
  material_revision?: string | null;
  content_sha256?: string | null;
  locator?: unknown;
  source_locator?: unknown;
}

export interface ReaderTextSegment {
  id?: string;
  order?: number;
  text: string;
  page?: number;
  start_time?: number;
  end_time?: number;
}

export interface ReaderAnnotationContext {
  reader_kind: AnnotationReaderKind;
  material_revision?: string;
  content_sha256?: string;
  segments?: ReaderTextSegment[];
  pages?: Array<{ page: number; text?: string }>;
  epub_cfi_available?: boolean;
  find_epub_quote?: (quote: AnnotationQuote) => string | undefined;
  duration?: number;
}

export type AnnotationResolutionStatus = "exact" | "degraded" | "unresolved";
export type AnnotationResolutionStrategy = "identity-anchor" | "quote" | "neighbor" | "coarse" | "none";

export interface AnnotationResolution {
  status: AnnotationResolutionStatus;
  strategy: AnnotationResolutionStrategy;
  locator?: ReaderAnnotationLocator;
  source_text?: string;
  message: string;
}

export interface CreateAnnotationDraftInput {
  materialId: string;
  readerKind: AnnotationReaderKind;
  sourceText: string;
  selectedText?: string;
  segmentId?: string;
  segmentOrder?: number;
  page?: number;
  totalPages?: number;
  startOffset?: number;
  endOffset?: number;
  cfi?: string;
  currentTime?: number;
  endTime?: number;
  duration?: number;
  materialRevision?: string;
  contentSha256?: string;
  prefix?: string;
  suffix?: string;
}

export interface ReaderAnnotationDraft {
  material_id: string;
  segment_id?: string;
  reader_kind: AnnotationReaderKind;
  source_text: string;
  quote: AnnotationQuote;
  material_revision?: string;
  content_sha256?: string;
  locator: ReaderAnnotationLocator;
}

function asRecord(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function asQuote(value: unknown): AnnotationQuote | undefined {
  const record = asRecord(value);
  return typeof record?.exact === "string" && record.exact.trim()
    ? { exact: record.exact, ...(typeof record.prefix === "string" ? { prefix: record.prefix } : {}), ...(typeof record.suffix === "string" ? { suffix: record.suffix } : {}) }
    : undefined;
}

function numberOrUndefined(value: unknown): number | undefined {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

export function parseReaderAnnotationLocator(value: unknown, readerKind?: AnnotationReaderKind): ReaderAnnotationLocator | undefined {
  const record = asRecord(value);
  if (!record) return undefined;
  const kind = record.kind;
  const resolvedReaderKind = (record.reader_kind ?? readerKind) as AnnotationReaderKind | undefined;
  if (!resolvedReaderKind || !["article", "txt", "pdf", "epub", "media"].includes(resolvedReaderKind)) return undefined;
  const identity = {
    ...(typeof record.material_revision === "string" ? { material_revision: record.material_revision } : {}),
    ...(typeof record.content_sha256 === "string" ? { content_sha256: record.content_sha256 } : {}),
    ...(asQuote(record.quote) ? { quote: asQuote(record.quote) } : {}),
  };

  if ((resolvedReaderKind === "article" || resolvedReaderKind === "txt" || resolvedReaderKind === "pdf") && kind === "text_range") {
    const start = numberOrUndefined(record.start_offset);
    const end = numberOrUndefined(record.end_offset);
    const quote = asQuote(record.quote);
    const page = numberOrUndefined(record.page);
    if (start === undefined || end === undefined || end <= start || !quote) return undefined;
    if (resolvedReaderKind === "pdf" && page === undefined) return undefined;
    return {
      ...identity,
      reader_kind: resolvedReaderKind,
      kind: "text_range",
      ...(typeof record.segment_id === "string" ? { segment_id: record.segment_id } : {}),
      ...(numberOrUndefined(record.segment_order) !== undefined ? { segment_order: numberOrUndefined(record.segment_order) } : {}),
      ...(page !== undefined ? { page } : {}),
      ...(numberOrUndefined(record.total_pages) !== undefined ? { total_pages: numberOrUndefined(record.total_pages) } : {}),
      start_offset: Math.trunc(start),
      end_offset: Math.trunc(end),
      quote,
    } as ReaderAnnotationLocator;
  }

  if ((resolvedReaderKind === "article" || resolvedReaderKind === "txt") && kind === "segment" && numberOrUndefined(record.segment_order) !== undefined) {
    return {
      ...identity,
      reader_kind: resolvedReaderKind,
      kind: "segment",
      segment_order: Math.max(0, Math.trunc(numberOrUndefined(record.segment_order)!)),
      ...(numberOrUndefined(record.total_segments) !== undefined ? { total_segments: numberOrUndefined(record.total_segments) } : {}),
      ...(typeof record.segment_id === "string" ? { segment_id: record.segment_id } : {}),
    };
  }

  if (resolvedReaderKind === "pdf" && kind === "page" && numberOrUndefined(record.page) !== undefined) {
    return { ...identity, reader_kind: "pdf", kind: "page", page: Math.max(1, Math.trunc(numberOrUndefined(record.page)!)), ...(numberOrUndefined(record.total_pages) !== undefined ? { total_pages: numberOrUndefined(record.total_pages) } : {}) };
  }

  if (resolvedReaderKind === "epub" && (kind === "epub_cfi" || kind === "cfi") && typeof record.cfi === "string") {
    return { ...identity, reader_kind: "epub", kind: "epub_cfi", cfi: record.cfi, ...(asQuote(record.quote) ? { quote: asQuote(record.quote) } : {}) };
  }

  if (resolvedReaderKind === "media" && (kind === "time_range" || kind === "time") && numberOrUndefined(record.current_time) !== undefined) {
    return {
      ...identity,
      reader_kind: "media",
      kind: "time_range",
      current_time: Math.max(0, numberOrUndefined(record.current_time)! ),
      ...(numberOrUndefined(record.end_time) !== undefined ? { end_time: numberOrUndefined(record.end_time) } : {}),
      ...(numberOrUndefined(record.duration) !== undefined ? { duration: numberOrUndefined(record.duration) } : {}),
      ...(typeof record.segment_id === "string" ? { segment_id: record.segment_id } : {}),
      ...(asQuote(record.quote) ? { quote: asQuote(record.quote) } : {}),
    };
  }
  return undefined;
}

export function getAnnotationLocator(annotation: ReaderAnnotationReference): ReaderAnnotationLocator | undefined {
  const parsed = parseReaderAnnotationLocator(annotation.locator ?? annotation.source_locator, annotation.reader_kind);
  if (!parsed) return undefined;
  return {
    ...parsed,
    ...(parsed.material_revision ?? annotation.material_revision ? { material_revision: parsed.material_revision ?? annotation.material_revision ?? undefined } : {}),
    ...(parsed.content_sha256 ?? annotation.content_sha256 ? { content_sha256: parsed.content_sha256 ?? annotation.content_sha256 ?? undefined } : {}),
    ...(parsed.quote ?? annotation.quote ? { quote: parsed.quote ?? annotation.quote } : {}),
  } as ReaderAnnotationLocator;
}

function sourceIdentityState(locator: ReaderAnnotationLocator, context: ReaderAnnotationContext): "match" | "mismatch" | "unknown" {
  const hasLocatorIdentity = Boolean(locator.material_revision || locator.content_sha256);
  if (!hasLocatorIdentity) return "match";
  if ((locator.material_revision && context.material_revision && locator.material_revision !== context.material_revision)
    || (locator.content_sha256 && context.content_sha256 && locator.content_sha256.toLowerCase() !== context.content_sha256.toLowerCase())) {
    return "mismatch";
  }
  return locator.material_revision === context.material_revision || locator.content_sha256?.toLowerCase() === context.content_sha256?.toLowerCase()
    ? "match"
    : "unknown";
}

function quoteMatch(text: string, quote: AnnotationQuote): { start: number; end: number } | undefined {
  const direct = text.indexOf(quote.exact);
  if (direct >= 0) return { start: direct, end: direct + quote.exact.length };
  const insensitive = text.toLocaleLowerCase().indexOf(quote.exact.toLocaleLowerCase());
  if (insensitive >= 0) return { start: insensitive, end: insensitive + quote.exact.length };
  const pattern = quote.exact.trim().split(/\s+/).map((part) => part.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")).join("\\s+");
  const match = pattern ? new RegExp(pattern, "i").exec(text) : undefined;
  return match && match.index !== undefined ? { start: match.index, end: match.index + match[0].length } : undefined;
}

function withTextRange(locator: ReaderAnnotationLocator, start: number, end: number, extra: Partial<ReaderAnnotationLocator> = {}): ReaderAnnotationLocator {
  return { ...locator, ...extra, start_offset: Math.max(0, Math.trunc(start)), end_offset: Math.max(Math.trunc(start) + 1, Math.trunc(end)) } as ReaderAnnotationLocator;
}

function resolveTextRange(annotation: ReaderAnnotationReference, locator: Extract<ReaderAnnotationLocator, { kind: "text_range" }>, context: ReaderAnnotationContext): AnnotationResolution {
  const identity = sourceIdentityState(locator, context);
  const quote = locator.quote ?? annotation.quote;
  const candidates: Array<{ id?: string; order?: number; page?: number; text: string }> = context.reader_kind === "pdf"
    ? (context.pages ?? []).filter((page): page is { page: number; text: string } => typeof page.text === "string" && page.text.length > 0).map((page) => ({ ...page }))
    : (context.segments ?? []);
  const preferred = candidates.filter((candidate) => {
    if (context.reader_kind === "pdf") return candidate.page === locator.page;
    return ("segment_id" in locator && locator.segment_id && candidate.id === locator.segment_id)
      || ("segment_order" in locator && locator.segment_order !== undefined && candidate.order === locator.segment_order)
      || (context.reader_kind === "txt" && "page" in locator && locator.page !== undefined && candidate.page === locator.page);
  });
  const ordered = [...preferred, ...candidates.filter((candidate) => !preferred.includes(candidate))];
  const exactCandidate = preferred.find((candidate) => {
    const start = locator.start_offset;
    const end = locator.end_offset;
    return identity === "match" && start >= 0 && end <= candidate.text.length && candidate.text.slice(start, end) === quote.exact;
  });
  if (exactCandidate) {
    const resolved = context.reader_kind === "pdf"
      ? { ...locator, page: exactCandidate.page! }
      : { ...locator, ...(exactCandidate.id ? { segment_id: exactCandidate.id } : {}), ...(exactCandidate.order !== undefined ? { segment_order: exactCandidate.order } : {}) };
    return { status: "exact", strategy: "identity-anchor", locator: resolved, source_text: exactCandidate.text, message: "已按版本和原文范围精确定位" };
  }

  if (quote) {
    const quoteCandidate = ordered.find((candidate) => quoteMatch(candidate.text, quote));
    if (quoteCandidate) {
      const match = quoteMatch(quoteCandidate.text, quote)!;
      const resolved = withTextRange(locator, match.start, match.end, context.reader_kind === "pdf"
      ? { page: quoteCandidate.page! }
        : { ...(quoteCandidate.id ? { segment_id: quoteCandidate.id } : {}), ...(quoteCandidate.order !== undefined ? { segment_order: quoteCandidate.order } : {}) });
      return { status: "degraded", strategy: "quote", locator: resolved, source_text: quoteCandidate.text, message: "原文版本或范围已变化，已按摘录文本定位" };
    }
  }

  const neighbor = preferred[0] ?? ordered[0];
  if (neighbor) {
    const resolved = context.reader_kind === "pdf"
      ? withTextRange(locator, 0, Math.max(1, Math.min(neighbor.text.length, locator.end_offset - locator.start_offset)), { page: neighbor.page! })
      : withTextRange(locator, 0, Math.max(1, Math.min(neighbor.text.length, locator.end_offset - locator.start_offset)), { ...(neighbor.id ? { segment_id: neighbor.id } : {}), ...(neighbor.order !== undefined ? { segment_order: neighbor.order } : {}) });
    return { status: "degraded", strategy: "neighbor", locator: resolved, source_text: neighbor.text, message: "无法匹配原文摘录，已降级到邻近段落或页面" };
  }
  if (context.reader_kind === "pdf") {
    return { status: "degraded", strategy: "coarse", locator, message: "无法读取 PDF 原文层，已降级到页面位置" };
  }
  return { status: "unresolved", strategy: "none", message: "当前材料中没有可用的原文定位" };
}

function resolveEpub(annotation: ReaderAnnotationReference, locator: Extract<ReaderAnnotationLocator, { reader_kind: "epub" }>, context: ReaderAnnotationContext): AnnotationResolution {
  const identity = sourceIdentityState(locator, context);
  if (context.epub_cfi_available !== false && identity === "match") {
    return { status: "exact", strategy: "identity-anchor", locator, message: "已按 EPUB CFI 精确定位" };
  }
  const quote = locator.quote ?? annotation.quote;
  const fallbackCfi = quote && context.find_epub_quote?.(quote);
  if (fallbackCfi) {
    return { status: "degraded", strategy: "quote", locator: { ...locator, cfi: fallbackCfi }, message: "EPUB 版本已变化，已按摘录文本定位" };
  }
  if (context.epub_cfi_available !== false) {
    return { status: "degraded", strategy: "coarse", locator, message: "无法验证 EPUB 摘录，已降级到 CFI 位置" };
  }
  return { status: "unresolved", strategy: "none", message: "当前 EPUB 无法恢复该 CFI 或摘录" };
}

function resolveMedia(annotation: ReaderAnnotationReference, locator: Extract<ReaderAnnotationLocator, { reader_kind: "media" }>, context: ReaderAnnotationContext): AnnotationResolution {
  const identity = sourceIdentityState(locator, context);
  const duration = context.duration ?? locator.duration;
  const inBounds = duration === undefined || locator.current_time <= duration;
  if (identity === "match" && inBounds && locator.current_time >= 0) {
    return { status: "exact", strategy: "identity-anchor", locator, message: "已按媒体时间范围精确定位" };
  }
  const quote = locator.quote ?? annotation.quote;
  const quoteSegment = quote && context.segments?.find((segment) => quoteMatch(segment.text, quote));
  if (quoteSegment) {
    return { status: "degraded", strategy: "quote", locator: { ...locator, current_time: Math.max(0, quoteSegment.start_time ?? 0), ...(quoteSegment.end_time !== undefined ? { end_time: quoteSegment.end_time } : {}), ...(quoteSegment.id ? { segment_id: quoteSegment.id } : {}) }, source_text: quoteSegment.text, message: "媒体版本或时间范围已变化，已按字幕摘录定位" };
  }
  const neighbor = locator.segment_id && context.segments?.find((segment) => segment.id === locator.segment_id);
  if (neighbor) {
    return { status: "degraded", strategy: "neighbor", locator: { ...locator, current_time: Math.max(0, neighbor.start_time ?? 0), ...(neighbor.end_time !== undefined ? { end_time: neighbor.end_time } : {}) }, source_text: neighbor.text, message: "已降级到相邻字幕时间范围" };
  }
  if (duration !== undefined && duration > 0) {
    return { status: "degraded", strategy: "coarse", locator: { ...locator, current_time: Math.min(Math.max(0, locator.current_time), duration) }, message: "已降级到媒体粗粒度时间位置" };
  }
  return { status: "unresolved", strategy: "none", message: "当前媒体中没有可用的时间定位" };
}

export function resolveAnnotation(annotation: ReaderAnnotationReference, context: ReaderAnnotationContext): AnnotationResolution {
  const locator = getAnnotationLocator({ ...annotation, reader_kind: annotation.reader_kind ?? context.reader_kind });
  if (!locator || locator.reader_kind !== context.reader_kind) {
    return { status: "unresolved", strategy: "none", message: "标注与当前阅读器类型不匹配" };
  }
  if (locator.kind === "text_range") return resolveTextRange(annotation, locator, context);
  if (locator.kind === "segment") {
    const segment = context.segments?.find((candidate) => candidate.id === locator.segment_id || candidate.order === locator.segment_order);
    return segment
      ? { status: "degraded", strategy: "neighbor", locator, source_text: segment.text, message: "已按段落位置恢复，原文范围不可验证" }
      : { status: "unresolved", strategy: "none", message: "当前材料中没有对应段落" };
  }
  if (locator.kind === "page") {
    return { status: "degraded", strategy: "coarse", locator, message: "已按页面位置恢复，原文范围不可验证" };
  }
  if (locator.reader_kind === "epub") return resolveEpub(annotation, locator, context);
  return resolveMedia(annotation, locator, context);
}

function quoteForDraft(sourceText: string, selectedText: string, start: number, prefix?: string, suffix?: string): AnnotationQuote {
  return {
    exact: selectedText,
    prefix: prefix ?? sourceText.slice(Math.max(0, start - 80), start),
    suffix: suffix ?? sourceText.slice(start + selectedText.length, start + selectedText.length + 80),
  };
}

export function createAnnotationDraft(input: CreateAnnotationDraftInput): ReaderAnnotationDraft {
  const sourceText = input.sourceText;
  const selectedText = (input.selectedText ?? sourceText).trim();
  const foundOffset = selectedText ? sourceText.indexOf(selectedText) : -1;
  const startOffset = Math.max(0, Math.trunc(input.startOffset ?? (foundOffset >= 0 ? foundOffset : 0)));
  const endOffset = Math.max(startOffset + 1, Math.trunc(input.endOffset ?? (startOffset + Math.max(1, selectedText.length))));
  const quote = quoteForDraft(sourceText, selectedText, startOffset, input.prefix, input.suffix);
  const identity = {
    ...(input.materialRevision ? { material_revision: input.materialRevision } : {}),
    ...(input.contentSha256 ? { content_sha256: input.contentSha256.toLowerCase() } : {}),
  };
  let locator: ReaderAnnotationLocator;
  if (input.readerKind === "epub") {
    locator = { ...identity, reader_kind: "epub", kind: "epub_cfi", cfi: input.cfi ?? "", quote };
  } else if (input.readerKind === "media") {
    locator = { ...identity, reader_kind: "media", kind: "time_range", current_time: Math.max(0, input.currentTime ?? 0), ...(input.endTime !== undefined ? { end_time: input.endTime } : {}), ...(input.duration !== undefined ? { duration: input.duration } : {}), ...(input.segmentId ? { segment_id: input.segmentId } : {}), quote };
  } else if (input.readerKind === "pdf") {
    locator = { ...identity, reader_kind: "pdf", kind: "text_range", page: Math.max(1, Math.trunc(input.page ?? 1)), ...(input.totalPages ? { total_pages: input.totalPages } : {}), start_offset: startOffset, end_offset: endOffset, quote };
  } else {
    locator = { ...identity, reader_kind: input.readerKind, kind: "text_range", ...(input.segmentId ? { segment_id: input.segmentId } : {}), ...(input.segmentOrder !== undefined ? { segment_order: input.segmentOrder } : {}), ...(input.page !== undefined ? { page: input.page } : {}), ...(input.totalPages ? { total_pages: input.totalPages } : {}), start_offset: startOffset, end_offset: endOffset, quote };
  }
  return { material_id: input.materialId, ...(input.segmentId ? { segment_id: input.segmentId } : {}), reader_kind: input.readerKind, source_text: sourceText, quote, ...identity, locator };
}

/** Converts a reader locator into the shared persisted locator contract. */
export function toSourceLocator(locator: ReaderAnnotationLocator): SourceLocatorV1 {
  const identity = {
    version: 1 as const,
    reader_kind: locator.reader_kind,
    ...(locator.material_revision ? { material_revision: locator.material_revision } : {}),
    ...(locator.content_sha256 ? { content_sha256: locator.content_sha256 } : {}),
    ...(locator.quote ? { quote: locator.quote } : {}),
  };
  if (locator.kind === "text_range") {
    return {
      ...identity,
      kind: "text_range",
      ...(("segment_id" in locator && locator.segment_id) ? { segment_id: locator.segment_id } : {}),
      ...(("segment_order" in locator && locator.segment_order !== undefined) ? { segment_order: locator.segment_order } : {}),
      ...(("page" in locator && locator.page !== undefined) ? { page: locator.page } : {}),
      ...(("total_pages" in locator && locator.total_pages !== undefined) ? { total_pages: locator.total_pages } : {}),
      start_offset: locator.start_offset,
      end_offset: locator.end_offset,
      quote: locator.quote,
    };
  }
  if (locator.kind === "segment") {
    return { ...identity, kind: "segment", segment_order: locator.segment_order, total_segments: locator.total_segments ?? 1, ...(locator.segment_id ? { segment_id: locator.segment_id } : {}) };
  }
  if (locator.kind === "page") {
    return { ...identity, kind: "page", page: locator.page, ...(locator.total_pages !== undefined ? { total_pages: locator.total_pages } : {}) };
  }
  if (locator.reader_kind === "epub") {
    return { ...identity, kind: "epub_cfi", cfi: locator.cfi };
  }
  return { ...identity, kind: "time_range", current_time: locator.current_time, ...(locator.end_time !== undefined ? { end_time: locator.end_time } : {}), ...(locator.duration !== undefined ? { duration: locator.duration } : {}), ...(locator.segment_id ? { segment_id: locator.segment_id } : {}) };
}

export function useAnnotationResolution(annotation: ReaderAnnotationReference | null | undefined, context: ReaderAnnotationContext): AnnotationResolution | undefined {
  return useMemo(() => annotation ? resolveAnnotation(annotation, context) : undefined, [annotation, context]);
}
