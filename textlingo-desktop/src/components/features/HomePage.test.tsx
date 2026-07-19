import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import type { HomeActivityApi, LearningActivityHeatmap } from "../../features/home";
import type { Article } from "../../types";
import { HomePage } from "./HomePage";

const EMPTY_HEATMAP: LearningActivityHeatmap = {
  start_date: "2026-04-22",
  end_date: "2026-07-14",
  days: Array.from({ length: 84 }, (_, index) => ({
    date: new Date(Date.UTC(2026, 3, 22 + index)).toISOString().slice(0, 10),
    read_materials: 0,
    learning_actions: 0,
    activity_score: 0,
  })),
};

function article(id: string, title: string, options: Partial<Article> = {}): Article {
  return {
    id,
    title,
    content: `${title} content`,
    source_type: "article",
    created_at: "2026-07-14T08:00:00Z",
    translated: false,
    segments: [],
    ...options,
  };
}

function renderHome(articles: Article[], activityApi: HomeActivityApi = { getActivityHeatmap: vi.fn().mockResolvedValue(EMPTY_HEATMAP) }) {
  const actions = {
    onSelectArticle: vi.fn(),
    onNewMaterial: vi.fn(),
    onOpenMaterials: vi.fn(),
    onOpenLearning: vi.fn(),
  };
  render(<HomePage articles={articles} activityApi={activityApi} {...actions} />);
  return actions;
}

describe("HomePage", () => {
  it("offers first import when the material library is empty", async () => {
    const actions = renderHome([]);
    await userEvent.click(screen.getByRole("button", { name: "导入第一份素材" }));
    expect(actions.onNewMaterial).toHaveBeenCalledOnce();
    expect(screen.getByRole("heading", { name: "建立你的阅读素材库" })).toBeInTheDocument();
  });

  it("prioritizes in-progress reading and excludes it from recent materials", async () => {
    const inProgress = article("reading", "Clinical Reading", {
      reading_progress: { reader_kind: "article", locator: {}, progress_ratio: 0.42, status: "reading", last_opened_at: "2026-07-14T10:00:00Z" },
    });
    const recent = article("recent", "Daily Notes", { created_at: "2026-07-13T08:00:00Z" });
    const actions = renderHome([recent, inProgress]);

    const primary = screen.getByRole("region", { name: "Clinical Reading" });
    expect(within(primary).getByText("42%")).toBeInTheDocument();
    expect(screen.getAllByText("Clinical Reading")).toHaveLength(1);
    expect(screen.getByText("Daily Notes")).toBeInTheDocument();
    await userEvent.click(within(primary).getByRole("button", { name: "继续阅读" }));
    expect(actions.onSelectArticle).toHaveBeenCalledWith(inProgress);
  });

  it("keeps reading actions usable when activity loading fails", async () => {
    const actions = renderHome([article("one", "Readable Material")], {
      getActivityHeatmap: vi.fn().mockRejectedValue(new Error("offline")),
    });
    expect(await screen.findByText("活动摘要暂时无法读取，不影响阅读。")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "开始阅读" }));
    expect(actions.onSelectArticle).toHaveBeenCalledOnce();
  });

  it("requests an 84-day heatmap with the local timezone", async () => {
    const getActivityHeatmap = vi.fn().mockResolvedValue(EMPTY_HEATMAP);
    renderHome([], { getActivityHeatmap });
    await waitFor(() => expect(getActivityHeatmap).toHaveBeenCalledOnce());
    const query = getActivityHeatmap.mock.calls[0][0];
    const span = (Date.parse(`${query.end_date}T00:00:00Z`) - Date.parse(`${query.start_date}T00:00:00Z`)) / 86_400_000;
    expect(span).toBe(83);
    expect(query.timezone_offset_minutes).toBe(-new Date().getTimezoneOffset());
    await waitFor(() => expect(screen.getAllByRole("list", { name: "近12周学习活跃度" }).length).toBeGreaterThan(0));
    const heatmaps = screen.getAllByRole("list", { name: "近12周学习活跃度" });
    const heatmap = heatmaps[heatmaps.length - 1];
    expect(within(heatmap).getAllByRole("listitem")).toHaveLength(84);
    expect(within(heatmap).queryByRole("button")).not.toBeInTheDocument();
  });
});
