import { cleanup, createEvent, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useEffect } from "react";

import { PdfReader } from "./PdfReader";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("react-pdf", () => ({
  pdfjs: {
    GlobalWorkerOptions: {
      workerSrc: "",
    },
  },
  Document: ({
    children,
    onLoadSuccess,
  }: {
    children: React.ReactNode;
    onLoadSuccess?: ({ numPages }: { numPages: number }) => void;
  }) => {
    useEffect(() => {
      onLoadSuccess?.({ numPages: 5 });
    }, [onLoadSuccess]);

    return <div data-testid="pdf-document">{children}</div>;
  },
  Page: ({ pageNumber }: { pageNumber: number }) => <div data-testid="pdf-page">Page {pageNumber}</div>,
}));

vi.mock("./BookmarkSidebar", () => ({
  BookmarkSidebar: () => null,
}));

function setScrollMetrics(
  element: HTMLElement,
  { clientHeight, scrollHeight, scrollTop }: { clientHeight: number; scrollHeight: number; scrollTop: number },
) {
  Object.defineProperty(element, "clientHeight", {
    value: clientHeight,
    configurable: true,
  });
  Object.defineProperty(element, "scrollHeight", {
    value: scrollHeight,
    configurable: true,
  });
  element.scrollTop = scrollTop;
}

describe("PdfReader", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn(() => null),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });
  });

  afterEach(() => {
    cleanup();
  });

  it("anchors the next-page button to the reader container instead of the window edge", async () => {
    render(<PdfReader bookPath="http://127.0.0.1/test.pdf" title="Test PDF" />);

    const nextButton = await screen.findByTitle("下一页");

    expect(nextButton.className).not.toContain("fixed");
    expect(nextButton.className).toContain("absolute");
  });

  it("scrolls within an oversized PDF page instead of changing pages immediately", async () => {
    render(<PdfReader bookPath="http://127.0.0.1/test.pdf" title="Test PDF" />);

    await waitFor(() => {
      expect(screen.getByText("1/5")).toBeInTheDocument();
    });

    const contentArea = screen.getByTitle("下一页").parentElement as HTMLElement | null;
    expect(contentArea).not.toBeNull();
    setScrollMetrics(contentArea as HTMLElement, {
      clientHeight: 600,
      scrollHeight: 1400,
      scrollTop: 300,
    });

    const wheelEvent = createEvent.wheel(contentArea as HTMLElement, { deltaY: 100 });
    fireEvent(contentArea as HTMLElement, wheelEvent);

    expect(wheelEvent.defaultPrevented).toBe(false);
    expect(screen.getByText("1/5")).toBeInTheDocument();
  });

  it("changes pages with the mouse wheel only after reaching the vertical scroll edge", async () => {
    render(<PdfReader bookPath="http://127.0.0.1/test.pdf" title="Test PDF" />);

    await waitFor(() => {
      expect(screen.getByText("1/5")).toBeInTheDocument();
    });

    const contentArea = screen.getByTitle("下一页").parentElement as HTMLElement | null;
    expect(contentArea).not.toBeNull();

    setScrollMetrics(contentArea as HTMLElement, {
      clientHeight: 600,
      scrollHeight: 1400,
      scrollTop: 800,
    });

    fireEvent.wheel(contentArea as HTMLElement, { deltaY: 100 });
    expect(screen.getByText("2/5")).toBeInTheDocument();

    setScrollMetrics(contentArea as HTMLElement, {
      clientHeight: 600,
      scrollHeight: 1400,
      scrollTop: 0,
    });

    fireEvent.wheel(contentArea as HTMLElement, { deltaY: -100 });
    expect(screen.getByText("1/5")).toBeInTheDocument();
  });

  it("restores synced progress and reports completion without legacy storage", async () => {
    const getItem = vi.fn(() => "2");
    const onProgressChange = vi.fn();
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem,
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });

    render(
      <PdfReader
        bookPath="http://127.0.0.1/test.pdf"
        initialProgress={{ reader_kind: "pdf", locator: { kind: "page", page: 4, total_pages: 5 }, progress_ratio: 0.8, status: "reading" }}
        onProgressChange={onProgressChange}
      />,
    );

    await waitFor(() => {
      expect(screen.getByText("4/5")).toBeInTheDocument();
    });
    expect(getItem).not.toHaveBeenCalled();

    fireEvent.change(screen.getByRole("slider"), { target: { value: "5" } });
    expect(onProgressChange).toHaveBeenLastCalledWith(expect.objectContaining({
      locator: { version: 1, kind: "page", page: 5, total_pages: 5 },
      progress_ratio: 1,
      status: "completed",
    }));
  });

  it("does not use the unscoped localStorage fallback", async () => {
    const getItem = vi.fn(() => "4");
    const setItem = vi.fn();
    Object.defineProperty(window, "localStorage", {
      value: { getItem, setItem, removeItem: vi.fn() },
      configurable: true,
    });

    render(<PdfReader bookPath="http://127.0.0.1/test.pdf" title="Test PDF" />);
    await waitFor(() => expect(screen.getByText("1/5")).toBeInTheDocument());
    fireEvent.change(screen.getByRole("slider"), { target: { value: "2" } });

    expect(getItem).not.toHaveBeenCalled();
    expect(setItem).not.toHaveBeenCalled();
  });

  it("uses the controlled annotation page when the PDF text layer is unavailable", async () => {
    const onAnnotationResolved = vi.fn();
    render(
      <PdfReader
        bookPath="http://127.0.0.1/test.pdf"
        annotation={{
          material_id: "pdf-1",
          reader_kind: "pdf",
          locator: { reader_kind: "pdf", kind: "text_range", page: 4, start_offset: 0, end_offset: 5, quote: { exact: "Paper" } },
        }}
        onAnnotationResolved={onAnnotationResolved}
      />,
    );

    await waitFor(() => expect(screen.getByText("4/5")).toBeInTheDocument());
    expect(screen.getByTestId("pdf-annotation-status")).toHaveTextContent("页面位置");
    expect(onAnnotationResolved).toHaveBeenCalledWith(expect.objectContaining({ status: "degraded" }));
  });
});
