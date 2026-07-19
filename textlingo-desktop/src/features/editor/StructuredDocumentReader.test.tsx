import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { StructuredDocumentReader } from "./StructuredDocumentReader";
import type { MaterialEditorBlock } from "./types";

const blocks: MaterialEditorBlock[] = [
  { id: "h2", block_type: "heading", block_order: 0, text: "Findings", attrs: { level: 2 } },
  { id: "p", block_type: "paragraph", block_order: 1, text: "Bold link", attrs: { tiptap_content: [
    { type: "text", text: "Bold", marks: [{ type: "bold" }] },
    { type: "text", text: " link", marks: [{ type: "link", attrs: { href: "https://example.com" } }] },
  ] }, segments: [{ id: "seg-1", order: 0, block_segment_order: 0, text: "Bold link" }] },
  { id: "li-1", block_type: "list_item", block_order: 2, text: "VHL", attrs: { list_type: "bullet" } },
  { id: "li-2", block_type: "list_item", block_order: 3, text: "HIF2A", attrs: { list_type: "bullet" } },
  { id: "quote", block_type: "quote", block_order: 4, text: "Evidence", attrs: {} },
  { id: "divider", block_type: "divider", block_order: 5, text: "", attrs: {} },
];

describe("StructuredDocumentReader", () => {
  it("renders structural blocks, inline marks and clickable segment identity", async () => {
    const onSegmentClick = vi.fn();
    render(<StructuredDocumentReader blocks={blocks} fontSize={18} viewMode="original" selectedSegmentId={null} onSegmentClick={onSegmentClick} />);

    expect(screen.getByRole("heading", { level: 2, name: "Findings" })).toBeInTheDocument();
    expect(screen.getByRole("list")).toHaveTextContent("VHLHIF2A");
    expect(screen.getByText("Evidence").closest("blockquote")).toBeInTheDocument();
    expect(screen.getByRole("separator")).toBeInTheDocument();
    expect(screen.getByText("Bold").closest("strong")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "link" })).toHaveAttribute("href", "https://example.com");

    await userEvent.click(screen.getByText("Bold").closest("[data-reader-segment-id]")!);
    expect(onSegmentClick).toHaveBeenCalledWith("seg-1");
  });

  it("uses live derived segment fields and renders soft line breaks", () => {
    const blocksWithBreak: MaterialEditorBlock[] = [{
      id: "live",
      block_type: "paragraph",
      block_order: 0,
      text: "First\nSecond",
      attrs: { tiptap_content: [
        { type: "text", text: "First" },
        { type: "hardBreak" },
        { type: "text", text: "Second" },
      ] },
      segments: [{ id: "seg-live", order: 0, block_segment_order: 0, text: "First\nSecond" }],
    }];
    const { container } = render(
      <StructuredDocumentReader
        blocks={blocksWithBreak}
        liveSegments={[{ id: "seg-live", order: 0, block_segment_order: 0, text: "First\nSecond", translation: "第一行\n第二行", reading_text: "first second" }]}
        fontSize={18}
        viewMode="bilingual"
        selectedSegmentId={null}
        onSegmentClick={() => undefined}
      />,
    );

    expect(container.querySelector("br")).toBeInTheDocument();
    expect(container.querySelector(".text-primary")).toHaveTextContent("第一行 第二行");
    expect(container).toHaveTextContent("first second");
  });
});
