import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { MaterialDocumentEditor } from "./MaterialDocumentEditor";
import type { MaterialDocument, MaterialEditImpact } from "./types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const baseDocument: MaterialDocument = {
  material_id: "material-1",
  title: "HIF review",
  current_revision: 1,
  content_sha256: "a".repeat(64),
  blocks: [{ id: "block-1", block_type: "paragraph", block_order: 0, text: "HIF signalling.", attrs: {} }],
  derived_summary: {},
};

const reorderDocument: MaterialDocument = {
  ...baseDocument,
  blocks: [
    { id: "block-a", block_type: "paragraph", block_order: 0, text: "Alpha", attrs: {} },
    { id: "block-b", block_type: "paragraph", block_order: 1, text: "Beta", attrs: {} },
    { id: "block-c", block_type: "paragraph", block_order: 2, text: "Gamma", attrs: {} },
  ],
};

const noImpact: MaterialEditImpact = {
  inserted_blocks: 1,
  updated_blocks: 0,
  deleted_blocks: 0,
  moved_blocks: 0,
  changed_segments: 1,
  deleted_segments: 0,
  stale_readings: 0,
  stale_translations: 0,
  stale_explanations: 0,
  affected_annotations: 0,
  affected_learning_items: 0,
};

function mockCommands(impact: MaterialEditImpact = noImpact) {
  invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
    if (command === "get_material_document_cmd") return Promise.resolve(baseDocument);
    if (command === "get_material_draft_cmd") return Promise.resolve(null);
    if (command === "save_material_draft_cmd") {
      const request = args?.payload as { base_revision: number; blocks: MaterialDocument["blocks"] };
      return Promise.resolve({ material_id: "material-1", ...request, updated_at: "2026-07-15T12:00:00Z", is_stale: false });
    }
    if (command === "delete_material_draft_cmd") return Promise.resolve(null);
    if (command === "preview_material_edit_cmd") {
      return Promise.resolve({ base_revision: 1, next_revision: 2, content_sha256: "b".repeat(64), preview_token: "preview-1", impact });
    }
    if (command === "commit_material_edit_cmd") {
      const request = args?.payload as { blocks: MaterialDocument["blocks"] };
      return Promise.resolve({
        document: { ...baseDocument, current_revision: 2, blocks: request.blocks },
        impact,
      });
    }
    if (command === "list_material_revisions_cmd") return Promise.resolve([]);
    return Promise.reject(new Error(`Unexpected command: ${command}`));
  });
}

function mockReorderCommands() {
  const moveImpact: MaterialEditImpact = {
    ...noImpact,
    inserted_blocks: 0,
    changed_segments: 0,
    moved_blocks: 1,
  };
  invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
    if (command === "get_material_document_cmd") return Promise.resolve(reorderDocument);
    if (command === "get_material_draft_cmd") return Promise.resolve(null);
    if (command === "save_material_draft_cmd") {
      const request = args?.payload as { base_revision: number; blocks: MaterialDocument["blocks"] };
      return Promise.resolve({ material_id: "material-1", ...request, updated_at: "2026-07-15T12:00:00Z", is_stale: false });
    }
    if (command === "delete_material_draft_cmd") return Promise.resolve(null);
    if (command === "preview_material_edit_cmd") {
      return Promise.resolve({ base_revision: 1, next_revision: 2, content_sha256: "b".repeat(64), preview_token: "preview-move", impact: moveImpact });
    }
    if (command === "commit_material_edit_cmd") {
      const request = args?.payload as { blocks: MaterialDocument["blocks"] };
      return Promise.resolve({ document: { ...reorderDocument, current_revision: 2, blocks: request.blocks }, impact: moveImpact });
    }
    if (command === "list_material_revisions_cmd") return Promise.resolve([]);
    return Promise.reject(new Error(`Unexpected command: ${command}`));
  });
}

function renderedBlockIds(): Array<string | undefined> {
  const editor = screen.getByTestId("material-block-editor");
  return Array.from(editor.querySelectorAll<HTMLElement>(".tiptap > [data-block-id]"))
    .map((element) => element.dataset.blockId);
}

beforeEach(() => {
  invokeMock.mockReset();
  mockCommands();
});

afterEach(cleanup);

describe("MaterialDocumentEditor", () => {
  it("previews and commits a structural edit without a confirmation when derived data is unaffected", async () => {
    const onCommitted = vi.fn();
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} onCommitted={onCommitted} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: /添加段落/ }));
    await user.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(onCommitted).toHaveBeenCalledTimes(1));
    expect(invokeMock).toHaveBeenCalledWith("preview_material_edit_cmd", expect.objectContaining({ materialId: "material-1" }));
    expect(invokeMock).toHaveBeenCalledWith("commit_material_edit_cmd", expect.objectContaining({
      materialId: "material-1",
        payload: expect.objectContaining({ base_revision: 1, client_request_id: expect.any(String), preview_token: "preview-1" }),
    }));
  });

  it("moves the selected block with toolbar controls in one undoable editor action", async () => {
    invokeMock.mockReset();
    mockReorderCommands();
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    expect(renderedBlockIds()).toEqual(["block-a", "block-b", "block-c"]);

    await user.click(screen.getByRole("button", { name: "下移当前块" }));
    expect(renderedBlockIds()).toEqual(["block-b", "block-a", "block-c"]);
    await user.click(screen.getByRole("button", { name: "撤销" }));
    expect(renderedBlockIds()).toEqual(["block-a", "block-b", "block-c"]);
    await user.click(screen.getByRole("button", { name: "重做" }));
    expect(renderedBlockIds()).toEqual(["block-b", "block-a", "block-c"]);
  });

  it("moves the selected block with Alt+ArrowDown and submits only the reordered ids", async () => {
    invokeMock.mockReset();
    mockReorderCommands();
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    const editor = await screen.findByTestId("material-block-editor");
    const tiptap = editor.querySelector<HTMLElement>(".tiptap");
    expect(tiptap).not.toBeNull();
    fireEvent.keyDown(tiptap!, { key: "ArrowDown", altKey: true });
    expect(renderedBlockIds()).toEqual(["block-b", "block-a", "block-c"]);

    await user.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith(
      "preview_material_edit_cmd",
      expect.objectContaining({
        payload: expect.objectContaining({
          blocks: [
            expect.objectContaining({ id: "block-b", block_order: 0 }),
            expect.objectContaining({ id: "block-a", block_order: 1 }),
            expect.objectContaining({ id: "block-c", block_order: 2 }),
          ],
        }),
      }),
    ));
  });

  it("requires explicit confirmation when translations or annotations are affected", async () => {
    invokeMock.mockReset();
    mockCommands({ ...noImpact, stale_translations: 1, affected_annotations: 1 });
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: /添加段落/ }));
    await user.click(screen.getByRole("button", { name: "保存" }));

    expect(await screen.findByRole("dialog", { name: "确认正文变更" })).toBeVisible();
    expect(screen.getByTestId("material-edit-impact-grid")).toHaveTextContent("翻译待更新1");
    expect(invokeMock).not.toHaveBeenCalledWith("commit_material_edit_cmd", expect.anything());
    await user.click(screen.getByRole("button", { name: "确认保存" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("commit_material_edit_cmd", expect.anything()));
  });

  it("discards the editor draft before returning and never commits cancelled content", async () => {
    const onCancel = vi.fn();
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={onCancel} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: /添加段落/ }));
    await user.click(screen.getByRole("button", { name: "返回阅读" }));
    expect(await screen.findByRole("dialog", { name: "处理未保存修改" })).toBeVisible();
    expect(screen.getByRole("button", { name: "保存新版本并返回" })).toBeVisible();
    expect(screen.getByRole("button", { name: "保留草稿并返回" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "丢弃并返回" }));

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("delete_material_draft_cmd", { materialId: "material-1" });
    expect(invokeMock).not.toHaveBeenCalledWith("commit_material_edit_cmd", expect.anything());
  });

  it("can keep a draft and return without committing", async () => {
    const onCancel = vi.fn();
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={onCancel} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: /添加段落/ }));
    await user.click(screen.getByRole("button", { name: "返回阅读" }));
    await user.click(await screen.findByRole("button", { name: "保留草稿并返回" }));

    await waitFor(() => expect(onCancel).toHaveBeenCalledTimes(1));
    expect(invokeMock).toHaveBeenCalledWith("save_material_draft_cmd", expect.objectContaining({ materialId: "material-1" }));
    expect(invokeMock).not.toHaveBeenCalledWith("commit_material_edit_cmd", expect.anything());
  });

  it("rebuilds controlled Markdown source into backend block types", async () => {
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: "编辑纯文本或 Markdown" }));
    const source = await screen.findByRole("textbox", { name: "Markdown 源文本" });
    await user.clear(source);
    await user.type(source, "## Findings\n\n- VHL");
    await user.click(screen.getByRole("button", { name: "应用到块结构" }));
    await user.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("preview_material_edit_cmd", expect.objectContaining({
      payload: expect.objectContaining({
        blocks: [
          expect.objectContaining({ block_type: "heading", text: "Findings", attrs: expect.objectContaining({ level: 2 }) }),
          expect.objectContaining({ block_type: "list_item", text: "VHL", attrs: expect.objectContaining({ list_type: "bullet" }) }),
        ],
      }),
    })));
  });

  it("preserves inline formatting when unchanged source text is applied", async () => {
    const richDocument: MaterialDocument = {
      ...baseDocument,
      blocks: [{
        ...baseDocument.blocks[0],
        attrs: { tiptap_content: [{ type: "text", text: "HIF", marks: [{ type: "bold" }] }, { type: "text", text: " signalling." }] },
      }],
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_material_document_cmd") return Promise.resolve(richDocument);
      if (command === "get_material_draft_cmd") return Promise.resolve(null);
      return Promise.resolve(null);
    });
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    const statusBefore = screen.getByRole("status").textContent;
    await user.click(screen.getByRole("button", { name: "编辑纯文本或 Markdown" }));
    await user.click(await screen.findByRole("button", { name: "应用到块结构" }));

    expect(screen.getByRole("status").textContent).toBe(statusBefore);
    expect(invokeMock).not.toHaveBeenCalledWith("preview_material_edit_cmd", expect.anything());
  });

  it("requires save, draft, or discard handling before restoring while dirty", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_material_document_cmd") return Promise.resolve(baseDocument);
      if (command === "get_material_draft_cmd") return Promise.resolve(null);
      if (command === "list_material_revisions_cmd") return Promise.resolve([{ revision: 1, parent_revision: null, action: "edit", change_summary: { updated_blocks: 1 }, created_at: "2026-07-15T10:00:00Z" }]);
      if (command === "get_material_revision_cmd") return Promise.resolve({ revision: 1, parent_revision: null, action: "edit", change_summary: { updated_blocks: 1 }, created_at: "2026-07-15T10:00:00Z", snapshot: baseDocument });
      return Promise.resolve(null);
    });
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: /添加段落/ }));
    await user.click(screen.getByRole("button", { name: /版本历史/ }));
    await user.click(await screen.findByRole("button", { name: /修改 1/ }));
    await user.click(await screen.findByRole("button", { name: "恢复此版本" }));

    expect(await screen.findByRole("dialog", { name: "恢复前处理未保存修改" })).toBeVisible();
    expect(screen.getByRole("button", { name: "保存当前版本后恢复" })).toBeVisible();
    expect(screen.getByRole("button", { name: "保留草稿后恢复" })).toBeVisible();
    expect(screen.getByRole("button", { name: "丢弃修改后恢复" })).toBeVisible();
  });

  it("preserves the current draft atomically when restoring an older revision", async () => {
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "get_material_document_cmd") return Promise.resolve(baseDocument);
      if (command === "get_material_draft_cmd") return Promise.resolve(null);
      if (command === "list_material_revisions_cmd") return Promise.resolve([{ revision: 1, parent_revision: null, action: "edit", change_summary: { updated_blocks: 1 }, created_at: "2026-07-15T10:00:00Z" }]);
      if (command === "get_material_revision_cmd") return Promise.resolve({ revision: 1, parent_revision: null, action: "edit", change_summary: { updated_blocks: 1 }, created_at: "2026-07-15T10:00:00Z", snapshot: baseDocument });
      if (command === "save_material_draft_cmd") {
        const request = args?.payload as { base_revision: number; blocks: MaterialDocument["blocks"] };
        return Promise.resolve({ material_id: "material-1", ...request, updated_at: "2026-07-15T12:00:00Z", is_stale: false });
      }
      if (command === "restore_material_revision_cmd") {
        return Promise.resolve({ document: { ...baseDocument, current_revision: 2 }, impact: noImpact });
      }
      return Promise.resolve(null);
    });
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: /添加段落/ }));
    await user.click(screen.getByRole("button", { name: /版本历史/ }));
    await user.click(await screen.findByRole("button", { name: /修改 1/ }));
    await user.click(await screen.findByRole("button", { name: "恢复此版本" }));
    await user.click(await screen.findByRole("button", { name: "保留草稿后恢复" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("restore_material_revision_cmd", expect.objectContaining({
      materialId: "material-1",
      payload: expect.objectContaining({ preserve_draft: true }),
    })));
    expect(invokeMock.mock.calls.filter(([command]) => command === "save_material_draft_cmd")).toHaveLength(1);
  });

  it("shows a revision conflict without overwriting the remote document", async () => {
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_material_document_cmd") return Promise.resolve(baseDocument);
      if (command === "get_material_draft_cmd") return Promise.resolve(null);
      if (command === "save_material_draft_cmd") return Promise.resolve(null);
      if (command === "preview_material_edit_cmd") return Promise.reject({ status: 409, code: "material_revision_conflict", message: "revision changed" });
      return Promise.resolve(null);
    });
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: /添加段落/ }));
    await user.click(screen.getByRole("button", { name: "保存" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("其他窗口更新");
    expect(screen.getByRole("button", { name: "重新载入" })).toBeVisible();
    expect(invokeMock).not.toHaveBeenCalledWith("commit_material_edit_cmd", expect.anything());
  });

  it("keeps the draft and returns to editing when the preview impact changed", async () => {
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === "get_material_document_cmd") return Promise.resolve(baseDocument);
      if (command === "get_material_draft_cmd") return Promise.resolve(null);
      if (command === "preview_material_edit_cmd") return Promise.resolve({
        base_revision: 1,
        next_revision: 2,
        content_sha256: "b".repeat(64),
        preview_token: "stale-preview",
        impact: noImpact,
      });
      if (command === "commit_material_edit_cmd") return Promise.reject({ status: 409, code: "document_preview_changed", message: "impact changed" });
      if (command === "save_material_draft_cmd") {
        const request = args?.payload as { base_revision: number; blocks: MaterialDocument["blocks"] };
        return Promise.resolve({ material_id: "material-1", ...request, updated_at: "2026-07-15T12:00:00Z", is_stale: false });
      }
      return Promise.resolve(null);
    });
    render(<MaterialDocumentEditor materialId="material-1" title="HIF review" onCancel={vi.fn()} />);
    const user = userEvent.setup();

    await screen.findByTestId("material-block-editor");
    await user.click(screen.getByRole("button", { name: /添加段落/ }));
    await user.click(screen.getByRole("button", { name: "保存" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("编辑影响已变化");
    expect(screen.getByRole("button", { name: "保存" })).toBeEnabled();
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("save_material_draft_cmd", expect.objectContaining({ materialId: "material-1" })));
  });
});
