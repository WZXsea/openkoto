import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type {
  BulkOrganizeLearningItemInput,
  BulkOrganizeLearningItemsResponse,
  LearningWorkbenchApi,
  LearningWorkbenchItem,
} from "../../features/learning";
import { LearningWorkbench } from "./LearningWorkbench";

function createItem(overrides: Partial<LearningWorkbenchItem> = {}): LearningWorkbenchItem {
  return {
    id: "item-1",
    material_id: "material-1",
    segment_id: "segment-7",
    item_type: "word",
    text: "mitigate",
    source_sentence: "Early action can mitigate treatment-related risk.",
    context_before: "The protocol prioritizes prevention.",
    context_after: "Monitoring then continues weekly.",
    meaning_in_context: "reduce the severity of risk",
    definition_en: "make less severe",
    definition_zh: "减轻",
    collocations: [],
    examples: [],
    tags: ["oncology", "academic"],
    quality_flags: ["needs_verification"],
    status: "candidate",
    priority: 0,
    difficulty: null,
    ai_explanation: null,
    review_state: {},
    source_material_title: "Clinical trial protocol",
    source_type: "article",
    source_segment_order: 7,
    accepted_at: null,
    rejected_at: null,
    created_at: "2026-07-14T00:00:00Z",
    updated_at: "2026-07-14T00:00:00Z",
    ...overrides,
  };
}

function createApi(items: LearningWorkbenchItem[] = [createItem()]): LearningWorkbenchApi {
  return {
    list: vi.fn().mockResolvedValue(items),
    update: vi.fn().mockImplementation(async (id, patch) => ({
      ...items.find((item) => item.id === id)!,
      ...patch,
      updated_at: "2026-07-14T00:01:00Z",
    })),
    bulkOrganize: vi.fn().mockImplementation(async (changes: BulkOrganizeLearningItemInput[]) => ({
      succeeded: changes.length,
      failed: 0,
      results: changes.map((change) => ({
        id: change.id,
        success: true,
        item: { ...items.find((item) => item.id === change.id)!, ...change },
      })),
    })),
  };
}

afterEach(cleanup);

describe("LearningWorkbench", () => {
  it("focuses a linked learning item even when it is outside the default candidate filter", async () => {
    const candidate = createItem();
    const accepted = createItem({ id: "learning-linked", text: "linked evidence", status: "accepted" });
    const api = createApi([candidate, accepted]);
    api.get = vi.fn().mockResolvedValue(accepted);
    vi.mocked(api.list).mockImplementation(async (query) => query?.status === "accepted" ? [accepted] : [candidate]);

    render(
      <LearningWorkbench
        api={api}
        initialItemId="learning-linked"
        onNavigateToSource={() => undefined}
      />,
    );

    expect(await screen.findByDisplayValue("linked evidence")).toBeInTheDocument();
    expect(api.get).toHaveBeenCalledWith("learning-linked");
    await waitFor(() => expect(screen.getByLabelText("筛选状态")).toHaveValue("accepted"));
  });

  it("loads candidates, filters through the API, and preserves source evidence navigation", async () => {
    const api = createApi();
    const onNavigateToSource = vi.fn();
    render(
      <LearningWorkbench
        api={api}
        materials={[{ id: "material-1", title: "Clinical trial protocol", sourceType: "article" }]}
        onNavigateToSource={onNavigateToSource}
      />,
    );

    expect(await screen.findByText("mitigate")).toBeInTheDocument();
    await waitFor(() => expect(screen.getAllByText("Early action can mitigate treatment-related risk.")).toHaveLength(2));
    expect(screen.getByText(/The protocol prioritizes prevention/)).toBeInTheDocument();
    expect(screen.getByText(/Monitoring then continues weekly/)).toBeInTheDocument();
    expect(screen.getAllByText("待人工核验").length).toBeGreaterThan(0);

    await userEvent.selectOptions(screen.getByLabelText("筛选状态"), "all");
    await userEvent.selectOptions(screen.getByLabelText("筛选类型"), "word");
    await userEvent.selectOptions(screen.getByLabelText("筛选来源素材"), "material-1");
    await userEvent.selectOptions(screen.getByLabelText("筛选来源类型"), "article");
    await userEvent.selectOptions(screen.getByLabelText("筛选标签"), "oncology");
    await userEvent.selectOptions(screen.getByLabelText("筛选质量标记"), "needs_verification");

    await waitFor(() => {
      expect(api.list).toHaveBeenLastCalledWith({
        status: undefined,
        item_type: "word",
        material_id: "material-1",
        source_type: "article",
        tag: "oncology",
        quality_flag: "needs_verification",
        limit: 300,
        offset: 0,
      });
    });

    await userEvent.click(screen.getByRole("button", { name: "返回原文" }));
    expect(onNavigateToSource).toHaveBeenCalledWith(expect.objectContaining({ id: "item-1", material_id: "material-1" }));
  });

  it("edits a candidate without dropping non-editable quality flags", async () => {
    const item = createItem({ quality_flags: ["needs_verification", "insufficient_context"] });
    const api = createApi([item]);
    render(<LearningWorkbench api={api} onNavigateToSource={() => {}} />);

    const textInput = await screen.findByLabelText("编辑学习内容");
    await userEvent.clear(textInput);
    await userEvent.type(textInput, "mitigation");
    await userEvent.clear(screen.getByLabelText("编辑候选项标签"));
    await userEvent.type(screen.getByLabelText("编辑候选项标签"), "medical, verified, medical");
    await userEvent.click(screen.getByLabelText("待人工核验"));
    await userEvent.click(screen.getByRole("button", { name: "保存候选项" }));

    await waitFor(() => {
      expect(api.update).toHaveBeenCalledWith("item-1", expect.objectContaining({
        text: "mitigation",
        tags: ["medical", "verified"],
        quality_flags: ["insufficient_context"],
      }));
    });
  });

  it("reports per-item failures from batch acceptance and keeps failed items selected", async () => {
    const word = createItem();
    const grammar = createItem({ id: "item-2", item_type: "grammar", text: "subjunctive" });
    const api = createApi([word, grammar]);
    vi.mocked(api.list)
      .mockResolvedValueOnce([word, grammar])
      .mockResolvedValue([grammar]);
    vi.mocked(api.bulkOrganize).mockResolvedValueOnce({
      succeeded: 1,
      failed: 1,
      results: [
        { id: "item-1", success: true, item: { ...word, status: "accepted" } },
        { id: "item-2", success: false, error: { code: "invalid_transition", message: "该语法项缺少必要上下文" } },
      ],
    } satisfies BulkOrganizeLearningItemsResponse);
    render(<LearningWorkbench api={api} onNavigateToSource={() => {}} />);

    await screen.findByText("subjunctive");
    await userEvent.click(screen.getByLabelText("选择全部当前候选项"));
    await userEvent.click(screen.getByRole("button", { name: "批量接受" }));

    await waitFor(() => expect(api.bulkOrganize).toHaveBeenCalledWith([
      { id: "item-1", status: "accepted", favorite_type: "vocabulary", pack_ids: [] },
      { id: "item-2", status: "accepted", favorite_type: "grammar", pack_ids: [] },
    ]));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("批量操作完成：1 项成功，1 项失败");
    expect(alert).toHaveTextContent("该语法项缺少必要上下文");
    expect(screen.getByLabelText("选择 subjunctive")).toBeChecked();
  });

  it("supports reject, archive, state-aware restore, tags, and verification organization", async () => {
    let current = [
      createItem({ id: "item-1", status: "candidate", quality_flags: ["insufficient_context"] }),
      createItem({ id: "item-2", text: "hazard ratio", status: "candidate", quality_flags: [] }),
    ];
    const api = createApi(current);
    vi.mocked(api.list).mockImplementation(async () => current);
    vi.mocked(api.bulkOrganize).mockImplementation(async (changes) => {
      current = current.map((item) => {
        const change = changes.find((entry) => entry.id === item.id);
        if (!change) return item;
        const statusBeforeArchive = change.status === "archived"
          ? item.status as LearningWorkbenchItem["status_before_archive"]
          : item.status_before_archive;
        return { ...item, ...change, status_before_archive: statusBeforeArchive };
      });
      return {
        succeeded: changes.length,
        failed: 0,
        results: changes.map((change) => ({ id: change.id, success: true, item: current.find((item) => item.id === change.id) })),
      };
    });
    render(<LearningWorkbench api={api} onNavigateToSource={() => {}} />);
    await screen.findByText("hazard ratio");

    const selectAll = () => userEvent.click(screen.getByLabelText("选择全部当前候选项"));
    await selectAll();
    await userEvent.click(screen.getByRole("button", { name: "批量拒绝" }));
    await waitFor(() => expect(api.bulkOrganize).toHaveBeenCalledTimes(1));
    expect(vi.mocked(api.bulkOrganize).mock.calls[0][0]).toEqual([
      { id: "item-1", status: "rejected" },
      { id: "item-2", status: "rejected" },
    ]);

    await selectAll();
    await userEvent.click(screen.getByRole("button", { name: "批量归档" }));
    await waitFor(() => expect(api.bulkOrganize).toHaveBeenCalledTimes(2));
    await selectAll();
    await userEvent.click(screen.getByRole("button", { name: "批量恢复" }));
    await waitFor(() => expect(api.bulkOrganize).toHaveBeenCalledTimes(3));
    expect(vi.mocked(api.bulkOrganize).mock.calls[2][0]).toEqual([
      { id: "item-1", status: "rejected" },
      { id: "item-2", status: "rejected" },
    ]);

    await selectAll();
    await userEvent.click(screen.getByRole("button", { name: "标记待核验" }));
    await waitFor(() => expect(api.bulkOrganize).toHaveBeenCalledTimes(4));
    expect(vi.mocked(api.bulkOrganize).mock.calls[3][0]).toEqual([
      { id: "item-1", quality_flags: ["insufficient_context", "needs_verification"] },
      { id: "item-2", quality_flags: ["needs_verification"] },
    ]);

    await selectAll();
    const tagInput = screen.getByLabelText("批量标签");
    await userEvent.type(tagInput, "professional, review, professional");
    await userEvent.click(screen.getByRole("button", { name: "设置标签" }));
    await waitFor(() => expect(api.bulkOrganize).toHaveBeenCalledTimes(5));
    expect(vi.mocked(api.bulkOrganize).mock.calls[4][0]).toEqual([
      { id: "item-1", tags: ["professional", "review"] },
      { id: "item-2", tags: ["professional", "review"] },
    ]);
  });

  it("merges selected duplicates into the active canonical item", async () => {
    const api = createApi([
      createItem({ id: "item-1" }),
      createItem({ id: "item-2", text: "mitigation" }),
    ]);
    render(<LearningWorkbench api={api} onNavigateToSource={() => {}} />);

    await screen.findByText("mitigation");
    await userEvent.click(screen.getByLabelText("选择全部当前候选项"));
    await userEvent.click(screen.getByRole("button", { name: "合并到当前项" }));

    await waitFor(() => expect(api.bulkOrganize).toHaveBeenCalledWith([
      { id: "item-2", merge_into_id: "item-1" },
    ]));
  });

  it("shows local review, records preview activity, and checks legacy migration", async () => {
    const api: LearningWorkbenchApi = {
      ...createApi([createItem({ status: "accepted" })]),
      getDailyReview: vi.fn().mockResolvedValue({
        date: "2026-07-14",
        material_id: null,
        total_events: 3,
        event_counts: { read: 1, accept: 1, local_preview: 1 },
        unique_learning_items: 1,
        events: [],
      }),
      recordLocalPreview: vi.fn().mockResolvedValue({
        id: "event-1",
        learning_item_id: "item-1",
        material_id: "material-1",
        event_type: "local_preview",
        metadata: {},
        occurred_at: "2026-07-14T00:00:00Z",
      }),
      migrateLegacy: vi.fn().mockResolvedValue({
        dry_run: true,
        planned: 2,
        migrated: 0,
        already_migrated: 1,
        conflicts: [],
      }),
    };
    render(<LearningWorkbench api={api} onNavigateToSource={() => {}} />);

    expect(await screen.findByText("活动 3")).toBeInTheDocument();
    await screen.findByLabelText("编辑学习内容");
    await userEvent.click(screen.getByRole("button", { name: "本地预习" }));
    await waitFor(() => expect(api.recordLocalPreview).toHaveBeenCalledWith("item-1", { origin: "learning_workbench" }));
    await userEvent.click(screen.getByRole("button", { name: "兼容数据检查" }));
    await waitFor(() => expect(api.migrateLegacy).toHaveBeenCalledWith(true));
    expect(await screen.findByText(/计划 2，完成 0，冲突 0/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "执行兼容迁移" })).toBeInTheDocument();
  });

  it("renders loading, empty, and recoverable error states", async () => {
    let resolveFirst!: (items: LearningWorkbenchItem[]) => void;
    const pending = new Promise<LearningWorkbenchItem[]>((resolve) => { resolveFirst = resolve; });
    const api = createApi([]);
    vi.mocked(api.list).mockReturnValueOnce(pending).mockRejectedValueOnce(new Error("backend offline")).mockResolvedValueOnce([]);
    render(<LearningWorkbench api={api} onNavigateToSource={() => {}} />);

    expect(screen.getByText("正在加载候选项")).toBeInTheDocument();
    resolveFirst([]);
    expect(await screen.findByText("暂无匹配的学习项")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "刷新候选项" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("backend offline");
    await userEvent.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => expect(api.list).toHaveBeenCalledTimes(3));
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
