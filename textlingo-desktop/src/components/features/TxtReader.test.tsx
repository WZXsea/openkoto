import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TxtReader } from "./TxtReader";

describe("TxtReader", () => {
  afterEach(() => {
    cleanup();
  });

  it("restores the supplied page and reports the final page immediately", () => {
    const onProgressChange = vi.fn();
    const content = ["a".repeat(2000), "b".repeat(2000), "c".repeat(2000)].join("\n\n");
    render(
      <TxtReader
        content={content}
        initialProgress={{ reader_kind: "txt", locator: { kind: "page", page: 2, total_pages: 3 }, progress_ratio: 2 / 3, status: "reading" }}
        onProgressChange={onProgressChange}
      />,
    );

    expect(screen.getAllByText("2 / 3").length).toBeGreaterThan(0);
    fireEvent.click(screen.getByTitle("下一页"));
    expect(onProgressChange).toHaveBeenLastCalledWith(expect.objectContaining({
      reader_kind: "txt",
      locator: { version: 1, kind: "page", page: 3, total_pages: 3 },
      progress_ratio: 1,
      status: "completed",
    }));
  });

  it("accepts a controlled annotation target and highlights the resolved range", () => {
    const onAnnotationResolved = vi.fn();
    render(
      <TxtReader
        content="target"
        annotation={{
          material_id: "txt-1",
          reader_kind: "txt",
          locator: { reader_kind: "txt", kind: "text_range", page: 1, start_offset: 0, end_offset: 6, quote: { exact: "target" } },
        }}
        onAnnotationResolved={onAnnotationResolved}
      />,
    );

    expect(screen.getByTestId("txt-annotation-highlight")).toHaveTextContent("target");
    expect(screen.getByTestId("txt-annotation-status")).toHaveTextContent("精确定位");
    expect(onAnnotationResolved).toHaveBeenCalledWith(expect.objectContaining({ status: "exact" }));
  });
});
