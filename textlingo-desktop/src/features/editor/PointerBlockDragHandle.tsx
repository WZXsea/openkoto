import type { Editor } from "@tiptap/core";
import { GripVertical } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { reorderEditorBlock, type MaterialBlockPlacement } from "./reorderMaterialBlock";

const DRAG_THRESHOLD_PX = 4;
const AUTO_SCROLL_EDGE_PX = 56;
const AUTO_SCROLL_STEP_PX = 14;

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
    dragRef.current = null;
    stopAutoScroll();
    setDropTarget(null);
    setIsDragging(false);
  }, [stopAutoScroll]);

  const updateDropTarget = useCallback((clientX: number, clientY: number) => {
    const drag = dragRef.current;
    if (!drag?.dragging) return;
    const pointed = document.elementFromPoint(clientX, clientY);
    const element = materialBlockElement(pointed, editor.view.dom, validIdsRef.current);
    const target = element ? positionedBlock(element) : null;
    if (!target || target.id === drag.sourceId) {
      drag.targetId = null;
      drag.placement = null;
      setDropTarget(null);
      return;
    }

    const placement: MaterialBlockPlacement = clientY < target.rect.top + target.rect.height / 2 ? "before" : "after";
    drag.targetId = target.id;
    drag.placement = placement;
    setDropTarget({ ...target, placement });
  }, [editor]);

  autoScrollTickRef.current = () => {
    autoScrollFrameRef.current = null;
    const drag = dragRef.current;
    if (!drag?.dragging || !autoScrollAtEditorEdge(editor, drag.clientY)) return;
    updateDropTarget(drag.clientX, drag.clientY);
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
    };
  };

  const handlePointerMove = (event: React.PointerEvent<HTMLButtonElement>) => {
    const drag = dragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    event.preventDefault();

    if (!drag.dragging && Math.hypot(event.clientX - drag.startX, event.clientY - drag.startY) < DRAG_THRESHOLD_PX) return;
    if (!drag.dragging) {
      drag.dragging = true;
      setIsDragging(true);
    }

    drag.clientX = event.clientX;
    drag.clientY = event.clientY;
    const scrolled = autoScrollAtEditorEdge(editor, event.clientY);
    updateDropTarget(event.clientX, event.clientY);
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
