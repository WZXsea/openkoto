import { describe, expect, it } from "vitest";

import {
  blocksToTiptapDocument,
  markdownToMaterialBlocks,
  materialBlocksFingerprint,
  materialBlocksToMarkdown,
  materialBlocksToPlainText,
  tiptapDocumentToBlocks,
} from "./documentModel";
import type { MaterialEditorBlock } from "./types";

const blocks: MaterialEditorBlock[] = [
  { id: "heading", block_type: "heading", block_order: 0, text: "Hypoxia", attrs: { source: "import", level: 2 } },
  { id: "paragraph", block_type: "paragraph", block_order: 1, text: "HIF signalling changes transcription.", attrs: {} },
  { id: "list-1", block_type: "list_item", block_order: 2, text: "VHL", attrs: { list_type: "bullet" } },
  { id: "list-2", block_type: "list_item", block_order: 3, text: "HIF2A", attrs: { list_type: "bullet" } },
];

describe("material editor document model", () => {
  it("round-trips constrained blocks through Tiptap JSON without losing stable ids", () => {
    const roundTrip = tiptapDocumentToBlocks(blocksToTiptapDocument(blocks));
    expect(roundTrip.map((block) => ({ ...block, attrs: { ...block.attrs, tiptap_content: undefined } })))
      .toEqual(blocks.map((block) => ({ ...block, attrs: { ...block.attrs, tiptap_content: undefined } })));
  });

  it("round-trips bold, italic, inline code, and links through attrs", () => {
    const richBlocks: MaterialEditorBlock[] = [{
      id: "rich",
      block_type: "paragraph",
      block_order: 0,
      text: "Bold italic code link",
      attrs: {
        tiptap_content: [
          { type: "text", text: "Bold", marks: [{ type: "bold" }] },
          { type: "text", text: " italic", marks: [{ type: "italic" }] },
          { type: "text", text: " code", marks: [{ type: "code" }] },
          { type: "text", text: " link", marks: [{ type: "link", attrs: { href: "https://example.com" } }] },
        ],
      },
    }];
    expect(tiptapDocumentToBlocks(blocksToTiptapDocument(richBlocks))).toEqual(richBlocks);
  });

  it("normalizes order for dirty comparison while retaining structural changes", () => {
    expect(materialBlocksFingerprint([...blocks].reverse())).toBe(materialBlocksFingerprint(blocks));
    expect(materialBlocksFingerprint([{ ...blocks[0], text: "Changed" }, ...blocks.slice(1)]))
      .not.toBe(materialBlocksFingerprint(blocks));
  });

  it("produces a readable plain-text fallback", () => {
    expect(materialBlocksToPlainText(blocks)).toBe("Hypoxia\n\nHIF signalling changes transcription.\n\nVHL\n\nHIF2A");
  });

  it("round-trips the controlled Markdown source into block structure", () => {
    const markdown = materialBlocksToMarkdown(blocks);
    expect(markdownToMaterialBlocks(markdown, blocks)).toEqual(blocks);
  });

  it("preserves inline marks on unchanged blocks when another source block changes", () => {
    const rich = [{
      ...blocks[0],
      attrs: {
        ...blocks[0].attrs,
        tiptap_content: [{ type: "text", text: "Hypoxia", marks: [{ type: "bold" }] }],
      },
    }, blocks[1]];

    const rebuilt = markdownToMaterialBlocks("## Hypoxia\n\nChanged paragraph", rich);

    expect(rebuilt[0].attrs.tiptap_content).toEqual(rich[0].attrs.tiptap_content);
    expect(rebuilt[1].attrs.tiptap_content).toBeUndefined();
  });

  it("preserves stable ids and inline marks across source block insertion and reordering", () => {
    const rich = [{
      id: "rich",
      block_type: "paragraph" as const,
      block_order: 0,
      text: "Hypoxia",
      attrs: {
        tiptap_content: [{ type: "text", text: "Hypoxia", marks: [{ type: "bold" }] }],
      },
    }, {
      id: "plain",
      block_type: "paragraph" as const,
      block_order: 1,
      text: "VHL",
      attrs: {},
    }];

    const rebuilt = markdownToMaterialBlocks("New block\n\nVHL\n\nHypoxia", rich);

    expect(rebuilt[0].id).toBeUndefined();
    expect(rebuilt[1].id).toBe("plain");
    expect(rebuilt[2].id).toBe("rich");
    expect(rebuilt[2].attrs.tiptap_content).toEqual(rich[0].attrs.tiptap_content);
  });
});
