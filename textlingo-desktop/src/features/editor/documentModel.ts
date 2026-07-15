import type { JSONContent } from "@tiptap/core";

import type { MaterialBlockType, MaterialEditorBlock } from "./types";

export const MATERIAL_BLOCK_NODE_TYPES = [
  "paragraph",
  "heading",
  "blockquote",
  "listItem",
  "horizontalRule",
] as const;

function textNode(text: string): JSONContent[] | undefined {
  return text.length > 0 ? [{ type: "text", text }] : undefined;
}

function savedInlineContent(block: MaterialEditorBlock): JSONContent[] | undefined {
  const content = block.attrs?.tiptap_content;
  return Array.isArray(content) ? content as JSONContent[] : textNode(block.text);
}

function blockToNode(block: MaterialEditorBlock): JSONContent {
  const attrs = { blockId: block.id, materialAttrs: block.attrs ?? {} };
  switch (block.block_type) {
    case "heading":
      return {
        type: "heading",
        attrs: { ...attrs, level: block.attrs?.level === 3 ? 3 : 2 },
        content: savedInlineContent(block),
      };
    case "quote":
      return {
        type: "blockquote",
        attrs,
        content: [{ type: "paragraph", content: savedInlineContent(block) }],
      };
    case "list_item":
      return {
        type: block.attrs?.list_type === "ordered" ? "orderedList" : "bulletList",
        content: [{
          type: "listItem",
          attrs,
          content: [{ type: "paragraph", content: savedInlineContent(block) }],
        }],
      };
    case "divider":
      return { type: "horizontalRule", attrs };
    case "paragraph":
    default:
      return { type: "paragraph", attrs, content: savedInlineContent(block) };
  }
}

export function blocksToTiptapDocument(blocks: MaterialEditorBlock[]): JSONContent {
  const ordered = [...blocks].sort((left, right) => left.block_order - right.block_order);
  return {
    type: "doc",
    content: (ordered.length > 0 ? ordered : [{
      block_type: "paragraph" as const,
      block_order: 0,
      text: "",
      attrs: {},
    }]).map(blockToNode),
  };
}

function nodeText(node: JSONContent): string {
  if (typeof node.text === "string") return node.text;
  if (node.type === "hardBreak") return "\n";
  const separator = node.type === "bulletList" || node.type === "orderedList" ? "\n" : "";
  return (node.content ?? []).map(nodeText).join(separator);
}

function inlineContent(node: JSONContent): JSONContent[] {
  if (node.type === "blockquote") return node.content?.[0]?.content ?? [];
  if (node.type === "listItem") return node.content?.[0]?.content ?? [];
  return node.content ?? [];
}

function attrsWithInlineContent(
  materialAttrs: Record<string, unknown>,
  node: JSONContent,
  extra: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    ...materialAttrs,
    ...extra,
    tiptap_content: inlineContent(node),
  };
}

function nodeBlockType(node: JSONContent): MaterialBlockType {
  if (node.type === "heading") return "heading";
  if (node.type === "blockquote") return "quote";
  if (node.type === "horizontalRule") return "divider";
  return "paragraph";
}

function createBlockId(): string {
  return typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
    ? crypto.randomUUID()
    : `block-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

export function tiptapDocumentToBlocks(document: JSONContent): MaterialEditorBlock[] {
  const blocks: MaterialEditorBlock[] = [];
  for (const node of document.content ?? []) {
    if (node.type === "bulletList" || node.type === "orderedList") {
      for (const item of node.content ?? []) {
        const materialAttrs = typeof item.attrs?.materialAttrs === "object" && item.attrs.materialAttrs !== null
          ? item.attrs.materialAttrs as Record<string, unknown>
          : {};
        blocks.push({
          id: typeof item.attrs?.blockId === "string" ? item.attrs.blockId : createBlockId(),
          block_type: "list_item",
          block_order: blocks.length,
          text: nodeText(item),
          attrs: attrsWithInlineContent(materialAttrs, item, {
            list_type: node.type === "orderedList" ? "ordered" : "bullet",
          }),
        });
      }
      continue;
    }
    const materialAttrs = typeof node.attrs?.materialAttrs === "object" && node.attrs.materialAttrs !== null
      ? node.attrs.materialAttrs as Record<string, unknown>
      : {};
    blocks.push({
      id: typeof node.attrs?.blockId === "string" ? node.attrs.blockId : createBlockId(),
      block_type: nodeBlockType(node),
      block_order: blocks.length,
      text: node.type === "horizontalRule" ? "" : nodeText(node),
      attrs: node.type === "horizontalRule"
        ? materialAttrs
        : attrsWithInlineContent(
          materialAttrs,
          node,
          node.type === "heading" ? { level: node.attrs?.level === 3 ? 3 : 2 } : {},
        ),
    });
  }
  return blocks;
}

export function normalizeMaterialBlocks(blocks: MaterialEditorBlock[]): MaterialEditorBlock[] {
  return [...blocks]
    .sort((left, right) => left.block_order - right.block_order)
    .map((block, index) => {
      const attrs = block.attrs ?? {};
      return {
        id: block.id,
        block_type: block.block_type,
        block_order: index,
        text: block.text,
        attrs: block.block_type === "divider" || Array.isArray(attrs.tiptap_content)
          ? attrs
          : { ...attrs, tiptap_content: textNode(block.text) ?? [] },
        ...(block.segments ? { segments: block.segments } : {}),
      };
    });
}

export function materialBlocksForPersistence(blocks: MaterialEditorBlock[]): MaterialEditorBlock[] {
  return normalizeMaterialBlocks(blocks)
    .filter((block) => block.block_type === "divider" || block.text.trim().length > 0)
    .map((block, index) => ({ ...block, block_order: index }));
}

export function materialBlocksToMarkdown(blocks: MaterialEditorBlock[]): string {
  return normalizeMaterialBlocks(blocks).map((block) => {
    if (block.block_type === "heading") return `${block.attrs.level === 3 ? "###" : "##"} ${block.text}`;
    if (block.block_type === "list_item") return `${block.attrs.list_type === "ordered" ? "1." : "-"} ${block.text}`;
    if (block.block_type === "quote") return `> ${block.text}`;
    if (block.block_type === "divider") return "---";
    return block.text;
  }).join("\n\n");
}

export function markdownToMaterialBlocks(source: string, existing: MaterialEditorBlock[] = []): MaterialEditorBlock[] {
  const chunks = source.replace(/\r\n/g, "\n").split(/\n{2,}/).map((chunk) => chunk.trim()).filter(Boolean);
  const parsed = chunks.map((chunk, index) => {
    let block_type: MaterialBlockType = "paragraph";
    let text = chunk;
    let attrs: Record<string, unknown> = {};
    if (/^###\s+/.test(chunk)) {
      block_type = "heading";
      text = chunk.replace(/^###\s+/, "");
      attrs = { level: 3 };
    } else if (/^##\s+/.test(chunk)) {
      block_type = "heading";
      text = chunk.replace(/^##\s+/, "");
      attrs = { level: 2 };
    } else if (/^>\s+/.test(chunk)) {
      block_type = "quote";
      text = chunk.replace(/^>\s+/, "");
    } else if (/^[-*]\s+/.test(chunk)) {
      block_type = "list_item";
      text = chunk.replace(/^[-*]\s+/, "");
      attrs = { list_type: "bullet" };
    } else if (/^\d+\.\s+/.test(chunk)) {
      block_type = "list_item";
      text = chunk.replace(/^\d+\.\s+/, "");
      attrs = { list_type: "ordered" };
    } else if (chunk === "---") {
      block_type = "divider";
      text = "";
    }
    return { block_type, block_order: index, text, attrs };
  });

  const usedExisting = new Set<number>();
  const matches: Array<number | undefined> = new Array(parsed.length);

  // Match unchanged blocks before positional edits so inserting or deleting a
  // source block cannot strip rich inline content from later unchanged blocks.
  parsed.forEach((block, index) => {
    const candidates = existing
      .map((previous, previousIndex) => ({ previous, previousIndex }))
      .filter(({ previous, previousIndex }) => (
        !usedExisting.has(previousIndex)
        && previous.block_type === block.block_type
        && previous.text === block.text
      ));
    const match = candidates.find(({ previousIndex }) => previousIndex === index) ?? candidates[0];
    if (match) {
      matches[index] = match.previousIndex;
      usedExisting.add(match.previousIndex);
    }
  });

  // A changed block may keep the identity of the block at the same position,
  // provided that old block was not already claimed by an unchanged match.
  parsed.forEach((block, index) => {
    if (matches[index] !== undefined) return;
    const previous = existing[index];
    if (previous && !usedExisting.has(index) && previous.block_type === block.block_type) {
      matches[index] = index;
      usedExisting.add(index);
    }
  });

  return parsed.map((block, index) => {
    const previousIndex = matches[index];
    const previous = previousIndex === undefined ? undefined : existing[previousIndex];
    const previousAttrs = previous ? { ...previous.attrs } : {};
    if (previous?.text !== block.text) delete previousAttrs.tiptap_content;
    return {
      ...(previous?.id ? { id: previous.id } : {}),
      block_type: block.block_type,
      block_order: index,
      text: block.text,
      attrs: { ...previousAttrs, ...block.attrs },
    };
  });
}

export function materialBlocksFingerprint(blocks: MaterialEditorBlock[]): string {
  return JSON.stringify(normalizeMaterialBlocks(blocks).map(({ id, block_type, block_order, text, attrs }) => ({
    id: id ?? null,
    block_type,
    block_order,
    text,
    attrs,
  })));
}

export function materialBlocksToPlainText(blocks: MaterialEditorBlock[]): string {
  return normalizeMaterialBlocks(blocks)
    .filter((block) => block.block_type !== "divider")
    .map((block) => block.text)
    .join("\n\n");
}
