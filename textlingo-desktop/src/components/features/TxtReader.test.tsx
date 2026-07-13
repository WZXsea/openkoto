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
      locator: { kind: "page", page: 3, total_pages: 3 },
      progress_ratio: 1,
      status: "completed",
    }));
  });
});
