import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { DEFAULT_MATERIAL_FILTERS } from "../../features/materials/types";
import type { Article } from "../../types";
import { MaterialsWorkbenchPage } from "./MaterialsWorkbenchPage";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));

const article: Article = {
  id: "article-1",
  title: "Reading One",
  content: "content",
  source_type: "article",
  created_at: "2026-07-14T00:00:00Z",
  translated: false,
  segments: [],
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((command: string) => {
    if (command === "material_library_list_tags_cmd") return Promise.resolve([{ id: "tag-1", name: "Research", material_count: 1 }]);
    if (command === "material_library_list_import_jobs_cmd") return Promise.resolve([]);
    return Promise.resolve(null);
  });
});

describe("MaterialsWorkbenchPage", () => {
  it("separates materials, tags, and import jobs while delegating filters", async () => {
    const onFiltersChange = vi.fn();
    render(<MaterialsWorkbenchPage
      articles={[article]}
      isLoading={false}
      filters={DEFAULT_MATERIAL_FILTERS}
      viewMode="card"
      initialScrollTop={0}
      onScrollTopChange={vi.fn()}
      onFiltersChange={onFiltersChange}
      onViewModeChange={vi.fn()}
      onSelectArticle={vi.fn()}
      onDeleteArticle={vi.fn()}
      onEditArticle={vi.fn()}
      onNewMaterial={vi.fn()}
      onArticleUpdate={vi.fn()}
      onRefresh={vi.fn().mockResolvedValue([article])}
    />);

    expect(screen.getByRole("heading", { name: "素材库" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "继续阅读" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox", { name: "搜索素材" }), { target: { value: "reading" } });
    expect(onFiltersChange).toHaveBeenCalledWith(expect.objectContaining({ query: "reading" }));

    await userEvent.click(screen.getByRole("tab", { name: /标签/ }));
    await waitFor(() => expect(screen.getByRole("region", { name: "标签管理" })).toHaveTextContent("Research"));
    await userEvent.click(screen.getByRole("tab", { name: /导入任务/ }));
    expect(screen.getByRole("region", { name: "最近导入任务" })).toBeInTheDocument();
  });
});
