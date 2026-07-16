import type { Editor } from "@tiptap/core";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { PointerBlockDragHandle } from "./PointerBlockDragHandle";

const { reorderEditorBlockMock } = vi.hoisted(() => ({
  reorderEditorBlockMock: vi.fn(),
}));

vi.mock("./reorderMaterialBlock", () => ({
  reorderEditorBlock: reorderEditorBlockMock,
}));

function rect(top: number, height = 40, left = 100, width = 480): DOMRect {
  return {
    bottom: top + height,
    height,
    left,
    right: left + width,
    top,
    width,
    x: left,
    y: top,
    toJSON: () => ({}),
  } as DOMRect;
}

function mountEditorDom() {
  const scrollContainer = document.createElement("div");
  scrollContainer.dataset.testid = "material-block-editor-scroll";
  scrollContainer.getBoundingClientRect = vi.fn(() => rect(0, 200, 0, 640));
  Object.defineProperties(scrollContainer, {
    clientHeight: { configurable: true, value: 200 },
    scrollHeight: { configurable: true, value: 1_000 },
  });
  scrollContainer.scrollTop = 100;

  const root = document.createElement("div");
  root.className = "tiptap";

  const quote = document.createElement("blockquote");
  quote.dataset.blockId = "quote-block";
  quote.getBoundingClientRect = vi.fn(() => rect(20));
  const quoteParagraph = document.createElement("p");
  quoteParagraph.textContent = "Quote";
  quote.append(quoteParagraph);

  const list = document.createElement("ul");
  const listItem = document.createElement("li");
  listItem.dataset.blockId = "list-block";
  listItem.getBoundingClientRect = vi.fn(() => rect(80));
  const listParagraph = document.createElement("p");
  listParagraph.textContent = "List item";
  listItem.append(listParagraph);
  list.append(listItem);

  root.append(quote, list);
  scrollContainer.append(root);
  document.body.append(scrollContainer);

  return { scrollContainer, root, quote, quoteParagraph, listItem, listParagraph };
}

function editorFor(root: HTMLElement): Editor {
  return {
    view: { dom: root },
  } as unknown as Editor;
}

function preparePointerCapture(handle: HTMLElement) {
  Object.defineProperties(handle, {
    setPointerCapture: { configurable: true, value: vi.fn() },
    hasPointerCapture: { configurable: true, value: vi.fn(() => true) },
    releasePointerCapture: { configurable: true, value: vi.fn() },
  });
}

function installPointerEventPolyfill() {
  class TestPointerEvent extends MouseEvent {
    pointerId: number;

    constructor(type: string, init: MouseEventInit & { pointerId?: number } = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 0;
    }
  }

  vi.stubGlobal("PointerEvent", TestPointerEvent);
}

let nextAnimationFrameId = 1;
let animationFrames = new Map<number, FrameRequestCallback>();

function installAnimationFrameHarness() {
  nextAnimationFrameId = 1;
  animationFrames = new Map();
  vi.stubGlobal("requestAnimationFrame", vi.fn((callback: FrameRequestCallback) => {
    const id = nextAnimationFrameId;
    nextAnimationFrameId += 1;
    animationFrames.set(id, callback);
    return id;
  }));
  vi.stubGlobal("cancelAnimationFrame", vi.fn((id: number) => {
    animationFrames.delete(id);
  }));
}

function runNextAnimationFrame(): boolean {
  const next = animationFrames.entries().next();
  if (next.done) return false;
  const [id, callback] = next.value;
  animationFrames.delete(id);
  callback(16 * id);
  return true;
}

function pointAt(element: Element) {
  Object.defineProperty(document, "elementFromPoint", {
    configurable: true,
    value: vi.fn(() => element),
  });
}

function hoverNestedTarget(target: Element) {
  fireEvent.pointerMove(target, { pointerId: 1, clientX: 120, clientY: 30 });
  return screen.getByTestId("material-block-drag-handle");
}

describe("PointerBlockDragHandle", () => {
  beforeEach(() => {
    reorderEditorBlockMock.mockReset();
    reorderEditorBlockMock.mockReturnValue(true);
    installPointerEventPolyfill();
    installAnimationFrameHarness();
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    Reflect.deleteProperty(document, "elementFromPoint");
    document.body.innerHTML = "";
  });

  it("uses the nearest valid quote/list block id when pointer events originate in nested content", () => {
    const { root, quoteParagraph, listParagraph } = mountEditorDom();
    render(<PointerBlockDragHandle editor={editorFor(root)} validBlockIds={["quote-block", "list-block"]} />);

    const quoteHandle = hoverNestedTarget(quoteParagraph);
    expect(quoteHandle).toHaveStyle({ top: "20px" });

    fireEvent.pointerMove(listParagraph, { pointerId: 1, clientX: 120, clientY: 90 });
    expect(screen.getByTestId("material-block-drag-handle")).toHaveStyle({ top: "80px" });
  });

  it("does not start a reorder before the four pixel threshold", () => {
    const { root, quoteParagraph, listParagraph } = mountEditorDom();
    render(<PointerBlockDragHandle editor={editorFor(root)} validBlockIds={["quote-block", "list-block"]} />);
    const handle = hoverNestedTarget(quoteParagraph);
    preparePointerCapture(handle);
    pointAt(listParagraph);

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 100, clientY: 20 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 103, clientY: 20 });

    expect(handle).toHaveAttribute("aria-pressed", "false");
    expect(screen.queryByTestId("material-block-drop-indicator")).not.toBeInTheDocument();
    fireEvent.pointerUp(handle, { pointerId: 1, clientX: 103, clientY: 20 });
    expect(reorderEditorBlockMock).not.toHaveBeenCalled();
  });

  it("shows an insertion line and reorders after a captured pointer drag", () => {
    const { root, quoteParagraph, listParagraph } = mountEditorDom();
    render(<PointerBlockDragHandle editor={editorFor(root)} validBlockIds={["quote-block", "list-block"]} />);
    const handle = hoverNestedTarget(quoteParagraph);
    preparePointerCapture(handle);
    pointAt(listParagraph);

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 100, clientY: 20 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 110, clientY: 105 });

    expect(handle).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("material-block-drop-indicator")).toHaveStyle({ top: "120px" });
    fireEvent.pointerUp(handle, { pointerId: 1, clientX: 110, clientY: 105 });

    expect(reorderEditorBlockMock).toHaveBeenCalledTimes(1);
    expect(reorderEditorBlockMock).toHaveBeenCalledWith(
      expect.anything(),
      "quote-block",
      "list-block",
      "after",
    );
    expect(screen.queryByTestId("material-block-drop-indicator")).not.toBeInTheDocument();
  });

  it("continues scrolling at the editor edge and stops when the pointer leaves the edge", () => {
    const { scrollContainer, root, quoteParagraph, listParagraph } = mountEditorDom();
    render(<PointerBlockDragHandle editor={editorFor(root)} validBlockIds={["quote-block", "list-block"]} />);
    const handle = hoverNestedTarget(quoteParagraph);
    preparePointerCapture(handle);
    pointAt(listParagraph);

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 100, clientY: 20 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 110, clientY: 190 });

    expect(scrollContainer.scrollTop).toBe(114);
    expect(animationFrames.size).toBe(1);
    expect(runNextAnimationFrame()).toBe(true);
    expect(scrollContainer.scrollTop).toBe(128);
    expect(animationFrames.size).toBe(1);
    expect(runNextAnimationFrame()).toBe(true);
    expect(scrollContainer.scrollTop).toBe(142);
    expect(animationFrames.size).toBe(1);

    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 110, clientY: 100 });
    expect(animationFrames.size).toBe(0);
    expect(runNextAnimationFrame()).toBe(false);
    expect(scrollContainer.scrollTop).toBe(142);
  });

  it.each([
    ["pointer cancel", (handle: HTMLElement) => fireEvent.pointerCancel(handle, { pointerId: 1 })],
    ["window blur", () => fireEvent.blur(window)],
  ])("cleans up continuous edge scrolling on %s", (_label, cancel) => {
    const { scrollContainer, root, quoteParagraph, listParagraph } = mountEditorDom();
    render(<PointerBlockDragHandle editor={editorFor(root)} validBlockIds={["quote-block", "list-block"]} />);
    const handle = hoverNestedTarget(quoteParagraph);
    preparePointerCapture(handle);
    pointAt(listParagraph);

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 100, clientY: 20 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 110, clientY: 190 });
    expect(scrollContainer.scrollTop).toBe(114);
    expect(animationFrames.size).toBe(1);

    cancel(handle);
    expect(animationFrames.size).toBe(0);
    expect(runNextAnimationFrame()).toBe(false);
    expect(scrollContainer.scrollTop).toBe(114);
    expect(reorderEditorBlockMock).not.toHaveBeenCalled();
  });

  it("cancels pointer state without modifying the document", () => {
    const { root, quoteParagraph, listParagraph } = mountEditorDom();
    render(<PointerBlockDragHandle editor={editorFor(root)} validBlockIds={["quote-block", "list-block"]} />);
    const handle = hoverNestedTarget(quoteParagraph);
    preparePointerCapture(handle);
    pointAt(listParagraph);

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 100, clientY: 20 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 110, clientY: 70 });
    fireEvent.pointerCancel(handle, { pointerId: 1 });
    fireEvent.pointerUp(handle, { pointerId: 1, clientX: 110, clientY: 70 });

    expect(reorderEditorBlockMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("material-block-drop-indicator")).not.toBeInTheDocument();
  });

  it.each([
    ["Escape", () => fireEvent.keyDown(document, { key: "Escape" })],
    ["window blur", () => fireEvent.blur(window)],
  ])("cancels an active drag on %s", (_label, cancel) => {
    const { root, quoteParagraph, listParagraph } = mountEditorDom();
    render(<PointerBlockDragHandle editor={editorFor(root)} validBlockIds={["quote-block", "list-block"]} />);
    const handle = hoverNestedTarget(quoteParagraph);
    preparePointerCapture(handle);
    pointAt(listParagraph);

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 100, clientY: 20 });
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 110, clientY: 70 });
    expect(screen.getByTestId("material-block-drop-indicator")).toBeInTheDocument();

    cancel();
    fireEvent.pointerUp(handle, { pointerId: 1, clientX: 110, clientY: 70 });

    expect(reorderEditorBlockMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("material-block-drop-indicator")).not.toBeInTheDocument();
  });
});
