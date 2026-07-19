import { Editor, Extension } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { NodeSelection } from "@tiptap/pm/state";
import { afterEach, describe, expect, it } from "vitest";

import { blocksToTiptapDocument, tiptapDocumentToBlocks } from "./documentModel";
import { reorderEditorBlock, reorderMaterialBlocks } from "./reorderMaterialBlock";
import type { MaterialEditorBlock } from "./types";

const TestMaterialBlockMetadata = Extension.create({
  name: "testMaterialBlockMetadata",
  addGlobalAttributes() {
    return [{
      types: ["paragraph", "heading", "blockquote", "listItem", "horizontalRule"],
      attributes: {
        blockId: { default: null, rendered: false },
        materialAttrs: { default: {}, rendered: false },
      },
    }];
  },
});

const richInline = [
  { type: "text", text: "Alpha", marks: [{ type: "bold" }] },
  { type: "text", text: " beta", marks: [{ type: "italic" }] },
];

const blocks: MaterialEditorBlock[] = [
  {
    id: "block-a",
    block_type: "paragraph",
    block_order: 0,
    text: "Alpha beta",
    attrs: { language: "en", tiptap_content: richInline },
  },
  {
    id: "block-b",
    block_type: "heading",
    block_order: 1,
    text: "Heading",
    attrs: { level: 2, tiptap_content: [{ type: "text", text: "Heading" }] },
  },
  {
    id: "block-c",
    block_type: "list_item",
    block_order: 2,
    text: "List item",
    attrs: {
      list_type: "bullet",
      source: "imported",
      tiptap_content: [{ type: "text", text: "List item", marks: [{ type: "code" }] }],
    },
  },
  {
    id: "block-d",
    block_type: "quote",
    block_order: 3,
    text: "Quoted",
    attrs: { tiptap_content: [{ type: "text", text: "Quoted" }] },
  },
];

function ids(items: MaterialEditorBlock[]) {
  return items.map((block) => block.id);
}

function createEditor() {
  return new Editor({
    extensions: [
      StarterKit.configure({ codeBlock: false, trailingNode: false }),
      TestMaterialBlockMetadata,
    ],
    content: blocksToTiptapDocument(blocks),
  });
}

const editors: Editor[] = [];

afterEach(() => {
  editors.splice(0).forEach((editor) => editor.destroy());
});

describe("reorderMaterialBlocks", () => {
  it("moves blocks upward, downward, and to both document edges", () => {
    expect(ids(reorderMaterialBlocks(blocks, "block-c", "block-a", "before")))
      .toEqual(["block-c", "block-a", "block-b", "block-d"]);
    expect(ids(reorderMaterialBlocks(blocks, "block-a", "block-c", "after")))
      .toEqual(["block-b", "block-c", "block-a", "block-d"]);
    expect(ids(reorderMaterialBlocks(blocks, "block-d", "block-a", "before")))
      .toEqual(["block-d", "block-a", "block-b", "block-c"]);
    expect(ids(reorderMaterialBlocks(blocks, "block-a", "block-d", "after")))
      .toEqual(["block-b", "block-c", "block-d", "block-a"]);
  });

  it("keeps the same order for self drops, adjacent slots, and unknown ids", () => {
    expect(ids(reorderMaterialBlocks(blocks, "block-b", "block-b", "before"))).toEqual(ids(blocks));
    expect(ids(reorderMaterialBlocks(blocks, "block-a", "block-b", "before"))).toEqual(ids(blocks));
    expect(ids(reorderMaterialBlocks(blocks, "block-b", "block-a", "after"))).toEqual(ids(blocks));
    expect(ids(reorderMaterialBlocks(blocks, "missing", "block-a", "before"))).toEqual(ids(blocks));
    expect(ids(reorderMaterialBlocks(blocks, "block-a", "missing", "after"))).toEqual(ids(blocks));
  });

  it("preserves ids, attrs, inline marks, and list item structure while only renumbering order", () => {
    const result = reorderMaterialBlocks(blocks, "block-c", "block-a", "before");
    const byId = new Map(result.map((block) => [block.id, block]));

    for (const original of blocks) {
      const moved = byId.get(original.id);
      expect(moved?.id).toBe(original.id);
      expect(moved?.block_type).toBe(original.block_type);
      expect(moved?.text).toBe(original.text);
      expect(moved?.attrs).toEqual(original.attrs);
    }
    expect(byId.get("block-a")?.attrs.tiptap_content).toEqual(richInline);
    expect(byId.get("block-c")).toMatchObject({
      block_type: "list_item",
      attrs: { list_type: "bullet", source: "imported" },
    });
    expect(result.map((block) => block.block_order)).toEqual([0, 1, 2, 3]);
  });
});

describe("reorderEditorBlock", () => {
  it("dispatches one document transaction and supports undo and redo", () => {
    const editor = createEditor();
    editors.push(editor);
    let documentTransactions = 0;
    editor.on("transaction", ({ transaction }) => {
      if (transaction.docChanged) documentTransactions += 1;
    });

    expect(reorderEditorBlock(editor, "block-a", "block-c", "after")).toBe(true);
    expect(documentTransactions).toBe(1);
    expect(ids(tiptapDocumentToBlocks(editor.getJSON())))
      .toEqual(["block-b", "block-c", "block-a", "block-d"]);

    expect(editor.commands.undo()).toBe(true);
    expect(ids(tiptapDocumentToBlocks(editor.getJSON()))).toEqual(ids(blocks));
    expect(editor.commands.redo()).toBe(true);
    expect(ids(tiptapDocumentToBlocks(editor.getJSON())))
      .toEqual(["block-b", "block-c", "block-a", "block-d"]);
  });

  it("places a text selection after the moved block and never leaves a selected-node decoration", () => {
    const editor = createEditor();
    editors.push(editor);

    expect(reorderEditorBlock(editor, "block-a", "block-c", "after")).toBe(true);

    expect(editor.state.selection).not.toBeInstanceOf(NodeSelection);
    expect(editor.view.dom.querySelector(".ProseMirror-selectednode")).toBeNull();
  });

  it("does not dispatch a transaction for a no-op or unknown block", () => {
    const editor = createEditor();
    editors.push(editor);
    let documentTransactions = 0;
    editor.on("transaction", ({ transaction }) => {
      if (transaction.docChanged) documentTransactions += 1;
    });

    expect(reorderEditorBlock(editor, "block-a", "block-b", "before")).toBe(false);
    expect(reorderEditorBlock(editor, "missing", "block-a", "before")).toBe(false);
    expect(documentTransactions).toBe(0);
  });
});
