import type { JSONContent } from "@tiptap/core";
import type { RefObject, ReactNode } from "react";

import type { ReaderAnnotationLocator } from "../reader";
import type { MaterialDocumentSegment, MaterialEditorBlock } from "./types";

type ReaderViewMode = "original" | "bilingual" | "translation";

interface StructuredDocumentReaderProps {
  blocks: MaterialEditorBlock[];
  liveSegments?: MaterialDocumentSegment[];
  fontSize: number;
  viewMode: ReaderViewMode;
  selectedSegmentId: string | null;
  activeSegmentRef?: RefObject<HTMLElement | null>;
  annotationLocator?: ReaderAnnotationLocator;
  annotationResolved?: boolean;
  onSegmentClick: (id: string) => void;
}

interface InlineRun {
  text: string;
  marks: JSONContent["marks"];
  start: number;
  end: number;
}

function safeHref(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  return /^(https?:|mailto:)/i.test(value) ? value : undefined;
}

function renderMarkedText(text: string, marks: JSONContent["marks"], key: string): ReactNode {
  let node: ReactNode = text;
  for (const mark of marks ?? []) {
    if (mark.type === "bold") node = <strong key={`${key}-bold`}>{node}</strong>;
    else if (mark.type === "italic") node = <em key={`${key}-italic`}>{node}</em>;
    else if (mark.type === "code") node = <code key={`${key}-code`}>{node}</code>;
    else if (mark.type === "link") {
      const href = safeHref(mark.attrs?.href);
      if (href) node = <a key={`${key}-link`} href={href} target="_blank" rel="noreferrer">{node}</a>;
    }
  }
  return node;
}

function inlineRuns(block: MaterialEditorBlock): InlineRun[] {
  const saved = block.attrs?.tiptap_content;
  const content = Array.isArray(saved) ? saved as JSONContent[] : [{ type: "text", text: block.text }];
  const runs: InlineRun[] = [];
  let offset = 0;
  const visit = (node: JSONContent) => {
    if (node.type === "hardBreak") {
      runs.push({ text: "\n", marks: [], start: offset, end: offset + 1 });
      offset += 1;
      return;
    }
    if (typeof node.text === "string") {
      runs.push({ text: node.text, marks: node.marks, start: offset, end: offset + node.text.length });
      offset += node.text.length;
      return;
    }
    for (const child of node.content ?? []) visit(child);
  };
  for (const node of content) visit(node);
  return runs;
}

function renderRange(runs: InlineRun[], start: number, end: number, highlight?: [number, number]): ReactNode[] {
  const nodes: ReactNode[] = [];
  for (const [runIndex, run] of runs.entries()) {
    const sliceStart = Math.max(start, run.start);
    const sliceEnd = Math.min(end, run.end);
    if (sliceStart >= sliceEnd) continue;
    const pieces = highlight
      ? [sliceStart, Math.max(sliceStart, Math.min(sliceEnd, highlight[0])), Math.max(sliceStart, Math.min(sliceEnd, highlight[1])), sliceEnd]
      : [sliceStart, sliceEnd];
    const boundaries = [...new Set(pieces)].sort((left, right) => left - right);
    for (let index = 0; index < boundaries.length - 1; index += 1) {
      const from = boundaries[index];
      const to = boundaries[index + 1];
      if (from >= to) continue;
      const text = run.text.slice(from - run.start, to - run.start);
      const key = `${runIndex}-${from}-${to}`;
      if (text === "\n") {
        nodes.push(<br key={`${key}-break`} />);
        continue;
      }
      const marked = renderMarkedText(text, run.marks, key);
      nodes.push(highlight && from >= highlight[0] && to <= highlight[1]
        ? <mark key={`${key}-highlight`} data-testid="article-annotation-highlight" className="rounded-sm bg-primary/25 text-foreground">{marked}</mark>
        : <span key={key}>{marked}</span>);
    }
  }
  return nodes;
}

function segmentRanges(block: MaterialEditorBlock): Array<{ segment: MaterialDocumentSegment; start: number; end: number }> {
  let cursor = 0;
  return (block.segments ?? []).map((segment) => {
    const located = block.text.indexOf(segment.text, cursor);
    const start = located >= 0 ? located : cursor;
    const end = Math.min(block.text.length, start + segment.text.length);
    cursor = end;
    return { segment, start, end };
  });
}

function BlockInlineContent({
  block,
  fontSize,
  viewMode,
  selectedSegmentId,
  activeSegmentRef,
  annotationLocator,
  annotationResolved,
  onSegmentClick,
}: Omit<StructuredDocumentReaderProps, "blocks"> & { block: MaterialEditorBlock }) {
  const runs = inlineRuns(block);
  const ranges = segmentRanges(block);
  if (ranges.length === 0) return <>{renderRange(runs, 0, block.text.length)}</>;
  const output: ReactNode[] = [];
  let cursor = 0;
  ranges.forEach(({ segment, start, end }, index) => {
    if (start > cursor) output.push(...renderRange(runs, cursor, start));
    const selected = segment.id === selectedSegmentId;
    const annotated = annotationResolved
      && annotationLocator?.kind === "text_range"
      && annotationLocator.reader_kind === "article"
      && (annotationLocator.segment_id === segment.id || annotationLocator.segment_order === segment.order);
    const highlight: [number, number] | undefined = annotated
      ? [start + annotationLocator.start_offset, start + annotationLocator.end_offset]
      : undefined;
    output.push(
      <span
        key={segment.id ?? `${block.id}-${index}`}
        ref={selected ? activeSegmentRef : undefined}
        data-reader-segment-id={segment.id}
        data-reader-segment-order={segment.order}
        data-annotation-active={annotated ? "true" : undefined}
        onClick={() => segment.id && onSegmentClick(segment.id)}
        className={`decoration-clone rounded-md border-2 px-1 py-0.5 transition-colors ${selected ? "border-primary bg-primary/20 ring-2 ring-primary/20" : "cursor-pointer border-transparent hover:border-border hover:bg-accent"}`}
        style={{ fontSize: `${fontSize}px`, WebkitBoxDecorationBreak: "clone", boxDecorationBreak: "clone" }}
      >
        {viewMode === "translation" && segment.translation
          ? segment.translation
          : renderRange(runs, start, end, highlight)}
      </span>,
    );
    if (index < ranges.length - 1) output.push(" ");
    cursor = end;
  });
  if (cursor < block.text.length) output.push(...renderRange(runs, cursor, block.text.length));
  return <>{output}</>;
}

function SegmentDetails({ block, selectedSegmentId, viewMode, fontSize }: {
  block: MaterialEditorBlock;
  selectedSegmentId: string | null;
  viewMode: ReaderViewMode;
  fontSize: number;
}) {
  return <>{(block.segments ?? []).map((segment) => {
    const visible = segment.id === selectedSegmentId || (viewMode === "bilingual" && Boolean(segment.translation));
    if (!visible || (!segment.reading_text && !(viewMode === "bilingual" && segment.translation))) return null;
    return (
      <div key={`detail-${segment.id}`} className="mt-3 rounded-xl border border-border bg-muted/30 px-4 py-3">
        {segment.reading_text && viewMode !== "translation" && <p className="mb-2 font-mono text-muted-foreground" style={{ fontSize: `${fontSize * 0.85}px` }}>{segment.reading_text}</p>}
        {viewMode === "bilingual" && segment.translation && <p className="text-primary" style={{ fontSize: `${fontSize * 0.95}px` }}>{segment.translation}</p>}
      </div>
    );
  })}</>;
}

function blockBody(block: MaterialEditorBlock, props: Omit<StructuredDocumentReaderProps, "blocks">) {
  return <BlockInlineContent block={block} {...props} />;
}

export function StructuredDocumentReader(props: StructuredDocumentReaderProps) {
  const liveById = new Map((props.liveSegments ?? []).map((segment) => [segment.id, segment]));
  const ordered = props.blocks
    .map((block) => ({
      ...block,
      segments: block.segments?.map((segment) => ({ ...segment, ...(liveById.get(segment.id) ?? {}) })),
    }))
    .sort((left, right) => left.block_order - right.block_order);
  const rendered: ReactNode[] = [];
  for (let index = 0; index < ordered.length; index += 1) {
    const block = ordered[index];
    if (block.block_type === "list_item") {
      const listType = block.attrs.list_type === "ordered" ? "ordered" : "bullet";
      const items: MaterialEditorBlock[] = [];
      while (index < ordered.length && ordered[index].block_type === "list_item" && (ordered[index].attrs.list_type === "ordered" ? "ordered" : "bullet") === listType) {
        items.push(ordered[index]);
        index += 1;
      }
      index -= 1;
      const ListTag = listType === "ordered" ? "ol" : "ul";
      rendered.push(
        <ListTag key={`list-${items[0].id ?? index}`} className={`mb-6 space-y-2 pl-7 ${listType === "ordered" ? "list-decimal" : "list-disc"}`}>
          {items.map((item) => <li key={item.id ?? item.block_order}>{blockBody(item, props)}<SegmentDetails block={item} selectedSegmentId={props.selectedSegmentId} viewMode={props.viewMode} fontSize={props.fontSize} /></li>)}
        </ListTag>,
      );
      continue;
    }
    if (block.block_type === "divider") {
      rendered.push(<hr key={block.id ?? index} className="my-8 border-border" />);
      continue;
    }
    const content = blockBody(block, props);
    const details = <SegmentDetails block={block} selectedSegmentId={props.selectedSegmentId} viewMode={props.viewMode} fontSize={props.fontSize} />;
    if (block.block_type === "heading") {
      rendered.push(block.attrs.level === 3
        ? <div key={block.id ?? index}><h3 className="mb-3 mt-7 text-xl font-semibold">{content}</h3>{details}</div>
        : <div key={block.id ?? index}><h2 className="mb-4 mt-8 text-2xl font-semibold">{content}</h2>{details}</div>);
    } else if (block.block_type === "quote") {
      rendered.push(<blockquote key={block.id ?? index} className="mb-6 border-l-4 border-primary/35 pl-4 italic text-muted-foreground">{content}{details}</blockquote>);
    } else {
      rendered.push(<div key={block.id ?? index} className="mb-6"><p className="leading-[2] text-foreground">{content}</p>{details}</div>);
    }
  }
  return <article className="openkoto-reader-font mx-auto max-w-3xl pb-20" data-testid="structured-document-reader">{rendered}</article>;
}
