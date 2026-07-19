import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { NewMaterialDialog } from "./NewMaterialDialog";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(),
}));

describe("NewMaterialDialog import lifecycle", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  afterEach(cleanup);

  it("cannot close or switch tabs while a confirmed import is committing", async () => {
    let resolveCommit!: (value: { id: string }) => void;
    invokeMock.mockImplementation((command: string) => {
      if (command === "get_config") return Promise.resolve(null);
      if (command === "preview_material_import_cmd") {
        return Promise.resolve({
          title: "Lifecycle lesson",
          content_snippet: "Lifecycle test content.",
          file: {},
          paragraph_count: 1,
          duplicates: { duplicate: false, matches: [] },
          job: { id: "job-lifecycle" },
        });
      }
      if (command === "create_article") {
        return new Promise((resolve) => { resolveCommit = resolve; });
      }
      return Promise.resolve(null);
    });
    const onClose = vi.fn();
    const onSave = vi.fn();
    const user = userEvent.setup();
    render(<NewMaterialDialog isOpen onClose={onClose} onSave={onSave} />);

    await user.type(screen.getByPlaceholderText("newArticle.titlePlaceholder"), "Lifecycle lesson");
    await user.type(screen.getByPlaceholderText("newArticle.contentPlaceholder"), "Lifecycle test content.");
    await user.click(screen.getByRole("button", { name: "newArticle.createArticle" }));
    await user.click(await screen.findByRole("button", { name: "确认并导入" }));

    expect(screen.getByRole("button", { name: "文本文件" })).toBeDisabled();
    expect(screen.getAllByRole("button", { name: "Close" }).every((button) => button.hasAttribute("disabled"))).toBe(true);
    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalledWith("material_library_cancel_import_job_cmd", expect.anything());

    await act(async () => {
      resolveCommit({ id: "material-lifecycle" });
    });
    expect(onSave).toHaveBeenCalledOnce();
    expect(onClose).toHaveBeenCalledOnce();
  });
});
