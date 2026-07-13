import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { EpubReader } from "./EpubReader";

vi.mock("react-reader", () => ({
  ReactReader: ({ location, locationChanged }: { location: string | number; locationChanged: (cfi: string) => void }) => (
    <button data-testid="epub-reader" onClick={() => locationChanged("epubcfi(/6/4!/4/1:0)")}>
      {String(location)}
    </button>
  ),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("./BookmarkSidebar", () => ({
  BookmarkSidebar: () => null,
}));

describe("EpubReader", () => {
  beforeEach(() => {
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn(() => "epubcfi(/6/2!/4/1:0)"),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      configurable: true,
    });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("restores synced CFI and reports CFI updates without legacy storage", () => {
    vi.useFakeTimers();
    const onProgressChange = vi.fn();
    render(
      <EpubReader
        bookPath="/tmp/book.epub"
        initialProgress={{
          reader_kind: "epub",
          locator: { kind: "epub_cfi", cfi: "epubcfi(/6/8!/4/1:0)" },
          progress_ratio: 0.5,
          status: "reading",
        }}
        onProgressChange={onProgressChange}
      />,
    );

    expect(screen.getByTestId("epub-reader")).toHaveTextContent("epubcfi(/6/8!/4/1:0)");
    expect(window.localStorage.getItem).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("epub-reader"));
    act(() => {
      vi.advanceTimersByTime(750);
    });
    expect(onProgressChange).toHaveBeenLastCalledWith(expect.objectContaining({
      reader_kind: "epub",
      locator: { version: 1, kind: "epub_cfi", cfi: "epubcfi(/6/4!/4/1:0)" },
      progress_ratio: 0.5,
      status: "reading",
    }));
    expect(window.localStorage.setItem).not.toHaveBeenCalled();
  });

  it("does not use the unscoped localStorage fallback", () => {
    const getItem = vi.fn(() => "epubcfi(/6/2!/4/1:0)");
    const setItem = vi.fn();
    Object.defineProperty(window, "localStorage", {
      value: { getItem, setItem, removeItem: vi.fn() },
      configurable: true,
    });

    render(<EpubReader bookPath="/tmp/book.epub" />);
    fireEvent.click(screen.getByTestId("epub-reader"));

    expect(getItem).not.toHaveBeenCalled();
    expect(setItem).not.toHaveBeenCalled();
  });
});
