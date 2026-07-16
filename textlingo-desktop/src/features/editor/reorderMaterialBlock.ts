import type { Editor } from "@tiptap/core";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { TextSelection } from "@tiptap/pm/state";

import {
  blocksToTiptapDocument,
  normalizeMaterialBlocks,
  tiptapDocumentToBlocks,
} from "./documentModel";
import type { MaterialEditorBlock } from "./types";

export type MaterialBlockPlacement = "before" | "after";

function materialBlockPosition(document: ProseMirrorNode, blockId: string): { node: ProseMirrorNode; pos: number } | null {
  let result: { node: ProseMirrorNode; pos: number } | null = null;
  document.descendants((node, pos) => {
    if (node.attrs.blockId !== blockId) return result === null;
    result = { node, pos };
    return false;
  });
  return result;
}

function textSelectionInsideBlock(
  document: ProseMirrorNode,
  block: { node: ProseMirrorNode; pos: number },
): TextSelection | null {
  if (block.node.isTextblock) {
    return TextSelection.create(document, block.pos + 1);
  }

  let selectionPosition: number | null = null;
  block.node.descendants((node, relativePos) => {
    if (!node.isTextblock) return selectionPosition === null;
    selectionPosition = block.pos + relativePos + 2;
    return false;
  });
  return selectionPosition === null ? null : TextSelection.create(document, selectionPosition);
}

export function reorderMaterialBlocks(
  blocks: MaterialEditorBlock[],
  sourceId: string,
  targetId: string,
  placement: MaterialBlockPlacement,
): MaterialEditorBlock[] {
  const ordered = normalizeMaterialBlocks(blocks);
  const sourceIndex = ordered.findIndex((block) => block.id === sourceId);
  const targetIndex = ordered.findIndex((block) => block.id === targetId);

  if (sourceId === targetId || sourceIndex < 0 || targetIndex < 0) return ordered;

  const next = [...ordered];
  const [source] = next.splice(sourceIndex, 1);
  const remainingTargetIndex = next.findIndex((block) => block.id === targetId);
  if (!source || remainingTargetIndex < 0) return ordered;

  const insertionIndex = remainingTargetIndex + (placement === "after" ? 1 : 0);
  next.splice(insertionIndex, 0, source);

  return next.map((block, blockOrder) => ({ ...block, block_order: blockOrder }));
}

export function reorderEditorBlock(
  editor: Editor,
  sourceId: string,
  targetId: string,
  placement: MaterialBlockPlacement,
): boolean {
  if (!editor.isEditable || sourceId === targetId) return false;

  const currentBlocks = tiptapDocumentToBlocks(editor.getJSON());
  const sourceIndex = currentBlocks.findIndex((block) => block.id === sourceId);
  const targetIndex = currentBlocks.findIndex((block) => block.id === targetId);
  if (sourceIndex < 0 || targetIndex < 0) return false;

  const reordered = reorderMaterialBlocks(currentBlocks, sourceId, targetId, placement);
  if (reordered.every((block, index) => block.id === currentBlocks[index]?.id)) return false;

  const nextDocument = editor.schema.nodeFromJSON(blocksToTiptapDocument(reordered));
  const transaction = editor.state.tr
    .replaceWith(0, editor.state.doc.content.size, nextDocument.content)
    .setMeta("uiEvent", "pointer-block-drop")
    .scrollIntoView();

  const movedBlock = materialBlockPosition(transaction.doc, sourceId);
  if (movedBlock) {
    const selection = textSelectionInsideBlock(transaction.doc, movedBlock);
    if (selection) transaction.setSelection(selection);
  }

  editor.view.dispatch(transaction);
  return true;
}
