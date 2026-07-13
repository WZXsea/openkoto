import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { MaterialTagsPanel } from "./MaterialTagsPanel";
import type { ManagedMaterialTag, MaterialTagsApi } from "./materialManagement";

const tags: ManagedMaterialTag[] = [
  { id: "clinical", name: "Clinical", color: "#2563eb", materialCount: 3 },
  { id: "writing", name: "Writing", color: "#16a34a", materialCount: 1 },
];

function createApi(): MaterialTagsApi {
  return {
    createTag: vi.fn().mockResolvedValue(undefined),
    renameTag: vi.fn().mockResolvedValue(undefined),
    deleteTag: vi.fn().mockResolvedValue(undefined),
    mergeTags: vi.fn().mockResolvedValue(undefined),
    applyTags: vi.fn().mockResolvedValue(undefined),
  };
}

function renderPanel(api = createApi(), selectedMaterialIds = ["material-1", "material-2"]) {
  render(<MaterialTagsPanel tags={tags} selectedMaterialIds={selectedMaterialIds} api={api} />);
  return api;
}

describe("MaterialTagsPanel", () => {
  afterEach(cleanup);

  it("creates and renames tags through the injected API", async () => {
    const api = renderPanel();
    const user = userEvent.setup();

    await user.type(screen.getByLabelText("新标签名称"), "Research");
    await user.click(screen.getByRole("button", { name: "创建" }));
    expect(api.createTag).toHaveBeenCalledWith({ name: "Research", color: "#2563eb" });

    await user.click(screen.getByRole("button", { name: "重命名 Clinical" }));
    const rename = screen.getByLabelText("重命名 Clinical");
    await user.clear(rename);
    await user.type(rename, "Medicine");
    await user.click(screen.getByRole("button", { name: "保存 Clinical" }));
    expect(api.renameTag).toHaveBeenCalledWith({ tagId: "clinical", name: "Medicine", color: "#2563eb" });
  });

  it("confirms delete and merge operations", async () => {
    const api = renderPanel();
    const user = userEvent.setup();

    await user.selectOptions(screen.getByLabelText("合并 Clinical 到"), "writing");
    await user.click(screen.getByRole("button", { name: "合并 Clinical" }));
    expect(screen.getByRole("alertdialog", { name: "合并标签" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "确认合并" }));
    expect(api.mergeTags).toHaveBeenCalledWith({ sourceTagId: "clinical", targetTagId: "writing" });

    await user.click(screen.getByRole("button", { name: "删除 Writing" }));
    await user.click(screen.getByRole("button", { name: "确认删除" }));
    expect(api.deleteTag).toHaveBeenCalledWith("writing");
  });

  it("applies selected tags to all selected materials with add, remove, and replace modes", async () => {
    const api = renderPanel();
    const user = userEvent.setup();

    await user.click(screen.getByLabelText("批量选择 Clinical"));
    for (const mode of ["add", "remove", "replace"] as const) {
      await user.selectOptions(screen.getByLabelText("批量标签操作"), mode);
      await user.click(screen.getByRole("button", { name: "应用到 2 项" }));
      await user.click(screen.getByRole("button", { name: "确认应用" }));
      expect(api.applyTags).toHaveBeenLastCalledWith({ materialIds: ["material-1", "material-2"], tagIds: ["clinical"], mode });
      if (mode !== "replace") await user.click(screen.getByLabelText("批量选择 Clinical"));
    }
  });
});
