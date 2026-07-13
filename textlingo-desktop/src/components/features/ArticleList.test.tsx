import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { Article } from "../../types";
import { ArticleList } from "./ArticleList";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: string | { defaultValue?: string }) => typeof options === "string"
      ? options
      : options?.defaultValue || key,
  }),
}));

const articles: Article[] = [
  {
    id: "current",
    title: "Current reading",
    content: "A current material.",
    source_type: "web",
    created_at: "2026-07-01T08:00:00Z",
    translated: false,
    tags: ["work"],
    reading_progress: { progress_ratio: 0.4, status: "reading", last_opened_at: "2026-07-08T08:00:00Z" },
  } as Article,
  {
    id: "complete",
    title: "Completed material",
    content: "An archived resource.",
    source_type: "book",
    created_at: "2026-07-02T08:00:00Z",
    translated: true,
    tags: ["reference"],
    reading_progress: { progress_ratio: 1, completed_at: "2026-07-09T08:00:00Z" },
  } as Article,
];

function renderList(overrides: Partial<React.ComponentProps<typeof ArticleList>> = {}) {
  const onDelete = vi.fn().mockResolvedValue(undefined);
  const onBulkArchive = vi.fn().mockResolvedValue(undefined);
  const onSelectArticle = vi.fn();
  const onUpdate = vi.fn();
  render(<ArticleList
    articles={articles}
    isLoading={false}
    onSelectArticle={onSelectArticle}
    onDelete={onDelete}
    onBulkArchive={onBulkArchive}
    onEdit={() => {}}
    viewMode="list"
    onUpdate={onUpdate}
    {...overrides}
  />);
  return { onDelete, onBulkArchive, onSelectArticle, onUpdate };
}

describe("ArticleList material workbench", () => {
  afterEach(cleanup);

  it("supports batch selection and delegates archive through the batch callback", async () => {
    const { onBulkArchive } = renderList();
    const user = userEvent.setup();

    await user.click(screen.getByLabelText("选择 Current reading"));
    expect(screen.getByText("已选择 1 项")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "归档" }));
    await user.click(within(screen.getByText("将归档 1 项素材。").parentElement!).getByRole("button", { name: "归档" }));

    expect(onBulkArchive).toHaveBeenCalledWith(["current"]);
  });

  it("renders a stable empty state", () => {
    renderList({ articles: [] });
    expect(screen.getByText("暂无素材")).toBeInTheDocument();
    expect(screen.getByText("导入或新建第一份阅读素材")).toBeInTheDocument();
  });

  it("shows no-results state for search and clears the active filter", async () => {
    renderList();
    const user = userEvent.setup();
    const search = screen.getByLabelText("搜索素材");

    await user.type(search, "missing material");
    expect(screen.getByText("没有匹配的素材")).toBeInTheDocument();

    await user.click(screen.getAllByRole("button", { name: "清除筛选" })[1]);
    expect(search).toHaveValue("");
    expect(screen.getAllByText("Current reading")).toHaveLength(2);
  });

  it("filters materials by inclusive import dates", async () => {
    renderList();
    const allMaterials = within(screen.getByRole("region", { name: "全部素材" }));

    fireEvent.change(screen.getByLabelText("导入日期从"), { target: { value: "2026-07-02" } });
    expect(allMaterials.queryByText("Current reading")).not.toBeInTheDocument();
    expect(allMaterials.getByText("Completed material")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("导入日期从"), { target: { value: "" } });
    fireEvent.change(screen.getByLabelText("导入日期至"), { target: { value: "2026-07-01" } });
    expect(allMaterials.getByText("Current reading")).toBeInTheDocument();
    expect(allMaterials.queryByText("Completed material")).not.toBeInTheDocument();
  });

  it("renders distinct list and compact card layouts", () => {
    const { unmount } = render(<ArticleList
      articles={articles}
      isLoading={false}
      onSelectArticle={() => {}}
      onDelete={vi.fn().mockResolvedValue(undefined)}
      onEdit={() => {}}
      viewMode="list"
    />);
    expect(screen.getByTestId("material-list-layout")).toBeInTheDocument();
    expect(screen.queryByTestId("material-card-layout")).not.toBeInTheDocument();
    unmount();

    render(<ArticleList
      articles={articles}
      isLoading={false}
      onSelectArticle={() => {}}
      onDelete={vi.fn().mockResolvedValue(undefined)}
      onEdit={() => {}}
      viewMode="card"
    />);
    expect(screen.getByTestId("material-card-layout")).toHaveClass("sm:grid-cols-2", "xl:grid-cols-3");
    expect(screen.queryByTestId("material-list-layout")).not.toBeInTheDocument();
  });

  it("runs media maintenance commands and refreshes after completion", async () => {
    const { invoke } = await import("@tauri-apps/api/core");
    const onUpdate = vi.fn();
    const user = userEvent.setup();
    renderList({
      articles: [{
        ...articles[0],
        id: "video",
        title: "Recorded lesson",
        source_type: "local_video",
        media_path: "/tmp/lesson.mp4",
      } as Article],
      onUpdate,
    });

    await user.click(screen.getByRole("button", { name: "操作 Recorded lesson" }));
    await user.click(screen.getByText("删除字幕"));

    expect(invoke).toHaveBeenCalledWith("delete_article_subtitles_cmd", { id: "video" });
    expect(onUpdate).toHaveBeenCalledTimes(1);

    await user.click(screen.getByRole("button", { name: "操作 Recorded lesson" }));
    await user.click(screen.getByText("删除翻译解析"));

    expect(invoke).toHaveBeenCalledWith("delete_article_analysis_cmd", { id: "video" });
    expect(onUpdate).toHaveBeenCalledTimes(2);
  });

  it("keeps long narrow-screen content accessible and selection separate from opening", async () => {
    const longTitle = "A very long material title that must remain readable in a narrow workbench layout";
    const longUrl = "https://example.com/a/very/long/source/path/that/should/wrap/instead/of/overflowing/the/card-layout";
    const { onSelectArticle } = renderList({
      viewMode: "card",
      articles: [{ ...articles[0], title: longTitle, source_url: longUrl, source_name: undefined, tags: ["an-extremely-long-tag-that-must-wrap-on-narrow-screens"] } as Article],
    });
    const user = userEvent.setup();

    const cardLayout = within(screen.getByTestId("material-card-layout"));
    expect(cardLayout.getByText(longTitle)).toHaveClass("break-words");
    expect(cardLayout.getByText(longUrl)).toHaveClass("break-all");
    await user.click(screen.getByLabelText(`选择 ${longTitle}`));
    expect(onSelectArticle).not.toHaveBeenCalled();
    expect(cardLayout.getByText("an-extremely-long-tag-that-must-wrap-on-narrow-screens")).toHaveClass("break-all");
  });
});
