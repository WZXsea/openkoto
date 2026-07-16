import type { Editor } from "@tiptap/core";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { Plugin, PluginKey } from "@tiptap/pm/state";
import { Decoration, DecorationSet } from "@tiptap/pm/view";
import { GripVertical } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { reorderEditorBlock, type MaterialBlockPlacement } from "./reorderMaterialBlock";

const DRAG_THRESHOLD_PX = 4;
const AUTO_SCROLL_EDGE_PX = 56;
const AUTO_SCROLL_STEP_PX = 14;
const DRAG_SOURCE_PLUGIN_KEY = new PluginKey("openkotoPointerDragSource");

interface PointerBlockDragHandleProps {
  editor: Editor;
  validBlockIds: readonly string[];
  disabled?: boolean;
}

interface PositionedBlock {
  id: string;
  rect: DOMRect;
}

interface PointerDragState {
  pointerId: number;
  sourceId: string;
  startX: number;
  startY: number;
  dragging: boolean;
  targetId: string | null;
  placement: MaterialBlockPlacement | null;
  clientX: number;
  clientY: number;
  sourceElement: HTMLElement | null;
  preview: DragPreviewSnapshot | null;
}

interface DragPreviewSnapshot {
  html: string;
  text: string;
  width: number;
  height: number;
  pointerOffsetY: number;
}

interface DragPreviewState extends DragPreviewSnapshot {
  clientX: number;
  clientY: number;
}

function materialBlockElement(
  target: Element | null,
  editorRoot: HTMLElement,
  validIds: ReadonlySet<string>,
): HTMLElement | null {
  let current: Element | null = target;
  while (current && current !== editorRoot) {
    if (current instanceof HTMLElement) {
      const blockId = current.dataset.blockId;
      if (blockId && validIds.has(blockId)) return current;
    }
    current = current.parentElement;
  }
  return null;
}

function positionedBlock(element: HTMLElement): PositionedBlock | null {
  const id = element.dataset.blockId;
  return id ? { id, rect: element.getBoundingClientRect() } : null;
}

function positionedValidBlocks(
  editorRoot: HTMLElement,
  validIds: ReadonlySet<string>,
): PositionedBlock[] {
  const seen = new Set<string>();
  return Array.from(editorRoot.querySelectorAll<HTMLElement>("[data-block-id]"))
    .map((element) => positionedBlock(element))
    .filter((block): block is PositionedBlock => {
      if (!block || !validIds.has(block.id) || seen.has(block.id)) return false;
      seen.add(block.id);
      return true;
    })
    .sort((left, right) => left.rect.top - right.rect.top || left.rect.left - right.rect.left);
}

function verticalDropTarget(
  blocks: PositionedBlock[],
  sourceId: string,
  clientY: number,
): (PositionedBlock & { placement: MaterialBlockPlacement }) | null {
  const candidates = blocks.filter((block) => block.id !== sourceId);
  if (candidates.length === 0) return null;

  const target = candidates.find((block) => clientY <= block.rect.top + block.rect.height / 2);
  if (target) return { ...target, placement: "before" };

  const last = candidates[candidates.length - 1];
  return last ? { ...last, placement: "after" } : null;
}

function sourceElementForBlock(editorRoot: HTMLElement, blockId: string): HTMLElement | null {
  return Array.from(editorRoot.querySelectorAll<HTMLElement>("[data-block-id]"))
    .find((element) => element.dataset.blockId === blockId) ?? null;
}

function sourceBlockRange(
  document: ProseMirrorNode,
  sourceId: string,
): { from: number; to: number } | null {
  let result: { from: number; to: number } | null = null;
  document.descendants((node, pos) => {
    if (node.attrs.blockId !== sourceId) return result === null;
    result = { from: pos, to: pos + node.nodeSize };
    return false;
  });
  return result;
}

function dragSourcePlugin(sourceId: string): Plugin {
  return new Plugin({
    key: DRAG_SOURCE_PLUGIN_KEY,
    props: {
      decorations(state) {
        const range = sourceBlockRange(state.doc, sourceId);
        return range
          ? DecorationSet.create(state.doc, [
            Decoration.node(range.from, range.to, { class: "openkoto-editor-drag-source" }),
          ])
          : DecorationSet.empty;
      },
    },
  });
}

function autoScrollAtEditorEdge(editor: Editor, clientY: number): boolean {
  const scrollContainer = editor.view.dom.closest<HTMLElement>("[data-testid='material-block-editor-scroll']");
  if (!scrollContainer) return false;
  const rect = scrollContainer.getBoundingClientRect();
  if (clientY < rect.top + AUTO_SCROLL_EDGE_PX && scrollContainer.scrollTop > 0) {
    scrollContainer.scrollTop -= AUTO_SCROLL_STEP_PX;
    return true;
  } else if (
    clientY > rect.bottom - AUTO_SCROLL_EDGE_PX
    && scrollContainer.scrollTop + scrollContainer.clientHeight < scrollContainer.scrollHeight
  ) {
    scrollContainer.scrollTop += AUTO_SCROLL_STEP_PX;
    return true;
  }
  return false;
}

export function PointerBlockDragHandle({ editor, validBlockIds, disabled = false }: PointerBlockDragHandleProps) {
  const validIdsRef = useRef<ReadonlySet<string>>(new Set(validBlockIds));
  const corridorRef = useRef<HTMLDivElement | null>(null);
  const handleRef = useRef<HTMLButtonElement | null>(null);
  const dragRef = useRef<PointerDragState | null>(null);
  const autoScrollFrameRef = useRef<number | null>(null);
  const autoScrollTickRef = useRef<() => void>(() => {});
  const [activeBlock, setActiveBlock] = useState<PositionedBlock | null>(null);
  const [dropTarget, setDropTarget] = useState<(PositionedBlock & { placement: MaterialBlockPlacement }) | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [dragPreview, setDragPreview] = useState<DragPreviewState | null>(null);

  validIdsRef.current = new Set(validBlockIds);

  const stopAutoScroll = useCallback(() => {
    if (autoScrollFrameRef.current === null) return;
    cancelAnimationFrame(autoScrollFrameRef.current);
    autoScrollFrameRef.current = null;
  }, []);

  const clearDrag = useCallback((releaseCapture = true) => {
    const drag = dragRef.current;
    const handle = handleRef.current;
    if (releaseCapture && drag && handle?.hasPointerCapture(drag.pointerId)) {
      handle.releasePointerCapture(drag.pointerId);
    }
    if (typeof editor.unregisterPlugin === "function") editor.unregisterPlugin(DRAG_SOURCE_PLUGIN_KEY);
    drag?.sourceElement?.classList.remove("openkoto-editor-drag-source");
    dragRef.current = null;
    stopAutoScroll();
    setDropTarget(null);
    setIsDragging(false);
    setDragPreview(null);
    setActiveBlock(null);
  }, [editor, stopAutoScroll]);

  const updateDropTarget = useCallback((clientY: number) => {
    const drag = dragRef.current;
    if (!drag?.dragging) return;
    const target = verticalDropTarget(
      positionedValidBlocks(editor.view.dom, validIdsRef.current),
      drag.sourceId,
      clientY,
    );
    if (!target) {
      drag.targetId = null;
      drag.placement = null;
      setDropTarget(null);
      return;
    }

    drag.targetId = target.id;
    drag.placement = target.placement;
    setDropTarget(target);
  }, [editor]);

  autoScrollTickRef.current = () => {
    autoScrollFrameRef.current = null;
    const drag = dragRef.current;
    if (!drag?.dragging || !autoScrollAtEditorEdge(editor, drag.clientY)) return;
    updateDropTarget(drag.clientY);
    autoScrollFrameRef.current = requestAnimationFrame(() => autoScrollTickRef.current());
  };

  const continueAutoScroll = useCallback(() => {
    if (autoScrollFrameRef.current !== null) return;
    autoScrollFrameRef.current = requestAnimationFrame(() => autoScrollTickRef.current());
  }, []);

  const refreshActiveBlock = useCallback(() => {
    setActiveBlock((current) => {
      if (!current) return null;
      const element = Array.from(editor.view.dom.querySelectorAll<HTMLElement>("[data-block-id]"))
        .find((candidate) => candidate.dataset.blockId === current.id);
      return element ? positionedBlock(element) : null;
    });
  }, [editor]);

  useEffect(() => {
    const editorRoot = editor.view.dom;

    const handlePointerMove = (event: PointerEvent) => {
      if (disabled || dragRef.current) return;
      const element = materialBlockElement(event.target as Element | null, editorRoot, validIdsRef.current);
      if (element) setActiveBlock(positionedBlock(element));
    };
    const handlePointerLeave = (event: PointerEvent) => {
      const relatedTarget = event.relatedTarget;
      if (
        dragRef.current
        || (relatedTarget instanceof Node && corridorRef.current?.contains(relatedTarget))
      ) return;
      setActiveBlock(null);
    };
    const handleScroll = () => refreshActiveBlock();

    editorRoot.addEventListener("pointermove", handlePointerMove);
    editorRoot.addEventListener("pointerleave", handlePointerLeave);
    document.addEventListener("scroll", handleScroll, true);
    window.addEventListener("resize", handleScroll);
    return () => {
      editorRoot.removeEventListener("pointermove", handlePointerMove);
      editorRoot.removeEventListener("pointerleave", handlePointerLeave);
      document.removeEventListener("scroll", handleScroll, true);
      window.removeEventListener("resize", handleScroll);
    };
  }, [disabled, editor, refreshActiveBlock]);

  useEffect(() => {
    const cancelDrag = () => clearDrag();
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !dragRef.current) return;
      event.preventDefault();
      clearDrag();
    };
    window.addEventListener("blur", cancelDrag);
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("blur", cancelDrag);
      document.removeEventListener("keydown", handleKeyDown);
      clearDrag();
    };
  }, [clearDrag]);

  const handlePointerDown = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (disabled || event.button !== 0 || !activeBlock) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    const sourceElement = sourceElementForBlock(editor.view.dom, activeBlock.id);
    const sourceRect = sourceElement?.getBoundingClientRect() ?? activeBlock.rect;
    const preview = sourceElement ? {
      html: sourceElement.outerHTML,
      text: sourceElement.textContent?.trim() ?? "",
      width: sourceRect.width,
      height: sourceRect.height,
      pointerOffsetY: Math.min(Math.max(event.clientY - sourceRect.top, 0), sourceRect.height),
    } : null;
    dragRef.current = {
      pointerId: event.pointerId,
      sourceId: activeBlock.id,
      startX: event.clientX,
      startY: event.clientY,
      dragging: false,
      targetId: null,
      placement: null,
      clientX: event.clientX,
      clientY: event.clientY,
      sourceElement,
      preview,
    };
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLButtonElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.preventDefault();

    if (!drag.dragging && Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < DRAG_THRESHOLD_PX) return;
    if (!drag.dragging) {
      drag.dragging = true;
      if (typeof editor.registerPlugin === "function") {
        editor.unregisterPlugin(DRAG_SOURCE_PLUGIN_KEY);
        editor.registerPlugin(dragSourcePlugin(drag.sourceId));
      } else {
        drag.sourceElement?.classList.add("openkoto-editor-drag-source");
      }
      setIsDragging(true);
    }

    drag.clientX = event.clientX;
    drag.clientY = event.clientY;
    if (drag.preview) {
      setDragPreview({ ...drag.preview, clientX: event.clientX, clientY: event.clientY });
    }
    const scrolled = autoScrollAtEditorEdge(editor, event.clientY);
    updateDropTarget(event.clientY);
    if (scrolled) continueAutoScroll();
    else stopAutoScroll();
  };

  const handlePointerUp = (event: React.PointerEvent<HTMLButtonElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.preventDefault();
    const { dragging, sourceId, targetId, placement } = drag;
    clearDrag();
    if (dragging && targetId && placement) reorderEditorBlock(editor, sourceId, targetId, placement);
  };

  const handlePointerCancel = (event: React.PointerEvent<HTMLButtonElement>) => {
    if (dragRef.current?.pointerId === event.pointerId) clearDrag(false);
  };

  const handleCorridorPointerLeave = (event: React.PointerEvent<HTMLDivElement>) => {
    const relatedTarget = event.nativeEvent.relatedTarget;
    if (dragRef.current || (relatedTarget instanceof Node && editor.view.dom.contains(relatedTarget))) return;
    setActiveBlock(null);
  };

  if (disabled || !activeBlock) return null;

  return (
    <>
      <div
        ref={corridorRef}
        className="openkoto-editor-drag-corridor"
        data-testid="material-block-drag-corridor"
        style={{
          left: activeBlock.rect.left - 38,
          top: activeBlock.rect.top,
          height: Math.max(30, activeBlock.rect.height),
        }}
        onPointerLeave={handleCorridorPointerLeave}
      >
        <button
          ref={handleRef}
          type="button"
          draggable={false}
          className="openkoto-editor-drag-handle openkoto-editor-pointer-handle"
          data-testid="material-block-drag-handle"
          style={{ left: activeBlock.rect.left - 34, top: activeBlock.rect.top }}
          aria-label="拖动当前块"
          aria-pressed={isDragging}
          aria-grabbed={isDragging}
          title="拖动当前块"
          onDragStart={(event) => event.preventDefault()}
          onPointerDown={handlePointerDown}
          onPointerMove={handlePointerMove}
          onPointerUp={handlePointerUp}
          onPointerCancel={handlePointerCancel}
        >
          <GripVertical size={17} />
        </button>
      </div>
      {dragPreview && (
        <div
          className="openkoto-editor-drag-preview"
          data-testid="material-block-drag-preview"
          style={{
            left: dragPreview.clientX + 18,
            top: dragPreview.clientY - dragPreview.pointerOffsetY,
            width: dragPreview.width,
            minHeight: dragPreview.height,
          }}
          aria-hidden="true"
          title={dragPreview.text}
        >
          <div
            className="openkoto-editor-drag-preview-content"
            dangerouslySetInnerHTML={{ __html: dragPreview.html }}
          />
        </div>
      )}
      {dropTarget && (
        <div
          className="openkoto-editor-drop-indicator"
          data-testid="material-block-drop-indicator"
          style={{
            left: dropTarget.rect.left,
            top: dropTarget.placement === "before" ? dropTarget.rect.top : dropTarget.rect.bottom,
            width: dropTarget.rect.width,
          }}
          aria-hidden="true"
        />
      )}
    </>
  );
}
